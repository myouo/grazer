//! Additive game ABI v2; legacy motion ABI v1 entrypoints remain intact.
use super::{BUFFER_TOO_SMALL, INVALID_ARGUMENT, OK, RUNTIME_ERROR, VERSION_MISMATCH, guard};
use crate::{
    game::{AdvancedHud, AudioEvent, DemoStage, Game, GameConfig, GameInput, GameSprite, Hud},
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
    pub(super) inner: HostedGame,
}
pub(super) enum HostedGame {
    Native(Box<Game>),
    Script(Box<Game<crate::language::ScriptStage>>),
}
impl HostedGame {
    fn step(&mut self, input: GameInput) -> Result<(), crate::GameError> {
        match self {
            Self::Native(game) => game.step(input),
            Self::Script(game) => game.step(input),
        }
    }
    fn restart(&mut self) -> Result<(), crate::GameError> {
        match self {
            Self::Native(game) => game.restart(),
            Self::Script(game) => game.restart(),
        }
    }
    fn state_hash(&self) -> u64 {
        match self {
            Self::Native(game) => game.state_hash(),
            Self::Script(game) => game.state_hash(),
        }
    }
    fn hud(&self) -> Hud {
        match self {
            Self::Native(game) => game.hud(),
            Self::Script(game) => game.hud(),
        }
    }
    fn advanced_hud(&self) -> Option<AdvancedHud> {
        match self {
            Self::Native(game) => game.advanced_hud(),
            Self::Script(game) => game.advanced_hud(),
        }
    }
    fn audio_events(&self) -> &[AudioEvent] {
        match self {
            Self::Native(game) => game.audio_events(),
            Self::Script(game) => game.audio_events(),
        }
    }
    fn resources(&self) -> &ResourcePack {
        match self {
            Self::Native(game) => game.resources(),
            Self::Script(game) => game.resources(),
        }
    }
    fn diagnostic(&self) -> Option<&crate::language::Diagnostic> {
        match self {
            Self::Native(_) => None,
            Self::Script(game) => game.diagnostic(),
        }
    }
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
                    *out = Box::into_raw(Box::new(GrazerGame {
                        inner: HostedGame::Native(Box::new(inner)),
                    }));
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
        match &game.inner {
            // SAFETY: both variants use the same caller-provided snapshot contract.
            HostedGame::Native(game) => unsafe {
                copy(
                    game.sprites(),
                    game.sprites().count(),
                    out,
                    capacity,
                    required,
                )
            },
            // SAFETY: both variants use the same caller-provided snapshot contract.
            HostedGame::Script(game) => unsafe {
                copy(
                    game.sprites(),
                    game.sprites().count(),
                    out,
                    capacity,
                    required,
                )
            },
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
pub(super) unsafe fn copy<T: Copy>(
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

pub const SCRIPT_API_VERSION: u32 = 1;
pub const SCRIPT_ERROR: i32 = 6;
#[repr(C)]
#[derive(Default)]
pub struct GrazerScriptDiagnostic {
    pub kind: u32,
    pub line: u32,
    pub column: u32,
    pub start: u32,
    pub end: u32,
    pub task_slot: u32,
    pub task_generation: u32,
    pub reserved: u32,
}
fn diagnostic_info(d: &crate::language::Diagnostic) -> GrazerScriptDiagnostic {
    GrazerScriptDiagnostic {
        kind: d.kind as u32,
        line: d.line,
        column: d.column,
        start: d.span.start,
        end: d.span.end,
        task_slot: d.task.map_or(u32::MAX, |t| t.slot),
        task_generation: d.task.map_or(0, |t| t.generation),
        reserved: 0,
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn grazer_script_api_version() -> u32 {
    SCRIPT_API_VERSION
}
/// # Safety
/// Config/output/diagnostic/source buffers are valid aligned and nonoverlapping.
/// Source has length readable bytes; null/zero with format 0 selects built-in.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_game_create_script(
    config: *const GrazerGameConfig,
    source: *const u8,
    length: u32,
    format: u32,
    out: *mut *mut GrazerGame,
    diagnostic: *mut GrazerScriptDiagnostic,
) -> i32 {
    // SAFETY: forwards the documented source and output buffer contract.
    unsafe { create_script(config, source, length, format, out, diagnostic, None) }
}
/// # Safety
/// Same buffer contract as create_script. Null/zero source selects the M4
/// showcase; difficulty is 0 Easy, 1 Normal or 2 Hard.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_game_create_advanced(
    config: *const GrazerGameConfig,
    source: *const u8,
    length: u32,
    format: u32,
    difficulty: u32,
    out: *mut *mut GrazerGame,
    diagnostic: *mut GrazerScriptDiagnostic,
) -> i32 {
    let Some(difficulty) = crate::advanced::Difficulty::from_u32(difficulty) else {
        if !out.is_null() {
            // SAFETY: caller provides writable output storage by contract.
            unsafe {
                *out = std::ptr::null_mut();
            }
        }
        return INVALID_ARGUMENT;
    };
    // SAFETY: forwards the documented source and output buffer contract.
    unsafe {
        create_script(
            config,
            source,
            length,
            format,
            out,
            diagnostic,
            Some(difficulty),
        )
    }
}
unsafe fn create_script(
    config: *const GrazerGameConfig,
    source: *const u8,
    length: u32,
    format: u32,
    out: *mut *mut GrazerGame,
    diagnostic: *mut GrazerScriptDiagnostic,
    difficulty: Option<crate::advanced::Difficulty>,
) -> i32 {
    guard(|| {
        if out.is_null() {
            return INVALID_ARGUMENT;
        }
        // SAFETY: caller provides valid writable output pointer storage.
        unsafe {
            *out = std::ptr::null_mut();
        }
        if !diagnostic.is_null() {
            // SAFETY: optional diagnostic is valid writable storage by contract.
            unsafe {
                diagnostic.write(GrazerScriptDiagnostic::default());
            }
        }
        // SAFETY: config must be live/readable or null by contract.
        let Some(config) = (unsafe { config.as_ref() }) else {
            return INVALID_ARGUMENT;
        };
        if config.abi_version != GAME_ABI_VERSION
            || config.struct_size as usize != std::mem::size_of::<GrazerGameConfig>()
        {
            return VERSION_MISMATCH;
        }
        if format > 1 || length > 16 * 1024 * 1024 || source.is_null() && length > 0 {
            return INVALID_ARGUMENT;
        }
        let bytes = if length == 0 {
            &[][..]
        } else {
            // SAFETY: caller guarantees length readable source bytes.
            unsafe { std::slice::from_raw_parts(source, length as usize) }
        };
        let stage = if format == 1 {
            crate::language::Program::from_bytes(bytes).and_then(|program| {
                crate::language::ScriptStage::new(
                    std::sync::Arc::new(program),
                    crate::language::VmLimits::default(),
                    config.seed,
                )
            })
        } else if bytes.is_empty() {
            if difficulty.is_some() {
                crate::language::ScriptStage::showcase(config.seed)
            } else {
                crate::language::ScriptStage::builtin(config.seed)
            }
        } else {
            match std::str::from_utf8(bytes) {
                Ok(source) => crate::language::ScriptStage::compile(
                    "c-stage.graze",
                    source,
                    crate::language::VmLimits::default(),
                    config.seed,
                ),
                Err(_) => return INVALID_ARGUMENT,
            }
        };
        let stage = match stage {
            Ok(stage) => stage,
            Err(error) => {
                if !diagnostic.is_null() {
                    // SAFETY: optional diagnostic is valid nonoverlapping writable storage.
                    unsafe {
                        diagnostic.write(diagnostic_info(&error));
                    }
                }
                return SCRIPT_ERROR;
            }
        };
        let mut cfg = GameConfig::default();
        if config.projectile_capacity > 0 {
            cfg.simulation.projectile_capacity = config.projectile_capacity;
        }
        if config.player_health > 0 {
            cfg.simulation.player.health = config.player_health;
        }
        let game = if let Some(difficulty) = difficulty {
            Game::with_advanced_stage(
                cfg,
                config.seed,
                ResourcePack::builtin(),
                stage,
                crate::advanced::AdvancedConfig {
                    difficulty,
                    ..crate::advanced::AdvancedConfig::default()
                },
            )
        } else {
            Game::with_stage(cfg, config.seed, ResourcePack::builtin(), stage)
        };
        match game {
            Ok(game) => {
                // SAFETY: out is writable pointer storage; caller owns the Box handle.
                unsafe {
                    *out = Box::into_raw(Box::new(GrazerGame {
                        inner: HostedGame::Script(Box::new(game)),
                    }));
                }
                OK
            }
            Err(_) => INVALID_ARGUMENT,
        }
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn grazer_advanced_api_version() -> u32 {
    1
}
/// # Safety
/// Handle readable, output aligned/writable and nonoverlapping. Legacy games
/// return INVALID_ARGUMENT; they continue to expose the original HUD.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_game_advanced_hud(
    game: *const GrazerGame,
    out: *mut AdvancedHud,
) -> i32 {
    guard(|| {
        if out.is_null() {
            return INVALID_ARGUMENT;
        }
        // SAFETY: caller supplies a readable live handle or null.
        let Some(game) = (unsafe { game.as_ref() }) else {
            return INVALID_ARGUMENT;
        };
        let Some(hud) = game.inner.advanced_hud() else {
            return INVALID_ARGUMENT;
        };
        // SAFETY: caller provides aligned writable output storage.
        unsafe {
            out.write(hud);
        }
        OK
    })
}
/// # Safety
/// Same count/query, alignment, nonaliasing and serialization contract as
/// grazer_game_snapshot. Laser segments are separate from sprite snapshots.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_game_lasers(
    game: *const GrazerGame,
    out: *mut crate::advanced::LaserSegment,
    capacity: u32,
    required: *mut u32,
) -> i32 {
    guard(|| {
        // SAFETY: caller supplies a readable live handle or null.
        let Some(game) = (unsafe { game.as_ref() }) else {
            return INVALID_ARGUMENT;
        };
        match &game.inner {
            // SAFETY: copy receives the documented nonoverlapping output buffers.
            HostedGame::Native(game) => unsafe {
                copy(
                    game.laser_segments(),
                    game.laser_segments().count(),
                    out,
                    capacity,
                    required,
                )
            },
            // SAFETY: copy receives the documented nonoverlapping output buffers.
            HostedGame::Script(game) => unsafe {
                copy(
                    game.laser_segments(),
                    game.laser_segments().count(),
                    out,
                    capacity,
                    required,
                )
            },
        }
    })
}
/// # Safety
/// Handle live/readable; out is nonoverlapping writable diagnostic storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_game_diagnostic(
    game: *const GrazerGame,
    out: *mut GrazerScriptDiagnostic,
) -> i32 {
    guard(|| {
        if out.is_null() {
            return INVALID_ARGUMENT;
        }
        // SAFETY: caller guarantees readable live handle or null.
        let Some(game) = (unsafe { game.as_ref() }) else {
            return INVALID_ARGUMENT;
        };
        // SAFETY: out is valid writable diagnostic storage by contract.
        unsafe {
            out.write(
                game.inner
                    .diagnostic()
                    .map_or_else(GrazerScriptDiagnostic::default, diagnostic_info),
            );
        }
        OK
    })
}
/// # Safety
/// Same output-buffer rules as snapshot. Length includes terminating NUL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grazer_game_diagnostic_text(
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
        let text = game
            .inner
            .diagnostic()
            .map_or_else(String::new, ToString::to_string);
        // SAFETY: helper uses the same valid/nonaliasing byte buffer contract.
        unsafe {
            copy(
                text.bytes().chain(std::iter::once(0)),
                text.len() + 1,
                out,
                capacity,
                required,
            )
        }
    })
}
