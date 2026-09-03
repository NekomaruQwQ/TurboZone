//! Platform-independent product logic and backend contracts for TurboZone.
//!
//! Core owns configuration interpretation, stable rule identity, action orchestration,
//! snapshot grouping, and non-fatal logging policy. Platform adapters own native reads
//! and writes, while presentation crates decide when to tick and how to render the state.

pub mod util {
    mod cache;
    pub use cache::Cache;
}

mod config;
mod data;
pub use config::*;
pub use data::*;

mod engine;
mod logging;
mod config_parser;

pub use engine::*;
pub use logging::*;
pub use config_parser::*;
