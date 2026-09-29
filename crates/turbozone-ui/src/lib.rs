//! Framework-specific presentation and startup filesystem services for TurboZone.
//!
//! This crate renders the shared Engine contract through eframe and loads configuration using
//! only standard filesystem facilities. Native window behavior and process entry points
//! remain platform-adapter responsibilities.

pub mod config;
pub mod app;
