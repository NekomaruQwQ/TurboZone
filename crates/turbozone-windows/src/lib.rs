//! Windows snapshots and window manipulation for TurboRnR.

#![feature(normalize_lexically)]
#![deny(missing_docs)]

mod native;
mod path;
mod window;

pub use path::*;
pub use window::*;
pub use windows::core::{Error as NativeError, Result as NativeResult};

