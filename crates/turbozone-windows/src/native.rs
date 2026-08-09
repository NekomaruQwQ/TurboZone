//! Low-level Win32 queries and mutations kept behind the safe crate API.

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt as _;
use std::path::PathBuf;

use euclid::default::{Point2D, Size2D};
use windows::core::{BOOL, PWSTR, Result};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Threading::*;
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

/// Reads the live client-area size in physical pixels.
pub fn get_client_size(hwnd: HWND) -> Result<Size2D<i32>> {
    let rect = get_client_rect(hwnd)?;
    Ok(Size2D::new(rect.right - rect.left, rect.bottom - rect.top))
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

/// Reads the owning process ID, returning zero on query failure.
pub fn get_process_id(hwnd: HWND) -> u32 {
    let mut process_id = 0;
    // SAFETY: The output process ID remains valid for the call.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&raw mut process_id)); }
    process_id
}

/// Reads a process executable path when limited-information access is available.
pub fn get_executable_path(process_id: u32) -> Option<PathBuf> {
    // SAFETY: OpenProcess receives a PID returned by Windows. The query buffer
    // remains valid, and every successfully opened handle is closed.
    #[expect(clippy::multiple_unsafe_ops_per_block, reason = "one Win32 handle lifetime")]
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id).ok()?;
        if handle.is_invalid() {
            return None;
        }

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
        result.ok()?;

        Some(PathBuf::from(OsString::from_wide(&buffer[..length as usize])))
    }
}

/// Reads the nearest monitor work area, falling back to the primary monitor.
pub fn get_monitor_info_from_window(hwnd: HWND) -> Option<MONITORINFO> {
    // SAFETY: MONITOR_DEFAULTTOPRIMARY guarantees a fallback monitor.
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY) };
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };

    // SAFETY: cbSize is initialized and the output pointer remains valid.
    unsafe { GetMonitorInfoW(monitor, &raw mut info) }
        .as_bool()
        .then_some(info)
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

/// Computes normal-state frame overhead from the window styles.
pub fn get_normal_frame(hwnd: HWND) -> Result<Size2D<i32>> {
    // SAFETY: GetWindowLongW only queries the supplied handle.
    #[expect(clippy::multiple_unsafe_ops_per_block, reason = "paired style queries")]
    let (style, extended_style) = unsafe {(
        WINDOW_STYLE(GetWindowLongW(hwnd, GWL_STYLE) as u32),
        WINDOW_EX_STYLE(GetWindowLongW(hwnd, GWL_EXSTYLE) as u32),
    )};
    // SAFETY: GetMenu only queries the supplied handle.
    let has_menu = unsafe { !GetMenu(hwnd).is_invalid() };
    let mut rect = RECT::default();

    // SAFETY: The output RECT is valid, and styles came from the same window.
    unsafe { AdjustWindowRectEx(&raw mut rect, style, has_menu, extended_style) }?;
    Ok(Size2D::new(rect.right - rect.left, rect.bottom - rect.top))
}

/// Computes restored client size from placement and normal frame overhead.
pub fn get_restored_client_size(hwnd: HWND) -> Result<Size2D<i32>> {
    let placement = get_window_placement(hwnd)?;
    let frame = get_normal_frame(hwnd)?;
    let rect = placement.rcNormalPosition;
    let window_size = Size2D::new(rect.right - rect.left, rect.bottom - rect.top);
    Ok(window_size - frame)
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
    let window_rect = get_window_rect(hwnd)?;
    let client_rect = get_client_rect(hwnd)?;
    let old_position = Point2D::new(window_rect.left, window_rect.top);
    let old_window_size = Size2D::new(
        window_rect.right - window_rect.left,
        window_rect.bottom - window_rect.top);
    let old_client_size = Size2D::new(
        client_rect.right - client_rect.left,
        client_rect.bottom - client_rect.top);
    let new_window_size = old_window_size + size - old_client_size;
    let new_position = old_position - (new_window_size - old_window_size).to_vector() / 2;

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
