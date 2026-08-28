//! Safe window snapshots and high-level centering and resizing operations.

use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

use anyhow::Context as _;
use euclid::default::{Point2D, Rect, Size2D, Vector2D};
use turbozone_core::{WindowDetail, WindowInfo, WindowState};
use windows::core::{Error, Result};
use windows::Win32::Foundation::{E_INVALIDARG, HWND, RECT};
use windows::Win32::Graphics::Gdi::{HMONITOR, MONITORINFO};

use crate::native;

/// A borrowed native window identity; the default is a null, non-actionable handle.
///
/// Windows may destroy or reuse a handle after a snapshot, so native actions remain fallible.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WindowHandle(HWND);

impl WindowHandle {
    /// Returns the native identity for UI keys and diagnostics, without dereferencing it.
    pub fn address(self) -> usize { self.0.0.addr() }
}

impl Hash for WindowHandle {
    fn hash<H: Hasher>(&self, state: &mut H) { self.address().hash(state); }
}

/// Enumerates application windows, sharing monitor queries within each snapshot.
#[derive(Debug, Default)]
pub struct WindowEnumerator {
    /// Both successful queries and failures expire before the next enumeration.
    monitor_info_cache: BTreeMap<usize, Result<MONITORINFO>>,
}

impl WindowEnumerator {
    /// Captures relevant top-level application windows.
    ///
    /// Only a top-level enumeration failure is returned to the caller. Per-window
    /// detail failures retain the first contextual error on the snapshot.
    pub fn snapshot(&mut self) -> Result<Vec<WindowInfo<WindowHandle>>> {
        // Work areas and monitor handles can change between logic ticks.
        self.monitor_info_cache.clear();
        Ok(native::enumerate_windows()?
            .into_iter()
            .filter(|&handle| native::is_app_window(handle))
            .map(|handle| self.snapshot_window(handle))
            .filter(|window| !window.title.is_empty())
            .filter(|window| !(
                window.title == "Program Manager"
                && window.detail.as_ref().is_ok_and(|detail| {
                    detail.program_name.eq_ignore_ascii_case("explorer.exe")
                })))
            .collect())
    }

    /// Memoizes one query per monitor, including failures, for the current refresh.
    fn monitor_info(
        &mut self,
        monitor: HMONITOR,
        query: impl FnOnce(HMONITOR) -> Result<MONITORINFO>) -> Result<MONITORINFO> {
        self.monitor_info_cache.entry(monitor.0.addr())
            .or_insert_with(|| query(monitor))
            .clone()
    }

    /// Retains basic identity even when a detail query fails.
    fn snapshot_window(&mut self, handle: HWND) -> WindowInfo<WindowHandle> {
        let title = native::get_window_text(handle);
        let state = window_state(handle);
        let detail = self.window_detail(handle, state);
        WindowInfo { handle: WindowHandle(handle), title, state, detail }
    }

    /// Stops at the first failed query while retaining the original native error.
    fn window_detail(&mut self, handle: HWND, state: WindowState) -> anyhow::Result<WindowDetail> {
        let monitor = self.monitor_info(native::get_monitor(handle), native::get_monitor_info)
            .context("Monitor geometry")?;
        let content_rect = content_rect(handle, state, &monitor).context("Client geometry")?;
        let process_id = native::get_process_id(handle).context("Owning process")?;
        let native_path = native::get_program_path(process_id).context("Program path")?;
        let program_name = native_path.file_name().context("Program path has no filename")?
            .to_string_lossy().into_owned();
        // Windows supplies normalized paths; only the separator convention changes.
        let program_path = native_path.to_string_lossy().replace('\\', "/");
        Ok(WindowDetail {
            monitor_rect: native::rect_from_native(&monitor.rcWork),
            content_rect,
            process_id,
            program_path,
            program_name,
        })
    }
}

/// Returns whether the live or restored client area is centered, or none on query failure.
pub fn is_centered(handle: WindowHandle) -> Option<bool> {
    let monitor = native::get_monitor_info(native::get_monitor(handle.0)).ok()?;
    let content = content_rect(handle.0, window_state(handle.0), &monitor).ok()?;
    Some(content.center() == native::rect_from_native(&monitor.rcWork).center())
}

