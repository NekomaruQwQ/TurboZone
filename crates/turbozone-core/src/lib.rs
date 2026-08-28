//! Platform-independent configuration parsing, matching, and geometry for TurboZone.

pub mod prelude;

mod window;
mod manifest;

mod config;
mod config_parser;
mod pattern;
mod runtime;

pub use window::*;
pub use manifest::*;

pub use config::*;
pub use config_parser::*;
pub use pattern::*;
pub use runtime::*;
