//! High-level window manipulation and geometry utilities.

use turbozone_core::WindowState;
use crate::Handle;
use crate::native;
use crate::native::Convert as _;

use euclid::default::*;

use windows::core::{Error, Result};
use windows::Win32::Foundation::{E_INVALIDARG, HWND, RECT};
use windows::Win32::Graphics::Gdi::MONITORINFO;

/// Centers the client area without changing size, activation, z-order, or visual state.
///
/// Returns a native error when geometry cannot be queried or the move fails.
pub fn center_window(handle: Handle<HWND>) -> Result<()> {
    let monitor = native::get_monitor_info(native::get_monitor(handle.0))?;
    let state = get_window_state(handle.0);
    let content = get_content_rect(handle.0, state, &monitor)?;
    let delta = monitor.rcWork.convert().center() - content.center();
    match state {
        WindowState::Normal => {
            let outer: Rect<i32> = native::get_window_rect(handle.0)?.convert();
            native::set_window_position(handle.0, outer.origin + delta)
        },
        WindowState::Maximized | WindowState::Minimized => {
            let mut placement = native::get_window_placement(handle.0)?;
            // A translation is identical in workspace and screen coordinates.
            placement.rcNormalPosition =
                placement.rcNormalPosition.convert().translate(delta).convert();
            native::set_window_placement(handle.0, &placement)
        },
    }
}

/// Resizes the client area around its center without changing the visual state.
///
/// Returns an error for nonpositive dimensions, unavailable geometry, or failed mutation.
pub fn resize_window(handle: Handle<HWND>, size: Size2D<i32>) -> Result<()> {
    if size.width <= 0 || size.height <= 0 {
        return Err(Error::new(E_INVALIDARG, "size must be positive"));
    }

    match get_window_state(handle.0) {
        WindowState::Normal => native::resize_client(handle.0, size),
        WindowState::Maximized | WindowState::Minimized => {
            resize_restored_window(handle.0, size)
        },
    }
}

/// Reads the visual state without changing it.
pub fn get_window_state(handle: HWND) -> WindowState {
    if native::is_minimized(handle) {
        WindowState::Minimized
    } else if native::is_maximized(handle) {
        WindowState::Maximized
    } else {
        WindowState::Normal
    }
}

/// Queries live geometry or derives restored geometry using standard frame offsets.
pub fn get_content_rect(handle: HWND, state: WindowState, monitor: &MONITORINFO) -> Result<Rect<i32>> {
    match state {
        WindowState::Normal => native::get_content_rect(handle),
        WindowState::Maximized | WindowState::Minimized => {
            get_restored_content_rect(
                native::get_window_placement(handle)?.rcNormalPosition,
                native::get_normal_frame(handle)?,
                native::get_placement_offset(handle, monitor)?)
        },
    }
}

/// Derives restored client geometry from an outer placement rectangle and standard frame offsets.
///
/// The calculation stays independent from Win32 queries and mutation so restored-geometry policy
/// can be reused and verified without changing a desktop window. `offset` converts workspace
/// placement coordinates to screen coordinates when required by the window style.
///
/// # Errors
///
/// Returns [`E_INVALIDARG`] when the inferred frame is larger than the restored outer rectangle.
pub fn get_restored_content_rect(
    outer: RECT,
    frame: RECT,
    offset: Vector2D<i32>) -> Result<Rect<i32>> {
    let size = outer.convert().size - frame.convert().size;
    if size.width < 0 || size.height < 0 {
        return Err(Error::new(E_INVALIDARG, "Restored frame exceeds the window size"));
    }
    Ok(Rect::new(
        Point2D::new(outer.left - frame.left, outer.top - frame.top) + offset,
        size))
}

/// Updates restored placement only, preserving the current show command and flags.
fn resize_restored_window(handle: HWND, size: Size2D<i32>) -> Result<()> {
    let mut placement = native::get_window_placement(handle)?;
    let frame = native::get_normal_frame(handle)?;
    let content = get_restored_content_rect(placement.rcNormalPosition, frame, Vector2D::zero())?;
    let resized = native::resize_rect(content, size)?;
    let outer_size = native::checked_size_sum(size, frame.convert().size)?;
    placement.rcNormalPosition = Rect::new(
        resized.origin + Vector2D::new(frame.left, frame.top), outer_size).convert();
    native::set_window_placement(handle, &placement)
}
