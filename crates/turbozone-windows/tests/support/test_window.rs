//! Test-owned Win32 windows for exercising the public adapter without touching user windows.

use std::sync::atomic::{AtomicU64, Ordering};

use euclid::default::{Point2D, Rect, Size2D};
use smol_str::{SmolStr, format_smolstr};
use turbozone_core::WindowState;
use turbozone_windows::{Handle, window::get_content_rect};
use windows::core::{HSTRING, PCWSTR, w};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITORINFO, MONITOR_DEFAULTTOPRIMARY, MonitorFromWindow};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};

/// Distinguishes fixture windows when integration tests execute concurrently.
static NEXT_WINDOW: AtomicU64 = AtomicU64::new(0);

/// Owns a top-level window created by the current test and destroys it on the creating thread.
///
/// Hidden windows support mutation tests without desktop side effects. Snapshot tests use a
/// non-activating tool window positioned offscreen because the production backend intentionally
/// enumerates only visible top-level windows.
pub struct TestWindow {
    handle: HWND,
    title: SmolStr,
}

impl TestWindow {
    /// Creates an ordinary hidden window for geometry and action tests.
    pub fn hidden() -> Self { Self::new(false) }

    /// Creates a visible but non-activating offscreen window eligible for backend enumeration.
    pub fn visible_offscreen() -> Self { Self::new(true) }

    /// Uses the predefined STATIC class so fixtures require no process-global registration.
    fn new(visible: bool) -> Self {
        let sequence = NEXT_WINDOW.fetch_add(1, Ordering::Relaxed);
        let title = format_smolstr!("TurboZone integration test {}-{sequence}", std::process::id());
        let wide_title = HSTRING::from(title.as_str());
        let extended_style = if visible {
            WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW
        } else {
            WINDOW_EX_STYLE::default()
        };
        let style = if visible { WS_OVERLAPPEDWINDOW | WS_VISIBLE } else { WS_OVERLAPPEDWINDOW };
        let origin = if visible { -32_000 } else { 100 };

        // SAFETY: The predefined class needs no registration, strings live through the call,
        // and the fixture exclusively owns the returned top-level window.
        let handle = unsafe {
            CreateWindowExW(
                extended_style, w!("STATIC"), PCWSTR(wide_title.as_ptr()), style,
                origin, origin, 800, 600, None, None, None, None)
        }.unwrap();
        Self { handle, title }
    }

    /// Returns the product handle used by snapshots and deferred actions.
    pub const fn handle(&self) -> Handle<HWND> { Handle(self.handle) }

    /// Returns the unique title used to locate this fixture in a full desktop snapshot.
    pub fn title(&self) -> &str { &self.title }

    /// Queries client geometry through the public Windows adapter for the requested state path.
    pub fn content_rect(&self, state: WindowState) -> Rect<i32> {
        get_content_rect(self.handle, state, &self.monitor_info()).unwrap()
    }

    /// Returns the current monitor work area in the same Euclid representation as snapshots.
    pub fn monitor_rect(&self) -> Rect<i32> {
        let RECT { left, top, right, bottom } = self.monitor_info().rcWork;
        Rect::new(Point2D::new(left, top), Size2D::new(right - left, bottom - top))
    }

    /// Supplies the initialized monitor structure required by restored-geometry queries.
    fn monitor_info(&self) -> MONITORINFO {
        // SAFETY: The call only queries this fixture's live handle.
        let monitor = unsafe { MonitorFromWindow(self.handle, MONITOR_DEFAULTTOPRIMARY) };
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..MONITORINFO::default()
        };
        // SAFETY: The initialized output structure remains valid for the call.
        let succeeded = unsafe { GetMonitorInfoW(monitor, &raw mut info) }.as_bool();
        assert!(succeeded, "fixture monitor must remain queryable");
        info
    }
}

impl Drop for TestWindow {
    fn drop(&mut self) {
        // SAFETY: The fixture exclusively owns the window and drops on its creating thread.
        let _ = unsafe { DestroyWindow(self.handle) };
    }
}
