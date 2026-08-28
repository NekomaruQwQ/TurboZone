//! Application startup and presentation, separated from the process entry point.
//!
//! The library boundary allows integration tests to exercise loading, classification,
//! and rendering without launching a native event loop or changing process environment.

pub mod app;
pub mod config;
pub mod data;
pub mod diagnostics;
pub mod ui;