/// Centers the client area without changing size, activation, z-order, or visual state.
///
/// Returns a native error when geometry cannot be queried or the move fails.
pub fn center_window(handle: WindowHandle) -> Result<()> {
    let monitor = native::get_monitor_info(native::get_monitor(handle.0))?;
    let state = window_state(handle.0);
    let content = content_rect(handle.0, state, &monitor)?;
    let delta = native::rect_from_native(&monitor.rcWork).center() - content.center();
    match state {
        WindowState::Normal => {
            let outer = native::rect_from_native(&native::get_window_rect(handle.0)?);
            native::set_window_position(handle.0, outer.origin + delta)
        },
        WindowState::Maximized | WindowState::Minimized => {
            let mut placement = native::get_window_placement(handle.0)?;
            // A translation is identical in workspace and screen coordinates.
            placement.rcNormalPosition = rect_into_native(
                native::rect_from_native(&placement.rcNormalPosition).translate(delta));
            native::set_window_placement(handle.0, &placement)
        },
    }
}

/// Resizes the client area around its center without changing the visual state.
///
/// Returns an error for nonpositive dimensions, unavailable geometry, or failed mutation.
pub fn resize_window(handle: WindowHandle, size: Size2D<i32>) -> Result<()> {
    if size.width <= 0 || size.height <= 0 {
        return Err(Error::new(E_INVALIDARG, "Client dimensions must be positive"));
    }
    match window_state(handle.0) {
        WindowState::Normal => native::resize_client(handle.0, size),
        WindowState::Maximized | WindowState::Minimized => {
            resize_restored_window(handle.0, size)
        },
    }
}

/// Reads the visual state without changing it.
fn window_state(handle: HWND) -> WindowState {
    if native::is_minimized(handle) {
        WindowState::Minimized
    } else if native::is_maximized(handle) {
        WindowState::Maximized
    } else {
        WindowState::Normal
    }
}

/// Queries live geometry or derives restored geometry using standard frame offsets.
fn content_rect(handle: HWND, state: WindowState, monitor: &MONITORINFO) -> Result<Rect<i32>> {
    match state {
        WindowState::Normal => native::get_content_rect(handle),
        WindowState::Maximized | WindowState::Minimized => {
            restored_content_rect(
                &native::get_window_placement(handle)?.rcNormalPosition,
                &native::get_normal_frame(handle)?,
                native::get_placement_offset(handle, monitor)?)
        },
    }
}

/// Removes standard frame offsets and converts workspace placement to screen coordinates.
///
/// Returns an error when the inferred frame is larger than the restored outer rectangle.
fn restored_content_rect(
    outer: &RECT,
    frame: &RECT,
    offset: Vector2D<i32>) -> Result<Rect<i32>> {
    let size = native::rect_from_native(outer).size - native::rect_from_native(frame).size;
    if size.width < 0 || size.height < 0 {
        return Err(Error::new(E_INVALIDARG, "Restored frame exceeds the window size"));
    }
    Ok(Rect::new(
        Point2D::new(outer.left - frame.left, outer.top - frame.top) + offset,
        size))
}

/// Converts Euclid geometry back to a native rectangle in the same coordinate space.
fn rect_into_native(rect: Rect<i32>) -> RECT {
    RECT { left: rect.min_x(), top: rect.min_y(), right: rect.max_x(), bottom: rect.max_y() }
}

