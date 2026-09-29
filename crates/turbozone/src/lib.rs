//! Startup services owned by the executable's composition root.
//!
//! Explicit paths keep config filesystem operations independently testable while the
//! binary selects the production known folder and constructs the engine and UI.

pub mod config;
