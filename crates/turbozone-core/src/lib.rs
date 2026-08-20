//! Platform-independent configuration, matching, and geometry for TurboZone.

#![feature(const_array)]
#![feature(const_trait_impl)]

#![deny(missing_docs)]

mod config;
mod data;
mod manifest;

pub use config::*;
pub use data::*;
pub use manifest::*;
pub use euclid::default::Size2D;
