//! Additive game ABI v2; legacy motion ABI v1 entrypoints remain intact.
use super::{BUFFER_TOO_SMALL, INVALID_ARGUMENT, OK, RUNTIME_ERROR, VERSION_MISMATCH, guard};
use crate::{
    game::{AudioEvent, DemoStage, Game, GameConfig, GameInput, GameSprite, Hud},
    resources::{RESOURCE_VERSION, ResourcePack, SoundAsset, SpriteAsset},
};
pub const GAME_ABI_VERSION: u32 = 2;
#[repr(C)]
pub struct GrazerGameConfig {
    pub abi_version: u32,
    pub struct_size: u32,
    pub seed: u64,
    pub projectile_capacity: u32,
    pub player_health: u32,
}
#[repr(C)]
pub struct GrazerGameInput {
    pub x: i32,
    pub y: i32,
    pub flags: u32,
}
#[repr(C)]
pub struct GrazerResourceInfo {
    pub version: u32,
    pub width: u32,
    pub height: u32,
    pub atlas_bytes: u32,
    pub sprites: u32,
    pub sounds: u32,
    pub content_hash: u64,
}
pub struct GrazerGame {
    inner: Game,
}
#[unsafe(no_mangle)]
pub extern "C" fn grazer_game_abi_version() -> u32 {
    GAME_ABI_VERSION
}
/// # Safety
/// Config and pointer output must be valid aligned nonoverlapping storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_game_create(
    config: *const GrazerGameConfig,
    out: *mut *mut GrazerGame,
) -> i32 {
    guard(|| {
        if out.is_null() {
            return INVALID_ARGUMENT;
        }
        // SAFETY: caller supplies writable pointer storage.
        unsafe {
            *out = std::ptr::null_mut();
        }
        // SAFETY: caller supplies a valid readable config or null.
        let Some(config) = (unsafe { config.as_ref() }) else {
            return INVALID_ARGUMENT;
        };
        if config.abi_version != GAME_ABI_VERSION
            || config.struct_size as usize != std::mem::size_of::<GrazerGameConfig>()
        {
            return VERSION_MISMATCH;
        }
        let mut cfg = GameConfig::default();
        if config.projectile_capacity > 0 {
            cfg.simulation.projectile_capacity = config.projectile_capacity;
        }
        if config.player_health > 0 {
            cfg.simulation.player.health = config.player_health;
        }
        match Game::with_stage(
            cfg,
            config.seed,
            ResourcePack::builtin(),
            DemoStage::default(),
        ) {
            Ok(inner) => {
                // SAFETY: out is valid writable storage and owns the resulting Box.
                unsafe {
                    *out = Box::into_raw(Box::new(GrazerGame { inner }));
                }
                OK
            }
            Err(_) => INVALID_ARGUMENT,
        }
    })
}
/// # Safety
/// Pointer is null or a uniquely owned live handle, destroyed exactly once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_game_destroy(game: *mut GrazerGame) {
    if !game.is_null() {
        // SAFETY: caller transfers sole ownership of the live Box.
        drop(unsafe { Box::from_raw(game) });
    }
}
/// # Safety
/// Handle must be live and exclusively borrowed for the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_game_step(game: *mut GrazerGame, input: GrazerGameInput) -> i32 {
    guard(|| {
        // SAFETY: caller guarantees live exclusive storage or null.
        let Some(game) = (unsafe { game.as_mut() }) else {
            return INVALID_ARGUMENT;
        };
        let Ok(input) = GameInput::from_flags(input.x, input.y, input.flags) else {
            return INVALID_ARGUMENT;
        };
        game.inner.step(input).map_or(RUNTIME_ERROR, |_| OK)
    })
}
/// # Safety
/// Handle must be live and exclusively borrowed for the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_game_restart(game: *mut GrazerGame) -> i32 {
    guard(|| {
        // SAFETY: caller guarantees live exclusive storage or null.
        let Some(game) = (unsafe { game.as_mut() }) else {
            return INVALID_ARGUMENT;
        };
        game.inner.restart().map_or(RUNTIME_ERROR, |_| OK)
    })
}
/// # Safety
/// Handle is readable; out is writable nonoverlapping u64 storage. Calls serial.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_game_state_hash(game: *const GrazerGame, out: *mut u64) -> i32 {
    guard(|| {
        if out.is_null() {
            return INVALID_ARGUMENT;
        }
        // SAFETY: caller guarantees readable live handle or null.
        let Some(game) = (unsafe { game.as_ref() }) else {
            return INVALID_ARGUMENT;
        };
        // SAFETY: out is writable and does not alias the game.
        unsafe {
            *out = game.inner.state_hash();
        }
        OK
    })
}
/// # Safety
/// Handle readable; out writable nonoverlapping Hud storage. Calls serial.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_game_hud(game: *const GrazerGame, out: *mut Hud) -> i32 {
    guard(|| {
        if out.is_null() {
            return INVALID_ARGUMENT;
        }
        // SAFETY: caller supplies readable live handle or null.
        let Some(game) = (unsafe { game.as_ref() }) else {
            return INVALID_ARGUMENT;
        };
        // SAFETY: writable out has the documented C layout.
        unsafe {
            out.write(game.inner.hud());
        }
        OK
    })
}
/// # Safety
/// Handle readable, required writable, out writable for capacity sprites or null
/// at zero capacity. All regions nonoverlapping. No concurrent mutation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_game_snapshot(
    game: *const GrazerGame,
    out: *mut GameSprite,
    capacity: u32,
    required: *mut u32,
) -> i32 {
    guard(|| {
        // SAFETY: caller guarantees readable live handle or null.
        let Some(game) = (unsafe { game.as_ref() }) else {
            return INVALID_ARGUMENT;
        };
        // SAFETY: helper uses the same caller-provided buffer contract.
        unsafe {
            copy(
                game.inner.sprites(),
                game.inner.sprites().count(),
                out,
                capacity,
                required,
            )
        }
    })
}
/// # Safety
/// Same buffer, alignment, nonaliasing and serialization rules as snapshot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_game_audio(
    game: *const GrazerGame,
    out: *mut AudioEvent,
    capacity: u32,
    required: *mut u32,
) -> i32 {
    guard(|| {
        // SAFETY: caller guarantees readable live handle or null.
        let Some(game) = (unsafe { game.as_ref() }) else {
            return INVALID_ARGUMENT;
        };
        let events = game.inner.audio_events();
        // SAFETY: helper uses the documented buffer contract.
        unsafe {
            copy(
                events.iter().copied(),
                events.len(),
                out,
                capacity,
                required,
            )
        }
    })
}
/// # Safety
/// Handle readable; out writable nonoverlapping resource-info storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_game_resource_info(
    game: *const GrazerGame,
    out: *mut GrazerResourceInfo,
) -> i32 {
    guard(|| {
        if out.is_null() {
            return INVALID_ARGUMENT;
        }
        // SAFETY: caller guarantees readable live handle or null.
        let Some(game) = (unsafe { game.as_ref() }) else {
            return INVALID_ARGUMENT;
        };
        let pack = game.inner.resources();
        // SAFETY: out is correctly aligned writable storage for the C struct.
        unsafe {
            out.write(GrazerResourceInfo {
                version: RESOURCE_VERSION,
                width: pack.width(),
                height: pack.height(),
                atlas_bytes: pack.atlas().len() as u32,
                sprites: pack.sprites().len() as u32,
                sounds: pack.sounds().len() as u32,
                content_hash: pack.content_hash(),
            });
        }
        OK
    })
}
/// # Safety
/// Same buffer/nonaliasing contract as snapshot, measured in bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_game_atlas(
    game: *const GrazerGame,
    out: *mut u8,
    capacity: u32,
    required: *mut u32,
) -> i32 {
    guard(|| {
        // SAFETY: caller guarantees readable live handle or null.
        let Some(game) = (unsafe { game.as_ref() }) else {
            return INVALID_ARGUMENT;
        };
        let atlas = game.inner.resources().atlas();
        // SAFETY: helper uses the documented byte buffer contract.
        unsafe { copy(atlas.iter().copied(), atlas.len(), out, capacity, required) }
    })
}
/// # Safety
/// Same buffer/nonaliasing contract as snapshot, measured in SpriteAsset entries.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_game_sprite_assets(
    game: *const GrazerGame,
    out: *mut SpriteAsset,
    capacity: u32,
    required: *mut u32,
) -> i32 {
    guard(|| {
        // SAFETY: caller guarantees readable live handle or null.
        let Some(game) = (unsafe { game.as_ref() }) else {
            return INVALID_ARGUMENT;
        };
        let assets = game.inner.resources().sprites();
        // SAFETY: helper uses the documented struct buffer contract.
        unsafe {
            copy(
                assets.iter().copied(),
                assets.len(),
                out,
                capacity,
                required,
            )
        }
    })
}
/// # Safety
/// Same buffer/nonaliasing contract as snapshot, measured in SoundAsset entries.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_game_sound_assets(
    game: *const GrazerGame,
    out: *mut SoundAsset,
    capacity: u32,
    required: *mut u32,
) -> i32 {
    guard(|| {
        // SAFETY: caller guarantees readable live handle or null.
        let Some(game) = (unsafe { game.as_ref() }) else {
            return INVALID_ARGUMENT;
        };
        let assets = game.inner.resources().sounds();
        // SAFETY: helper uses the documented struct buffer contract.
        unsafe {
            copy(
                assets.iter().copied(),
                assets.len(),
                out,
                capacity,
                required,
            )
        }
    })
}
unsafe fn copy<T: Copy>(
    items: impl Iterator<Item = T>,
    count: usize,
    out: *mut T,
    capacity: u32,
    required: *mut u32,
) -> i32 {
    if required.is_null() {
        return INVALID_ARGUMENT;
    }
    // SAFETY: caller promises valid writable required storage.
    unsafe {
        *required = count as u32;
    }
    if (capacity as usize) < count {
        return BUFFER_TOO_SMALL;
    }
    if count == 0 {
        return OK;
    }
    if out.is_null() {
        return INVALID_ARGUMENT;
    }
    for (index, item) in items.enumerate() {
        // SAFETY: capacity covers all count elements; caller guarantees storage.
        unsafe {
            out.add(index).write(item);
        }
    }
    OK
}
