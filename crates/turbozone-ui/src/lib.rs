//! Framework-specific presentation for the shared application contract.
//!
//! App owns rendering, interactions, diagnostic text, and completion-based scheduling.
//! Its engine is supplied by the composition root; configuration loading and native
//! operations remain outside this crate, including its test dependencies.

pub mod app;
