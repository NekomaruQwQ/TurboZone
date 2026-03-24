use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt as _;
use std::path::PathBuf;

use euclid::default::*;

use windows::core::*;
use windows::Win32::{
    Foundation::*,
    Graphics::Dwm::*,
    Graphics::Gdi::*,
    System::Threading::*,
    UI::WindowsAndMessaging::*,
};

pub fn enumerate_windows() -> Result<Vec<HWND>> {
    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: `lparam` carries a pointer to a stack-local `Vec<HWND>` created
        // in `enumerate_windows()` below. The pointer is non-null, properly aligned,
        // and valid for the entire synchronous duration of `EnumWindows`. No aliasing
        // occurs because the callback is invoked sequentially — each `&mut Vec` exists
        // only within a single callback invocation.
        unsafe {
            (lparam.0 as *mut Vec<HWND>)
                .as_mut_unchecked()
                .push(hwnd);
            TRUE
        }
    }

    let mut out = Vec::new();
    let out_ptr = &raw mut out;

    // SAFETY: The callback has the correct `extern "system"` ABI and signature.
    // `LPARAM` carries a valid pointer to `out`, which lives on the stack and
    // outlives the synchronous `EnumWindows` call.
    unsafe { EnumWindows(Some(enum_proc), LPARAM(out_ptr as _)) }.map(|()| out)
}

/// Checks whether a window is "cloaked" (hidden by DWM).
/// Cloaked windows are technically visible but not shown to the user — common
/// with UWP app placeholders and windows on other virtual desktops.
pub fn is_cloaked(hwnd: HWND) -> bool {
    let mut cloaked: u32 = 0;
    let cloaked_ptr = &raw mut cloaked;

    // SAFETY: `cloaked` is a stack-local `u32`; its raw pointer is valid and
    // properly aligned. The buffer size (`size_of::<u32>()`) matches the type
    // expected by `DWMWA_CLOAKED`.
    let hr = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            cloaked_ptr.cast(),
            size_of::<u32>() as u32)
    };
    hr.is_ok() && cloaked != 0
}

/// Returns the client-area size `(width, height)` of a window, or `(0, 0)` on failure.
///
/// Uses `GetClientRect` because Windows Graphics Capture captures the client area,
/// so these dimensions match the captured texture size.
pub fn get_client_size(hwnd: HWND) -> Result<Size2D<u32>> {
    let mut rect = RECT::default();
    // SAFETY: `hwnd` is a valid enumerated handle; `&raw mut rect` is a valid local.
    unsafe { GetClientRect(hwnd, &raw mut rect) }?;
    Ok(Size2D::new(
        (rect.right - rect.left) as u32,
        (rect.bottom - rect.top) as u32))
}

/// Returns the window title as a `String`, or an empty string on failure.
///
/// Uses `GetWindowTextLengthW` and `GetWindowTextW` to retrieve the title as UTF-16,
/// then converts it to a Rust `String`. The conversion is lossy and replaces invalid
/// UTF-16 sequences with the Unicode replacement character, but this is acceptable
/// for display purposes.
pub fn get_window_text(hwnd: HWND) -> String {
    // SAFETY: Simple query with no pointer arguments beyond `hwnd`.
    let buf_len = unsafe { GetWindowTextLengthW(hwnd) } as usize + 1;
    let mut buf = vec![0u16; buf_len];

    // SAFETY: `hwnd` is a valid enumerated handle; `&mut buf` is a valid
    // buffer of `u16`, and `GetWindowTextW` writes at most `buf_len`
    // elements including the null terminator.
    let len = unsafe { GetWindowTextW(hwnd, &mut buf) } as usize;
    OsString::from_wide(&buf[..len])
        .to_string_lossy()
        .into_owned()
}

/// Returns the process ID of the window's owning process, or `0` on failure
/// (e.g. elevated process).
pub fn get_process_id(hwnd: HWND) -> u32 {
    let mut pid = 0;
    // SAFETY: `hwnd` is a valid enumerated handle; `&raw mut pid` is a valid local.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&raw mut pid)); }
    pid
}

/// Returns the full executable path of a process given its ID, or `None` on failure
/// (e.g. elevated process, system process, or process that has already exited).
pub fn get_executable_path(pid: u32) -> Option<PathBuf> {
    // SAFETY: `pid` is a non-zero process ID obtained from `GetWindowThreadProcessId`.
    // `OpenProcess` with `QUERY_LIMITED_INFORMATION` is a low-privilege operation.
    // `buf` is a stack-allocated 260-element u16 array (MAX_PATH). `CloseHandle` is
    // always called on the opened handle before returning.
    #[expect(clippy::multiple_unsafe_ops_per_block, reason = "Windows API calls")]
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        if handle.is_invalid() {
            None?;
        }

        let mut buf = [0u16; 260];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &raw mut len);
        let _ = CloseHandle(handle);
        ok.ok()?;

        Some(PathBuf::from(OsString::from_wide(&buf[..len as usize])))
    }
}

