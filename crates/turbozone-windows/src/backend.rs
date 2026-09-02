use turbozone_core::{
    Backend      as CoreBackend,
    WindowInfo   as CoreWindowInfo,
    WindowState,
    WindowDetail,
    WindowAction as CoreWindowAction,
};

use crate::window::*;
use crate::native;
use crate::native::Convert as _;
use crate::{
    Handle,
    center_window,
    resize_window,
};

use std::collections::HashMap;

use anyhow::Context as _;
use euclid::default::Size2D;

use windows::core::Result;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{HMONITOR, MONITORINFO};

/// Predicate type for general filtering operations.
type Pred<T> = fn(&T) -> bool;

type WindowInfo   = CoreWindowInfo<Handle<HWND>>;
type WindowAction = CoreWindowAction<Handle<HWND>>;

type MonitorInfoCache = HashMap<Handle<HMONITOR>, Result<MONITORINFO>>;

const IGNORE_WINDOWS: &[Pred<WindowInfo>] = &[
    |window| window.title.is_empty(),
    |window| {
        window.title == "Program Manager" &&
        window.detail.as_ref().is_ok_and(|detail| {
            detail.program_name.eq_ignore_ascii_case("explorer.exe")
        })},
];

#[derive(Debug, Default)]
#[non_exhaustive]
pub struct Backend;

impl CoreBackend for Backend {
    type Handle = Handle<HWND>;

    fn snapshot(&mut self) -> anyhow::Result<Vec<WindowInfo>> {
        let mut monitor_info_cache = HashMap::new();

        Ok(native::enumerate_windows()?
            .into_iter()
            .filter(|&handle| native::is_app_window(handle))
            .map(|handle| snapshot_window(&mut monitor_info_cache, handle))
            .filter(|window| !IGNORE_WINDOWS.iter().any(|pred| pred(window)))
            .collect())
    }

    #[expect(clippy::panic_in_result_fn, reason = "an unknown action is a core/backend contract mismatch, not an operational failure")]
    fn perform(&mut self, action: WindowAction) -> anyhow::Result<()> {
        match action {
            WindowAction::Resize(handle, size@Size2D { width, height, .. }) =>
                resize_window(handle, size)
                    .with_context(|| format!("failed to resize window {handle} to {width}x{height}")),
            WindowAction::MoveToCenter(handle) =>
                center_window(handle)
                    .with_context(|| format!("failed to center window {handle}")),
            _ => panic!("Windows backend received an unsupported TurboZone action"),
        }
    }
}

/// Retains basic identity even when a detail query fails.
fn snapshot_window(
    monitor_info_cache: &mut MonitorInfoCache,
    handle: HWND) -> WindowInfo {
    let state = get_window_state(handle);
    WindowInfo {
        handle: Handle(handle),
        title: native::get_window_text(handle).into(),
        state,
        detail: snapshot_window_detail(monitor_info_cache, handle, state),
    }
}

/// Stops at the first failed query while retaining the original native error.
fn snapshot_window_detail(
    monitor_info_cache: &mut MonitorInfoCache,
    handle: HWND,
    state: WindowState)
    -> anyhow::Result<WindowDetail> {
    let monitor_handle = native::get_monitor(handle);
    let monitor =
        monitor_info_cache
            .entry(Handle(monitor_handle))
            .or_insert_with(|| native::get_monitor_info(monitor_handle))
            .clone()
            .context("failed to get monitor info")?;
    let content_rect =
        get_content_rect(handle, state, &monitor)
            .context("failed to get content rect")?;
    let process_id =
        native::get_process_id(handle)
            .context("failed to get process ID")?;
    let native_path =
        native::get_program_path(process_id)
            .context("failed to get program path")?;
    let program_name =
        native_path
            .file_name()
            .context("program path has no filename")?
            .to_string_lossy()
            .into_owned()
            .into();
    // Windows supplies normalized paths; only the separator convention changes.
    let program_path =
        native_path
            .to_string_lossy()
            .replace('\\', "/")
            .into();
    Ok(WindowDetail {
        monitor_rect: monitor.rcWork.convert(),
        content_rect,
        process_id,
        program_path,
        program_name,
    })
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use euclid::default::Size2D;

    use windows::core::w;
    use windows::Win32::Foundation::E_INVALIDARG;
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
        let handle = Handle(window.0);
        let mut enumerator = Backend::default();
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
        let mut enumerator = Backend::default();
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
        let mut enumerator = Backend::default();
        enumerator.monitor_info(HMONITOR::default(), |_| {
            Err(windows::core::Error::new(E_INVALIDARG, "monitor disappeared"))
        }).unwrap_err();
        let error = enumerator.monitor_info(HMONITOR::default(), |_| {
            panic!("failed query must also be cached");
        }).unwrap_err();
        assert!(error.to_string().contains("monitor disappeared"));
    }

    #[test]
    fn invalid_window_retains_identity_and_the_first_native_error() {
        let mut enumerator = Backend::default();
        let window = enumerator.snapshot_window(HWND::default());
        assert_eq!(window.handle, Handle(HWND::default()));
        let error = window.detail.unwrap_err();
        assert_eq!(error.to_string(), "Client geometry");
        assert!(error.downcast_ref::<windows::core::Error>().is_some());
    }
}
