//! Framework-specific presentation and startup filesystem services for TurboZone.
//!
//! This crate renders a generic core engine through eframe and loads configuration using
//! only standard filesystem facilities. Native window behavior and process entry points
//! remain platform-adapter responsibilities.

pub mod app;
pub mod config;
pub mod ui;
