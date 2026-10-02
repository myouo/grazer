//! Deterministic foundations for 2D bullet-hell games.
//!
//! [`Game`] is the M2 playable SDK: native Rust stages, six controls, resources,
//! sprite/HUD snapshots and audio events. [`Simulation`] implements the M1
//! headless collision core. [`Runtime`] retains the protocol-1 M0 fixture.
//! [`language`] provides the M3 typed compiler, verified bytecode, cooperative
//! tasks, resource budgets, source diagnostics and serialized VM continuation.
//! [`advanced`] adds M4 patterns, composed motion, timed persistent lasers,
//! drops and difficulty. [`game::showcase`] runs the ten-minute script stage.
//! [`checkpoint`] and [`game::replay`] provide complete portable game snapshots
//! and verified playback; [`game::debug`] adds host-side iteration controls.
//! Rendering is optional; the default build has no dependencies.
//!
//! ```
//! use grazer::{Simulation, SimulationConfig, Input};
//! let mut game = Simulation::new(SimulationConfig::default(), 42)?;
//! game.step_with_input(Input { x: 1, y: 0 })?;
//! assert_eq!(game.tick(), 1);
//! # Ok::<(), grazer::SimulationError>(())
//! ```

pub mod checkpoint;
pub mod demo;
mod fixed;
pub mod game;
pub mod language;
pub use game::{
    AdvancedHud, AudioEvent, Game, GameConfig, GameError, GameInput, GamePhase, GameSprite, Hud,
    Stage, StageStatus,
};
pub mod resources;
mod runtime;
pub mod simulation;
pub use fixed::Fixed;
pub use runtime::{Bullet, Config, DrawSprite, Error, Input, PROTOCOL_VERSION, Runtime, TICK_RATE};
pub use simulation::advanced;
pub use simulation::{
    BoundsBehavior, Collider, Enemy, EntityHandle, EntityKind, EntitySnapshot, Event, Faction,
    PlayerConfig, PlayerState, Projectile, SIMULATION_PROTOCOL_VERSION, Simulation,
    SimulationConfig, SimulationError, Vec2,
};

#[cfg(all(feature = "desktop", not(target_arch = "wasm32")))]
pub mod audio;
#[cfg(feature = "ffi")]
pub mod ffi;
#[cfg(feature = "graphics")]
pub mod graphics;
#[cfg(all(feature = "web", target_arch = "wasm32"))]
mod web;
