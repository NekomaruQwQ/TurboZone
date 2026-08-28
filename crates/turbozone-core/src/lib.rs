//! Platform-independent configuration, matching, and geometry for TurboZone.

#![feature(const_array)]
#![feature(const_trait_impl)]

#![feature(normalize_lexically)]

pub mod prelude;

mod window;
mod manifest;

mod config;
mod config_parser;
mod pattern;
mod runtime;

pub use window::*;
pub use manifest::*;

pub use config::*;
pub use config_parser::*;
pub use pattern::*;
pub use runtime::*;
