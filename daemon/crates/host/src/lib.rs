//! OrcaDeck plugin host: runs Elgato Stream Deck SDK v2 (and Stream Dock) plugins.
//!
//! The host listens on a loopback WebSocket port, launches plugin processes with the SDK argv,
//! and routes events between plugins, property inspectors and the daemon. The daemon drives it
//! through [`Host`] (place actions on keys, inject key presses) and receives what plugins draw
//! through an [`Events`] sink. No hardware access happens here.
//!
//! ```no_run
//! use orcadeck_host::{discover, Config, Host, Instance, Output};
//! let host = Host::start(Config::default(), |out: Output| println!("{out:?}")).unwrap();
//! for plugin in discover(&["C:/plugins".into()], None) {
//!     host.launch(plugin).unwrap();
//! }
//! let ctx = host.appear(Instance::new("com.example.plugin", "com.example.plugin.action", "dev", 0, 0));
//! host.key(&ctx, true);
//! ```

mod host;
mod launch;
mod manifest;

pub use host::{Config, Events, Host, Instance, Key, Output};
pub use launch::{command, info, random_uuid, version_max, Device};
pub use manifest::{discover, load, Plugin, Skip};
