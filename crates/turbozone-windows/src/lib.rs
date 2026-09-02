//! Windows snapshots, action execution, and executable backend for TurboZone.

mod backend;
mod native;
mod window;

pub use backend::*;
pub use window::*;

pub use windows::core::{
    Error  as NativeError,
    Result as NativeResult,
};
