//! Deterministic foundations for 2D bullet-hell games.
//!
//! [`Simulation`] implements the M1 headless core: generational entities, swept
//! collisions, damage, grazing and versioned input/command replays. [`Runtime`]
//! retains the protocol-1 M0 motion and presentation fixture. A scripting VM and
//! playable game runner are later milestones.
//! Rendering is optional; the default build has no dependencies.
//!
//! ```
//! use grazer::{Simulation, SimulationConfig, Input};
//! let mut game = Simulation::new(SimulationConfig::default(), 42)?;
//! game.step_with_input(Input { x: 1, y: 0 })?;
//! assert_eq!(game.tick(), 1);
//! # Ok::<(), grazer::SimulationError>(())
//! ```

pub mod demo;
mod fixed;
mod runtime;
pub mod simulation;
pub use fixed::Fixed;
pub use runtime::{Bullet, Config, DrawSprite, Error, Input, PROTOCOL_VERSION, Runtime, TICK_RATE};
pub use simulation::{
    BoundsBehavior, Collider, Enemy, EntityHandle, EntityKind, EntitySnapshot, Event, Faction,
    PlayerConfig, PlayerState, Projectile, SIMULATION_PROTOCOL_VERSION, Simulation,
    SimulationConfig, SimulationError, Vec2,
};

#[cfg(feature = "ffi")]
pub mod ffi;
#[cfg(feature = "graphics")]
pub mod graphics;
#[cfg(all(feature = "web", target_arch = "wasm32"))]
mod web;
