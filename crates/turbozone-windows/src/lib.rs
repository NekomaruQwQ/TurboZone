//! Windows snapshots and window manipulation for TurboRnR.

mod native;
mod window;

pub use window::*;

pub use windows::core::{
    Error  as NativeError,
    Result as NativeResult,
};
