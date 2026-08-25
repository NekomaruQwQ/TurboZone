//! Low-level Win32 queries and mutations kept behind the safe crate API.

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt as _;
use std::path::PathBuf;

use euclid::default::{Point2D, Rect, Size2D, Vector2D};
use windows::core::{BOOL, Error, PWSTR, Result};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

/// Enumerates every top-level desktop window handle.
pub fn enumerate_windows() -> Result<Vec<HWND>> {
    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: LPARAM holds the valid stack-local output vector for the
        // synchronous duration of EnumWindows. Callbacks are sequential.
        unsafe {
            (lparam.0 as *mut Vec<HWND>)
                .as_mut_unchecked()
                .push(hwnd);
        }
        TRUE
    }

    let mut out = Vec::new();
    let out_ptr = &raw mut out;

    // SAFETY: The callback ABI matches EnumWindows and the pointer remains valid
    // for the entire synchronous call.
    unsafe { EnumWindows(Some(enum_proc), LPARAM(out_ptr as _)) }.map(|()| out)
}

/// Returns whether a handle represents a visible, unowned, uncloaked app window.
pub fn is_app_window(hwnd: HWND) -> bool {
    // SAFETY: These functions only query the supplied handle.
    unsafe { IsWindowVisible(hwnd) }.as_bool() && !is_owned(hwnd) && !is_cloaked(hwnd)
}

/// Returns whether another window owns this handle.
fn is_owned(hwnd: HWND) -> bool {
    // SAFETY: GetWindow only queries the supplied handle.
    !unsafe { GetWindow(hwnd, GW_OWNER) }
        .unwrap_or_default()
        .is_invalid()
}

/// Returns whether DWM currently hides this window from the user.
fn is_cloaked(hwnd: HWND) -> bool {
    let mut cloaked = 0u32;

    // SAFETY: The output pointer is aligned and valid, and its byte size matches
    // the DWMWA_CLOAKED value requested.
    let result = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            (&raw mut cloaked).cast(),
            size_of::<u32>() as u32)
    };
    result.is_ok() && cloaked != 0
}

/// Returns whether Windows reports the window as minimized.
pub fn is_minimized(hwnd: HWND) -> bool {
    // SAFETY: IsIconic only queries the supplied handle.
    unsafe { IsIconic(hwnd) }.as_bool()
}

/// Returns whether Windows reports the window as maximized.
pub fn is_maximized(hwnd: HWND) -> bool {
    // SAFETY: IsZoomed only queries the supplied handle.
    unsafe { IsZoomed(hwnd) }.as_bool()
}

/// Reads the outer window rectangle.
pub fn get_window_rect(hwnd: HWND) -> Result<RECT> {
    let mut rect = RECT::default();
    // SAFETY: The output RECT remains valid for the call.
    unsafe { GetWindowRect(hwnd, &raw mut rect) }?;
    Ok(rect)
}

/// Reads the client-area rectangle relative to its own origin.
fn get_client_rect(hwnd: HWND) -> Result<RECT> {
    let mut rect = RECT::default();
    // SAFETY: The output RECT remains valid for the call.
    unsafe { GetClientRect(hwnd, &raw mut rect) }?;
    Ok(rect)
}

/// Reads live client geometry in screen coordinates, failing if either query fails.
pub fn get_content_rect(hwnd: HWND) -> Result<Rect<i32>> {
    let rect = get_client_rect(hwnd)?;
    let mut origin = POINT { x: rect.left, y: rect.top };
    // SAFETY: The output point is valid and no pointer is retained.
    if !unsafe { ClientToScreen(hwnd, &raw mut origin) }.as_bool() {
        // ClientToScreen does not document GetLastError; do not report a stale code.
        return Err(Error::new(E_FAIL, "ClientToScreen failed"));
    }
    Ok(Rect::new(
        Point2D::new(origin.x, origin.y),
        Size2D::new(rect.right - rect.left, rect.bottom - rect.top)))
}

/// Reads a lossy UTF-8 window title, returning an empty string on query failure.
pub fn get_window_text(hwnd: HWND) -> String {
    // SAFETY: GetWindowTextLengthW only queries the supplied handle.
    let buffer_length = unsafe { GetWindowTextLengthW(hwnd) } as usize + 1;
    let mut buffer = vec![0u16; buffer_length];

    // SAFETY: The buffer is writable and sized for the length reported above.
    let length = unsafe { GetWindowTextW(hwnd, &mut buffer) } as usize;
    OsString::from_wide(&buffer[..length])
        .to_string_lossy()
        .into_owned()
}

