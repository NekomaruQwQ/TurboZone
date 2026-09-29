//! Shared contracts, configuration value semantics, and observation types.
//!
//! Platform and Engine describe cooperation without owning application state. Configuration
//! types retain their intrinsic matching and resize queries; orchestration lives in engine.

pub mod util {
    mod cache;
    pub use cache::Cache;
}

mod config;
mod data;
pub use config::*;
pub use data::*;

mod platform;
pub use platform::*;

mod engine;
pub use engine::*;
