//! Orca Deck as built-in daemon actions (port of the Node plugin `com.miguelpicado.orcadock`).
//! No plugin process and no WebSocket: the daemon owns a `Registry`, feeds it scene keys and
//! presses, and rasterises the SVG it returns with `raster::svg_to_rgba`.
//!
//! - `core`: pure logic and SVG rendering (agents, slots, agent/clock/summary/quota keys).
//! - `orca`: the `Orca` trait and its named-pipe client; window raise; local time.
//! - `actions`: `Deck` (shared data), the `BuiltinAction` trait, the four actions, `Registry`.
//! - `raster`: SVG to RGBA at 85x85 (keys) or 80x80 (side cells).
pub mod actions;
pub mod core;
pub mod orca;
pub mod raster;

pub use actions::{
    blink_ms, frame, next_wait_ms, now_ms, BuiltinAction, Ctx, Deck, Feedback, Key, Registry, POLL_MS, QUOTA_MS, SPIN_MS, WORKERS_MS,
};
pub use orca::{Orca, PipeOrca};