/// Reads the owning process ID, returning the native error if the window is gone.
pub fn get_process_id(hwnd: HWND) -> Result<u32> {
    let mut process_id = 0;
    // SAFETY: The output process ID remains valid for the call.
    if unsafe { GetWindowThreadProcessId(hwnd, Some(&raw mut process_id)) } == 0 {
        return Err(Error::from_thread());
    }
    Ok(process_id)
}

/// Reads a process executable path when limited-information access is available.
/// Returns access or query errors without discarding the native diagnostic.
pub fn get_executable_path(process_id: u32) -> Result<PathBuf> {
    // SAFETY: OpenProcess receives a PID returned by Windows. The query buffer
    // remains valid, and every successfully opened handle is closed.
    #[expect(clippy::multiple_unsafe_ops_per_block, reason = "one Win32 handle lifetime")]
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id)?;

        // QueryFullProcessImageNameW supports long paths; this is the documented
        // maximum Win32 path length including the terminator.
        let mut buffer = vec![0u16; 0x8000];
        let mut length = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &raw mut length);
        let _ = CloseHandle(handle);
        result?;

        Ok(PathBuf::from(OsString::from_wide(&buffer[..length as usize])))
    }
}

/// Finds the most-overlapped monitor, falling back to the primary when off-screen.
pub fn get_monitor(hwnd: HWND) -> HMONITOR {
    // SAFETY: MonitorFromWindow only queries the handle and retains no pointers.
    unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY) }
}

/// Reads monitor geometry. Failure has no documented extended Win32 error code.
pub fn get_monitor_info(monitor: HMONITOR) -> Result<MONITORINFO> {
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };

    // SAFETY: cbSize is initialized and the output pointer remains valid.
    if !unsafe { GetMonitorInfoW(monitor, &raw mut info) }.as_bool() {
        return Err(Error::new(E_FAIL, "GetMonitorInfoW failed"));
    }
    Ok(info)
}

/// Reads live and restored window-placement state.
pub fn get_window_placement(hwnd: HWND) -> Result<WINDOWPLACEMENT> {
    let mut placement = WINDOWPLACEMENT {
        length: size_of::<WINDOWPLACEMENT>() as u32,
        ..Default::default()
    };
    // SAFETY: length is initialized and the output pointer remains valid.
    unsafe { GetWindowPlacement(hwnd, &raw mut placement) }?;
    Ok(placement)
}

/// Writes restored geometry without changing the supplied show state.
pub fn set_window_placement(
    hwnd: HWND,
    placement: &WINDOWPLACEMENT) -> Result<()> {
    // SAFETY: placement has the required initialized length and is only read.
    unsafe { SetWindowPlacement(hwnd, placement) }
}

/// Queries a style word, distinguishing a valid zero from native failure.
fn get_window_style(hwnd: HWND, index: WINDOW_LONG_PTR_INDEX) -> Result<u32> {
    // SAFETY: These calls use only thread-local error state and query the handle.
    #[expect(clippy::multiple_unsafe_ops_per_block, reason = "one last-error transaction")]
    unsafe {
        SetLastError(ERROR_SUCCESS);
        let value = GetWindowLongW(hwnd, index);
        if value == 0 && GetLastError() != ERROR_SUCCESS {
            return Err(Error::from_thread());
        }
        Ok(value as u32)
    }
}

/// Computes standard restored-frame offsets around a zero-sized client rectangle.
/// Custom non-client layouts and wrapped menus cannot be inferred from styles.
pub fn get_normal_frame(hwnd: HWND) -> Result<RECT> {
    let style = WINDOW_STYLE(get_window_style(hwnd, GWL_STYLE)?) & !(WS_MINIMIZE | WS_MAXIMIZE);
    let extended_style = WINDOW_EX_STYLE(get_window_style(hwnd, GWL_EXSTYLE)?);
    // SAFETY: GetMenu only queries the supplied handle.
    let has_menu = unsafe { !GetMenu(hwnd).is_invalid() };
    // SAFETY: GetDpiForWindow only queries the supplied handle.
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi == 0 {
        return Err(Error::new(E_FAIL, "GetDpiForWindow failed"));
    }
    let mut rect = RECT::default();

    // SAFETY: The output RECT is valid, and styles came from the same window.
    unsafe { AdjustWindowRectExForDpi(&raw mut rect, style, has_menu, extended_style, dpi) }?;
    Ok(rect)
}

