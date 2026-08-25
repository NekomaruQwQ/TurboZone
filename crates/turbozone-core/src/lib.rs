//! Platform-independent configuration, matching, and geometry for TurboZone.

#![feature(const_array)]
#![feature(const_trait_impl)]

#![feature(normalize_lexically)]

pub mod prelude;

mod config;
mod data;
mod manifest;

pub use config::*;
pub use data::*;
pub use manifest::*;
