//! Platform-independent configuration, matching, grouping, and geometry for TurboRnR.

#![deny(missing_docs)]

mod config;
mod geometry;
mod grouping;

pub use config::*;
pub use geometry::*;
pub use grouping::*;

