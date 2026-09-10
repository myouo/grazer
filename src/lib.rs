//! Deterministic foundations for 2D bullet-hell games.
//!
//! The M0 runtime implements bounded linear motion, tick-boundary input and draw
//! snapshots. It does not yet implement collisions, a scripting VM or a game loop.
//! Rendering is optional; the default build has no dependencies.
//!
//! ```
//! use grazer::{Runtime, Config, Input};
//! let mut game = Runtime::new(Config::default(), 42)?;
//! game.set_input(Input { x: 1, y: 0 })?;
//! game.step()?;
//! assert_eq!(game.tick(), 1);
//! # Ok::<(), grazer::Error>(())
//! ```

pub mod demo;
mod fixed;
mod runtime;
pub use fixed::Fixed;
pub use runtime::{Bullet, Config, DrawSprite, Error, Input, PROTOCOL_VERSION, Runtime, TICK_RATE};

#[cfg(feature = "ffi")]
pub mod ffi;
#[cfg(feature = "graphics")]
pub mod graphics;
#[cfg(all(feature = "web", target_arch = "wasm32"))]
mod web;
