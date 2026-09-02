//! Platform-independent product logic and backend contracts for TurboZone.
//!
//! Core owns configuration interpretation, stable rule identity, action orchestration,
//! snapshot grouping, and non-fatal logging policy. Platform adapters own native reads
//! and writes, while presentation crates decide when to tick and how to render the state.

pub mod constants;

mod engine;
mod logging;
mod window;

mod config;
mod config_parser;
mod runtime;

pub use engine::*;
pub use logging::*;
pub use window::*;

pub use config::*;
pub use config_parser::*;
pub use runtime::*;
