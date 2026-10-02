//! Additive M5 checkpoint/replay API. All mutation is caller-serialized.
use super::{
    INVALID_ARGUMENT, OK,
    game::{GrazerGame, HostedGame, copy},
    guard,
};
use crate::{
    Game, Stage,
    checkpoint::{CheckpointError, MAX_CHECKPOINT_BYTES},
    game::{
        DemoStage,
        checkpoint::CheckpointInfo,
        replay::{GameReplay, ReplayError, ReplayPlayer},
    },
    language::ScriptStage,
    resources::ResourcePack,
};
use std::sync::Arc;
pub const REPLAY_DATA_ERROR: i32 = 7;
pub const REPLAY_INCOMPATIBLE: i32 = 8;
pub const REPLAY_DIVERGED: i32 = 9;
fn code(error: &ReplayError) -> i32 {
    match error {
        ReplayError::Checkpoint(
            CheckpointError::Resource | CheckpointError::Stage | CheckpointError::Version,
        ) => REPLAY_INCOMPATIBLE,
        ReplayError::Diverged(_) | ReplayError::Stopped => REPLAY_DIVERGED,
        _ => REPLAY_DATA_ERROR,
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn grazer_checkpoint_api_version() -> u32 {
    1
}
/// # Safety
/// Same live-handle/bulk output/alignment/nonaliasing contract as game_snapshot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_game_checkpoint(
    game: *const GrazerGame,
    out: *mut u8,
    capacity: u32,
    required: *mut u32,
) -> i32 {
    guard(|| {
        // SAFETY: caller provides a readable live handle or null.
        let Some(g) = (unsafe { game.as_ref() }) else {
            return INVALID_ARGUMENT;
        };
        let bytes = match &g.inner {
            HostedGame::Native(g) => g.checkpoint(),
            HostedGame::Script(g) => g.checkpoint(),
        };
        let bytes = match bytes {
            Ok(b) => b,
            Err(e) => return code(&ReplayError::Checkpoint(e)),
        };
        // SAFETY: caller provides the documented writable, nonoverlapping buffers.
        unsafe { copy(bytes.iter().copied(), bytes.len(), out, capacity, required) }
    })
}
/// # Safety
/// Bytes has length readable bytes; out is aligned/writable/nonoverlapping.
/// Restores against the builtin resource pack, as do existing C constructors.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_game_restore_checkpoint(
    bytes: *const u8,
    length: u32,
    out: *mut *mut GrazerGame,
) -> i32 {
    guard(|| {
        if out.is_null() {
            return INVALID_ARGUMENT;
        }
        // SAFETY: caller supplies writable handle output storage.
        unsafe {
            *out = std::ptr::null_mut();
        }
        if bytes.is_null() || length as usize > MAX_CHECKPOINT_BYTES {
            return INVALID_ARGUMENT;
        }
        // SAFETY: caller guarantees length readable source bytes.
        let bytes = unsafe { std::slice::from_raw_parts(bytes, length as usize) };
        let info = match CheckpointInfo::inspect(bytes) {
            Ok(i) => i,
            Err(e) => return code(&e.into()),
        };
        let pack = Arc::new(ResourcePack::builtin());
        let restored = if info.identity.stage_id == DemoStage::CONTENT_ID {
            Game::<DemoStage>::restore_checkpoint(pack, bytes)
                .map(|g| HostedGame::Native(Box::new(g)))
        } else if info.identity.stage_id == ScriptStage::CONTENT_ID {
            Game::<ScriptStage>::restore_checkpoint(pack, bytes)
                .map(|g| HostedGame::Script(Box::new(g)))
        } else {
            return REPLAY_INCOMPATIBLE;
        };
        match restored {
            Ok(inner) => {
                // SAFETY: successful output transfers unique Box ownership to caller.
                unsafe {
                    *out = Box::into_raw(Box::new(GrazerGame { inner }));
                }
                OK
            }
            Err(e) => code(&e.into()),
        }
    })
}
enum HostedPlayer {
    Native(Box<ReplayPlayer<DemoStage>>),
    Script(Box<ReplayPlayer<ScriptStage>>),
}
pub struct GrazerReplayPlayer {
    inner: HostedPlayer,
    error: Option<ReplayError>,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct GrazerReplayStatus {
    pub frame: u64,
    pub tick: u64,
    pub frames: u64,
    pub stopped: u32,
    pub component: u32,
    pub expected: u64,
    pub actual: u64,
}
/// # Safety
/// Source/output have valid aligned nonoverlapping storage; source is borrowed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_replay_load(
    bytes: *const u8,
    length: u32,
    out: *mut *mut GrazerReplayPlayer,
) -> i32 {
    guard(|| {
        if out.is_null() {
            return INVALID_ARGUMENT;
        }
        // SAFETY: caller supplies writable handle output storage.
        unsafe {
            *out = std::ptr::null_mut();
        }
        if bytes.is_null() || length as usize > crate::game::replay::MAX_REPLAY_BYTES {
            return INVALID_ARGUMENT;
        }
        // SAFETY: caller guarantees length readable bytes.
        let bytes = unsafe { std::slice::from_raw_parts(bytes, length as usize) };
        let replay = match GameReplay::from_bytes(bytes) {
            Ok(r) => Arc::new(r),
            Err(e) => return code(&e),
        };
        let pack = Arc::new(ResourcePack::builtin());
        let player = if replay.metadata().identity.stage_id == DemoStage::CONTENT_ID {
            ReplayPlayer::<DemoStage>::new(replay, pack).map(|p| HostedPlayer::Native(Box::new(p)))
        } else if replay.metadata().identity.stage_id == ScriptStage::CONTENT_ID {
            ReplayPlayer::<ScriptStage>::new(replay, pack)
                .map(|p| HostedPlayer::Script(Box::new(p)))
        } else {
            return REPLAY_INCOMPATIBLE;
        };
        match player {
            Ok(inner) => {
                // SAFETY: caller owns the successful uniquely allocated handle.
                unsafe {
                    *out = Box::into_raw(Box::new(GrazerReplayPlayer { inner, error: None }));
                }
                OK
            }
            Err(e) => code(&e),
        }
    })
}
/// # Safety
/// Null or a live uniquely owned handle; destroy exactly once, no concurrent calls.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_replay_destroy(player: *mut GrazerReplayPlayer) {
    let _ = guard(|| {
        if !player.is_null() {
            // SAFETY: caller transfers unique ownership of a live Box handle.
            unsafe {
                drop(Box::from_raw(player));
            }
        }
        OK
    });
}
/// # Safety
/// Live writable handle and nonoverlapping writable advanced output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_replay_step(
    player: *mut GrazerReplayPlayer,
    advanced: *mut u32,
) -> i32 {
    guard(|| {
        if advanced.is_null() {
            return INVALID_ARGUMENT;
        }
        // SAFETY: caller supplies writable live handle or null.
        let Some(p) = (unsafe { player.as_mut() }) else {
            return INVALID_ARGUMENT;
        };
        // SAFETY: advanced is valid writable output storage.
        unsafe {
            *advanced = 0;
        }
        let result = match &mut p.inner {
            HostedPlayer::Native(p) => p.step(),
            HostedPlayer::Script(p) => p.step(),
        };
        match result {
            Ok(value) => {
                // SAFETY: advanced is valid writable output storage.
                unsafe {
                    *advanced = u32::from(value);
                }
                p.error = None;
                OK
            }
            Err(e) => {
                let c = code(&e);
                if !matches!(e, ReplayError::Stopped) || p.error.is_none() {
                    p.error = Some(e);
                }
                c
            }
        }
    })
}
/// # Safety
/// Live uniquely mutable handle; frame in 0..=recording length. Seek is atomic.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_replay_seek(player: *mut GrazerReplayPlayer, frame: u64) -> i32 {
    guard(|| {
        // SAFETY: caller supplies live mutable handle or null.
        let Some(p) = (unsafe { player.as_mut() }) else {
            return INVALID_ARGUMENT;
        };
        let Ok(frame) = usize::try_from(frame) else {
            return INVALID_ARGUMENT;
        };
        let result = match &mut p.inner {
            HostedPlayer::Native(p) => p.seek(frame),
            HostedPlayer::Script(p) => p.seek(frame),
        };
        match result {
            Ok(()) => {
                p.error = None;
                OK
            }
            Err(e) => {
                let c = code(&e);
                p.error = Some(e);
                c
            }
        }
    })
}
/// # Safety
/// Live readable handle and valid aligned nonoverlapping writable hash output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_replay_state_hash(
    player: *const GrazerReplayPlayer,
    out: *mut u64,
) -> i32 {
    guard(|| {
        if out.is_null() {
            return INVALID_ARGUMENT;
        }
        // SAFETY: caller supplies readable live handle or null.
        let Some(p) = (unsafe { player.as_ref() }) else {
            return INVALID_ARGUMENT;
        };
        let hash = match &p.inner {
            HostedPlayer::Native(p) => p.game().state_hash(),
            HostedPlayer::Script(p) => p.game().state_hash(),
        };
        // SAFETY: caller supplies writable hash output storage.
        unsafe {
            out.write(hash);
        }
        OK
    })
}
/// # Safety
/// Handle readable; output writable/aligned/nonoverlapping. No callbacks.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_replay_status(
    player: *const GrazerReplayPlayer,
    out: *mut GrazerReplayStatus,
) -> i32 {
    guard(|| {
        if out.is_null() {
            return INVALID_ARGUMENT;
        }
        // SAFETY: caller supplies readable live handle or null.
        let Some(p) = (unsafe { player.as_ref() }) else {
            return INVALID_ARGUMENT;
        };
        let (frame, tick, frames, stopped) = match &p.inner {
            HostedPlayer::Native(p) => (
                p.frame(),
                p.game().hud().tick,
                p.replay().frames().len(),
                p.last_error().is_some(),
            ),
            HostedPlayer::Script(p) => (
                p.frame(),
                p.game().hud().tick,
                p.replay().frames().len(),
                p.last_error().is_some(),
            ),
        };
        let mut status = GrazerReplayStatus {
            frame: frame as u64,
            tick,
            frames: frames as u64,
            stopped: u32::from(stopped),
            component: u32::MAX,
            expected: 0,
            actual: 0,
        };
        if let Some(ReplayError::Diverged(d)) = &p.error {
            status.component = d.component as u32;
            status.expected = d.expected;
            status.actual = d.actual;
        }
        // SAFETY: output has the documented POD layout and writable storage.
        unsafe {
            out.write(status);
        }
        OK
    })
}
/// # Safety
/// Live readable player and nonoverlapping writable handle output. The result
/// is an owned checkpoint clone; destroy using grazer_game_destroy.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_replay_clone_game(
    player: *const GrazerReplayPlayer,
    out: *mut *mut GrazerGame,
) -> i32 {
    guard(|| {
        if out.is_null() {
            return INVALID_ARGUMENT;
        }
        // SAFETY: caller supplies writable handle output storage.
        unsafe {
            *out = std::ptr::null_mut();
        }
        // SAFETY: caller supplies readable live player or null.
        let Some(p) = (unsafe { player.as_ref() }) else {
            return INVALID_ARGUMENT;
        };
        let inner = match &p.inner {
            HostedPlayer::Native(p) => HostedGame::Native(Box::new(p.game().clone())),
            HostedPlayer::Script(p) => HostedGame::Script(Box::new(p.game().clone())),
        };
        // SAFETY: transfer unique ownership of the successful clone handle.
        unsafe {
            *out = Box::into_raw(Box::new(GrazerGame { inner }));
        }
        OK
    })
}
