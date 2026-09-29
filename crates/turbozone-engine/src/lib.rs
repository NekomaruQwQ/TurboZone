//! Application behavior behind the shared Engine contract.
//!
//! The implementation owns rules, observations, and pending effects. Platform adapters
//! supply native operations; presentation controls scheduling and renders borrowed views.

mod engine;
pub use engine::*;
