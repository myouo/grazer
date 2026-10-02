//! M4 deterministic ten-minute showcase, shared by native, C and WASM hosts.
use super::*;
use crate::{
    advanced::{AdvancedConfig, Difficulty},
    language::ScriptStage,
};
pub const SHOWCASE_TICKS: u64 = 36000;
/// Completion runs at boundary 36,000, then commits its final simulation tick.
pub const SHOWCASE_CLEAR_TICK: u64 = SHOWCASE_TICKS + 1;
pub fn game(difficulty: Difficulty, health: u32) -> Result<Game<ScriptStage>, GameError> {
    let mut config = GameConfig::default();
    if health > 0 {
        config.simulation.player.health = health;
    }
    Game::with_advanced_stage(
        config,
        42,
        ResourcePack::builtin(),
        ScriptStage::showcase(42).map_err(GameError::Script)?,
        AdvancedConfig {
            difficulty,
            ..AdvancedConfig::default()
        },
    )
}
pub fn conformance_game() -> Game<ScriptStage> {
    let mut config = GameConfig::default();
    config.simulation.projectile_capacity = 1024;
    config.simulation.player.health = 10000;
    Game::with_advanced_stage(
        config,
        42,
        ResourcePack::builtin(),
        ScriptStage::showcase(42).expect("showcase compiles"),
        AdvancedConfig::default(),
    )
    .expect("bounded showcase")
}
pub fn input(frame: u64) -> GameInput {
    GameInput {
        fire: true,
        focus: frame % 200 < 100,
        bomb: frame % 2400 == 100,
        restart: frame % 42000 == 41999,
        ..GameInput::default()
    }
}
pub fn trace(frames: u32) -> Vec<u64> {
    let mut game = conformance_game();
    (0..frames)
        .map(|frame| {
            game.step(input(u64::from(frame)))
                .expect("bounded showcase content");
            game.state_hash()
        })
        .collect()
}