/// Updates restored placement only, preserving the current show command and flags.
fn resize_restored_window(handle: HWND, size: Size2D<i32>) -> Result<()> {
    let mut placement = native::get_window_placement(handle)?;
    let frame = native::get_normal_frame(handle)?;
    let content = restored_content_rect(&placement.rcNormalPosition, &frame, Vector2D::zero())?;
    let resized = native::resize_rect(content, size)?;
    let outer_size = native::checked_size_sum(size, native::rect_from_native(&frame).size)?;
    placement.rcNormalPosition = rect_into_native(Rect::new(
        resized.origin + Vector2D::new(frame.left, frame.top), outer_size));
    native::set_window_placement(handle, &placement)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use windows::core::w;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_OVERLAPPEDWINDOW,
    };

    use super::*;

    /// Owns an invisible test window; no existing desktop windows are touched.
    struct TestWindow(HWND);

    impl TestWindow {
        /// Uses the predefined STATIC class so no global class registration is needed.
        fn new() -> Self {
            // SAFETY: Both strings are static and the predefined class needs no instance or user data.
            let handle = unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE::default(), w!("STATIC"), w!("TurboZone snapshot test"),
                    WS_OVERLAPPEDWINDOW, 100, 100, 800, 600, None, None, None, None)
            }.unwrap();
            Self(handle)
        }
    }

    impl Drop for TestWindow {
        fn drop(&mut self) {
            // SAFETY: This fixture owns the window and drops on its creating thread.
            let _ = unsafe { DestroyWindow(self.0) };
        }
    }

    #[test]
    fn successful_capture_and_actions_use_the_same_client_geometry() {
        let window = TestWindow::new();
        let handle = WindowHandle(window.0);
        let mut enumerator = WindowEnumerator::default();
        let before = enumerator.snapshot_window(window.0).detail.unwrap();
        resize_window(handle, Size2D::new(641, 481)).unwrap();
        let resized = enumerator.snapshot_window(window.0).detail.unwrap();
        assert_eq!(resized.content_rect.size, Size2D::new(641, 481));
        assert_eq!(resized.content_rect.center(), before.content_rect.center());
        center_window(handle).unwrap();
        let centered = enumerator.snapshot_window(window.0).detail.unwrap();
        assert!(centered.is_centered());
        assert_eq!(centered.content_rect.size, resized.content_rect.size);
        assert!(!centered.program_path.is_empty() && !centered.program_name.is_empty());
    }

    #[test]
    fn monitor_query_is_cached_until_the_next_refresh() {
        let mut enumerator = WindowEnumerator::default();
        let calls = Cell::new(0);
        let query = |_| {
            calls.set(calls.get() + 1);
            Ok(MONITORINFO::default())
        };
        enumerator.monitor_info(HMONITOR::default(), query).unwrap();
        enumerator.monitor_info(HMONITOR::default(), query).unwrap();
        assert_eq!(calls.get(), 1);
        enumerator.monitor_info_cache.clear();
        enumerator.monitor_info(HMONITOR::default(), query).unwrap();
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn failed_monitor_query_is_shared_within_the_refresh() {
        let mut enumerator = WindowEnumerator::default();
        enumerator.monitor_info(HMONITOR::default(), |_| {
            Err(Error::new(E_INVALIDARG, "monitor disappeared"))
        }).unwrap_err();
        let error = enumerator.monitor_info(HMONITOR::default(), |_| {
            panic!("failed query must also be cached");
        }).unwrap_err();
        assert!(error.to_string().contains("monitor disappeared"));
    }

    #[test]
    fn invalid_window_retains_identity_and_the_first_native_error() {
        let mut enumerator = WindowEnumerator::default();
        let window = enumerator.snapshot_window(HWND::default());
        assert_eq!(window.handle, WindowHandle::default());
        let error = window.detail.unwrap_err();
        assert_eq!(error.to_string(), "Client geometry");
        assert!(error.downcast_ref::<Error>().is_some());
    }

    #[test]
    fn restored_content_uses_frame_and_workspace_offsets() {
        let outer = RECT { left: -1900, top: 20, right: -1220, bottom: 550 };
        let frame = RECT { left: -8, top: -31, right: 8, bottom: 8 };
        let content = restored_content_rect(&outer, &frame, Vector2D::new(0, 40)).unwrap();
        assert_eq!(content, Rect::new(Point2D::new(-1892, 91), Size2D::new(664, 491)));
    }

    #[test]
    fn restored_content_rejects_impossible_frame_geometry() {
        let outer = RECT { left: 0, top: 0, right: 10, bottom: 10 };
        let frame = RECT { left: -8, top: -31, right: 8, bottom: 8 };
        assert_eq!(restored_content_rect(&outer, &frame, Vector2D::zero()).unwrap_err().code(), E_INVALIDARG);
    }

}