/// Returns the offset from workspace placement coordinates to screen coordinates.
/// Tool windows already use screen coordinates and need no offset.
pub fn get_placement_offset(hwnd: HWND, monitor: &MONITORINFO) -> Result<Vector2D<i32>> {
    let style = WINDOW_EX_STYLE(get_window_style(hwnd, GWL_EXSTYLE)?);
    Ok(if style.contains(WS_EX_TOOLWINDOW) {
        Vector2D::zero()
    } else {
        Vector2D::new(
            monitor.rcWork.left - monitor.rcMonitor.left,
            monitor.rcWork.top - monitor.rcMonitor.top)
    })
}

/// Converts a native rectangle without changing its coordinate space.
pub const fn rect_from_native(rect: &RECT) -> Rect<i32> {
    Rect::new(
        Point2D::new(rect.left, rect.top),
        Size2D::new(rect.right - rect.left, rect.bottom - rect.top))
}

/// Moves a live window without resizing, activating, or changing z-order.
pub fn set_window_position(hwnd: HWND, position: Point2D<i32>) -> Result<()> {
    // SAFETY: Position and flags are plain values; no pointers are retained.
    unsafe {
        SetWindowPos(
            hwnd,
            None,
            position.x,
            position.y,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOSIZE | SWP_NOZORDER)
    }
}

/// Resizes a live client area around its existing center.
pub fn resize_client(hwnd: HWND, size: Size2D<i32>) -> Result<()> {
    let outer = rect_from_native(&get_window_rect(hwnd)?);
    let content = get_content_rect(hwnd)?;
    let resized = resize_rect(content, size)?;
    let new_window_size = checked_size_sum(size, outer.size - content.size)?;
    let new_position = resized.origin + (outer.origin - content.origin);

    // SAFETY: Geometry is derived from successful Win32 queries; no pointers
    // are retained and the flags preserve activation and z-order.
    unsafe {
        SetWindowPos(
            hwnd,
            None,
            new_position.x,
            new_position.y,
            new_window_size.width,
            new_window_size.height,
            SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER)
    }
}

/// Adds frame overhead without allowing a large configured target to wrap.
pub fn checked_size_sum(size: Size2D<i32>, overhead: Size2D<i32>) -> Result<Size2D<i32>> {
    let width = size.width.checked_add(overhead.width);
    let height = size.height.checked_add(overhead.height);
    match (width, height) {
        (Some(width), Some(height)) if width > 0 && height > 0 => Ok(Size2D::new(width, height)),
        _ => Err(Error::new(E_INVALIDARG, "Window dimensions exceed the native coordinate range")),
    }
}

/// Resizes around the integer center, preserving odd sizes and rejecting coordinate overflow.
pub fn resize_rect(rect: Rect<i32>, size: Size2D<i32>) -> Result<Rect<i32>> {
    let x = i64::from(rect.origin.x) + i64::from(rect.size.width / 2) - i64::from(size.width / 2);
    let y = i64::from(rect.origin.y) + i64::from(rect.size.height / 2) - i64::from(size.height / 2);
    if size.width <= 0 || size.height <= 0
        || x < i64::from(i32::MIN) || x + i64::from(size.width) > i64::from(i32::MAX)
        || y < i64::from(i32::MIN) || y + i64::from(size.height) > i64::from(i32::MAX) {
        return Err(Error::new(E_INVALIDARG, "Client rectangle exceeds the native coordinate range"));
    }
    Ok(Rect::new(Point2D::new(x as i32, y as i32), size))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_preserves_integer_center_across_odd_dimensions() {
        let old = Rect::new(Point2D::new(-1000, 123), Size2D::new(641, 481));
        let new = resize_rect(old, Size2D::new(800, 600)).unwrap();
        assert_eq!((new.center(), new.size), (old.center(), Size2D::new(800, 600)));
    }

    #[test]
    fn resize_rejects_coordinate_overflow() {
        let old = Rect::new(Point2D::new(i32::MAX - 10, 0), Size2D::new(10, 10));
        assert_eq!(resize_rect(old, Size2D::new(100, 100)).unwrap_err().code(), E_INVALIDARG);
    }

    #[test]
    fn frame_overhead_cannot_overflow_a_large_resize_target() {
        assert_eq!(
            checked_size_sum(Size2D::new(i32::MAX, 100), Size2D::new(16, 39)).unwrap_err().code(),
            E_INVALIDARG);
    }
}
