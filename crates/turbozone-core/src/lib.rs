//! Platform-independent product logic and backend contracts for TurboZone.
//!
//! Core owns configuration interpretation, stable rule identity, action orchestration,
//! snapshot grouping, and non-fatal logging policy. Platform adapters own native reads
//! and writes, while presentation crates decide when to tick and how to render the state.

pub mod prelude;
pub mod constants;

mod args;
mod data;
mod engine;
mod logging;
mod window;
mod manifest;

mod config;
mod config_parser;
mod pattern;
mod runtime;

pub use args::*;
pub use constants::*;
pub use data::*;
pub use engine::*;
pub use logging::*;
pub use window::*;
pub use manifest::*;

pub use config::*;
pub use config_parser::*;
pub use pattern::*;
pub use runtime::*;