/// Returns the [`MONITORINFO`] of the monitor that a window is currently on,
/// or `None` if it cannot be determined.
pub fn get_monitor_info_from_window(hwnd: HWND) -> Option<MONITORINFO> {
    // SAFETY: `MONITOR_DEFAULTTOPRIMARY` guarantees a valid `HMONITOR` is
    // returned, falling back to the primary monitor.
    let hmonitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY) };
    let mut monitor_info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as _,
        ..Default::default()
    };

    // SAFETY: `monitor_info` is stack-local with `cbSize` correctly initialized
    // to `size_of::<MONITORINFO>()`; its raw pointer is valid for the call.
    unsafe { GetMonitorInfoW(hmonitor, &raw mut monitor_info) }
        .as_bool()
        .then_some(monitor_info)
}

/// Returns the [`WINDOWPLACEMENT`] of a window, which includes the restored
/// position (`rcNormalPosition`) and the current show state (`showCmd`).
pub fn get_window_placement(hwnd: HWND) -> Result<WINDOWPLACEMENT> {
    let mut placement = WINDOWPLACEMENT {
        length: size_of::<WINDOWPLACEMENT>() as u32,
        ..Default::default()
    };
    // SAFETY: `placement` is stack-local with `length` correctly initialized;
    // its raw pointer is valid for the duration of the call.
    unsafe { GetWindowPlacement(hwnd, &raw mut placement) }?;
    Ok(placement)
}

/// Sets the [`WINDOWPLACEMENT`] of a window. This updates the restored
/// position (`rcNormalPosition`) and show state (`showCmd`) without forcing
/// the window to change its current visual state when `showCmd` matches
/// the current state.
pub fn set_window_placement(hwnd: HWND, placement: &WINDOWPLACEMENT) -> Result<()> {
    // SAFETY: `placement` is a valid reference with `length` correctly initialized;
    // `SetWindowPlacement` reads from the pointer without modifying it.
    unsafe { SetWindowPlacement(hwnd, placement) }
}

/// Computes the total window-frame overhead (border + title bar) for `hwnd`
/// by querying its window style and calling `AdjustWindowRectEx` on a zero rect.
///
/// Returns a [`Size2D`] representing how much larger the window rectangle is
/// than the client area. This is style-derived, so it is correct regardless of
/// whether the window is currently maximized or minimized.
pub fn get_normal_frame(hwnd: HWND) -> Result<Size2D<i32>> {
    // SAFETY: `GetWindowLongW` is a simple query on `hwnd` with no pointer arguments.
    #[expect(clippy::multiple_unsafe_ops_per_block, reason = "Windows API calls")]
    let (style, ex_style) = unsafe {(
        WINDOW_STYLE(GetWindowLongW(hwnd, GWL_STYLE) as u32),
        WINDOW_EX_STYLE(GetWindowLongW(hwnd, GWL_EXSTYLE) as u32),
    )};
    // SAFETY: `GetMenu` is a simple query returning a menu handle (or null).
    let has_menu = unsafe { !GetMenu(hwnd).is_invalid() };
    let mut rect = RECT::default();
    // SAFETY: `rect` is stack-local; its raw pointer is valid for the call.
    // `style`, `ex_style`, and `has_menu` are derived from prior successful queries.
    unsafe { AdjustWindowRectEx(&raw mut rect, style, has_menu, ex_style) }?;
    Ok(Size2D::new(rect.right - rect.left, rect.bottom - rect.top))
}

/// Returns the restored client size of a window by subtracting the normal-state
/// frame from `rcNormalPosition` in the window's [`WINDOWPLACEMENT`].
///
/// For normal windows this matches `GetClientRect`; for maximized/minimized
/// windows it returns the size the client area *will* have when the window is
/// restored.
pub fn get_restored_client_size(hwnd: HWND) -> Result<Size2D<u32>> {
    let placement = get_window_placement(hwnd)?;
    let frame = get_normal_frame(hwnd)?;
    let rc = placement.rcNormalPosition;
    let window_size = Size2D::new(rc.right - rc.left, rc.bottom - rc.top);
    let client_size = window_size - frame;
    Ok(Size2D::new(client_size.width as u32, client_size.height as u32))
}

pub fn resize_client(hwnd: HWND, size: Size2D<i32>) -> Result<()> {
    let mut window_rect = RECT::default();
    let mut client_rect = RECT::default();

    // SAFETY: Both `RECT`s are stack-local; their raw pointers are valid for
    // the duration of each call.
    unsafe { GetWindowRect(hwnd, &raw mut window_rect) }?;
    // SAFETY: Mentioned above.
    unsafe { GetClientRect(hwnd, &raw mut client_rect) }?;

    let old_position =
        Point2D::new(
            window_rect.left,
            window_rect.top);
    let old_window_size =
        Size2D::new(
            window_rect.right  - window_rect.left,
            window_rect.bottom - window_rect.top);
    let old_client_size =
        Size2D::new(
            client_rect.right  - client_rect.left,
            client_rect.bottom - client_rect.top);
    let new_window_size = old_window_size + size - old_client_size;
    let new_position =
        old_position - (new_window_size - old_window_size).to_vector() / 2;

    // SAFETY: All positional/size arguments are computed from prior successful
    // Win32 API calls; flag constants are valid.
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
