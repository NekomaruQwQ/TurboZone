//! Application behavior behind the shared Engine contract.
//!
//! Parsing verifies complete configurations without I/O. The implementation owns rules,
//! observations, and pending effects. Platform adapters
//! supply native operations; presentation controls scheduling and renders borrowed views.

mod engine;
pub use engine::*;

pub mod config_parser;
