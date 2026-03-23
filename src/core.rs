use std::path::PathBuf;

use euclid::default::Size2D;
use win32_version_info::VersionInfo;

use windows::core::*;
use windows::Win32::{
    Foundation::*,
    UI::WindowsAndMessaging::*,
};

use crate::native::*;

/// The visual state of a window — keeps Win32 constants (`SW_*`) out of the
/// domain layer and provides clean pattern matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WindowState {
    Normal,
    Maximized,
    Minimized,
}

/// Returns the current visual state of a window.
pub fn get_window_state(hwnd: HWND) -> WindowState {
    // SAFETY: `IsIconic` and `IsZoomed` are simple boolean queries on `hwnd`
    // with no pointer arguments.
    #[expect(clippy::multiple_unsafe_ops_per_block, reason = "Windows API calls")]
    unsafe {
        if IsIconic(hwnd).as_bool() {
            WindowState::Minimized
        } else if IsZoomed(hwnd).as_bool() {
            WindowState::Maximized
        } else {
            WindowState::Normal
        }
    }
}

pub const RESOLUTION_GROUPS: &[(&str, &[SIZE])] = &[
    ("16:10", RESOLUTIONS_16_10),
];

pub const RESOLUTIONS_16_10: &[SIZE] = &[
    SIZE { cx: 3840, cy: 2400 },
    SIZE { cx: 2880, cy: 1800 },
    SIZE { cx: 2560, cy: 1600 },
    SIZE { cx: 1920, cy: 1200 },
    SIZE { cx: 1680, cy: 1050 },
    SIZE { cx: 1440, cy:  900 },
    SIZE { cx: 1280, cy:  800 },
    SIZE { cx:  960, cy:  600 },
    SIZE { cx:  800, cy:  500 },
    SIZE { cx:  640, cy:  400 },
    SIZE { cx:  480, cy:  300 },
];

pub fn is_known_resolution(width: u32, height: u32) -> bool {
    RESOLUTION_GROUPS
        .iter()
        .flat_map(|&(_, arr)| arr)
        .any(|&item| {
            item.cx == width as i32 &&
            item.cy == height as i32})
}

pub const fn get_center_of_rect(rect: &RECT) -> POINT {
    POINT {
        x: rect.left + (rect.right  - rect.left) / 2,
        y: rect.top  + (rect.bottom - rect.top ) / 2,
    }
}

fn get_display_name_for_executable(path: &PathBuf)
    -> Option<String> {
    VersionInfo::from_file(path)
        .map(|info| info.file_description)
        .ok()
        .or_else(|| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
}

pub struct ExecutableInfo {
    pub display_path: String,
    pub display_name: Option<String>,
}

impl ExecutableInfo {
    pub fn from_path(path: &PathBuf) -> Self {
        Self {
            display_path: path.to_string_lossy().into_owned(),
            display_name: get_display_name_for_executable(path),
        }
    }
}

pub struct WindowInfo {
    /// Window handle.
    pub hwnd: HWND,
    /// Window title (lossy UTF-16 → UTF-8 conversion).
    pub window_text: String,
    /// Current visual state of the window (normal, maximized, or minimized).
    pub state: WindowState,
    /// "Controllable" client-area size in physical pixels, or `None` if unavailable.
    /// For normal windows this is the live client rect; for maximized/minimized
    /// windows it is the *restored* client size (the size the window will have
    /// when un-maximized/un-minimized).
    pub client_size: Option<Size2D<u32>>,
    /// Whether the window is centered on the screen, or `None` if it cannot be
    /// determined (e.g. due to missing monitor info or window rect).
    /// For maximized/minimized windows this checks the *restored* position.
    pub is_centered: Option<bool>,
    /// Full executable path, or empty if inaccessible.
    pub executable_path: Option<PathBuf>,
}

impl WindowInfo {
    /// Centers this window on its screen.
    /// For maximized/minimized windows, adjusts the restored position.
    pub fn center(&self) -> Result<()> {
        match self.state {
            WindowState::Normal =>
                center_to_screen(self.hwnd),
            WindowState::Maximized | WindowState::Minimized =>
                center_restored_to_screen(self.hwnd),
        }
    }

    /// Resizes this window's client area to `(width, height)`.
    /// For maximized/minimized windows, adjusts the restored size.
    pub fn resize(&self, width: i32, height: i32) -> Result<()> {
        match self.state {
            WindowState::Normal =>
                resize_client(self.hwnd, width, height),
            WindowState::Maximized | WindowState::Minimized =>
                resize_restored_client(self.hwnd, width, height),
        }
    }

    pub fn from_hwnd(hwnd: HWND) -> Self {
        let window_text =
            get_window_text(hwnd);
        let state =
            get_window_state(hwnd);
        let (client_size, is_centered) = match state {
            WindowState::Normal => (
                get_client_size(hwnd).ok(),
                is_centered(hwnd)),
            WindowState::Maximized | WindowState::Minimized => (
                get_restored_client_size(hwnd).ok(),
                is_restored_centered(hwnd)),
        };
        let process_id =
            get_process_id(hwnd);
        let executable_path =
            get_executable_path(process_id);
        Self {
            hwnd,
            window_text,
            state,
            client_size,
            is_centered,
            executable_path,
        }
    }
}

/// Checks if a window is active and should be included in the list of windows
/// that can be manipulated by the user. This function filters out windows that
/// are not visible, owned by other windows, or cloaked by the Desktop Window
/// Manager (DWM). Maximized and minimized windows are included — their
/// restored geometry can be inspected and modified via `WINDOWPLACEMENT`.
pub fn is_active(hwnd: HWND) -> bool {
    // SAFETY: `IsWindowVisible` and `GetWindow` are simple boolean/handle
    // queries on `hwnd` with no pointer arguments.
    #[expect(clippy::multiple_unsafe_ops_per_block, reason = "Windows API calls")]
    unsafe {
        IsWindowVisible(hwnd).as_bool()
        // Exclude owned windows, which are typically tooltips, popups, and other
        // auxiliary windows that shouldn't be treated as main application windows.
        && GetWindow(hwnd, GW_OWNER)
            .unwrap_or_default()
            .is_invalid()
        // Exclude cloaked windows, which are technically visible but not shown to
        // the user.
        && !is_cloaked(hwnd)
    }
}

pub fn is_centered(hwnd: HWND) -> Option<bool> {
    let monitor_info = get_monitor_info_from_window(hwnd)?;
    let mut window_rect = RECT::default();

    // SAFETY: `window_rect` is stack-local; its raw pointer is valid
    // for the duration of the call.
    unsafe { GetWindowRect(hwnd, &raw mut window_rect) }.ok()?;
    let screen_center = get_center_of_rect(&monitor_info.rcWork);
    let window_center = get_center_of_rect(&window_rect);
    Some(window_center == screen_center)
}

pub fn center_to_screen(hwnd: HWND) -> Result<()> {
    let Some(monitor_info) = get_monitor_info_from_window(hwnd) else {
        return Err(Error::empty());
    };
    let mut window_rect = RECT::default();
    // SAFETY: `window_rect` is stack-local; its raw pointer is valid for the
    // duration of the call.
    unsafe { GetWindowRect(hwnd, &raw mut window_rect) }?;
    // SAFETY: Positional arguments are computed from prior successful API
    // calls (`get_monitor_info_from_window`, `GetWindowRect`); `SWP_NOSIZE`
    // makes the width/height arguments (0, 0) ignored; flag constants are valid.
    unsafe {
        SetWindowPos(
            hwnd,
            None,
            monitor_info.rcWork.left
                + (monitor_info.rcWork.right - monitor_info.rcWork.left) / 2
                - (window_rect.right - window_rect.left) / 2,
            monitor_info.rcWork.top
                + (monitor_info.rcWork.bottom - monitor_info.rcWork.top) / 2
                - (window_rect.bottom - window_rect.top) / 2,
            0,
            0,
            SWP_NOACTIVATE |
            SWP_NOOWNERZORDER |
            SWP_NOSIZE |
            SWP_NOZORDER)
    }
}

/// Checks whether the *restored* position (`rcNormalPosition`) of a maximized
/// or minimized window is centered on the monitor work area.
pub fn is_restored_centered(hwnd: HWND) -> Option<bool> {
    let monitor_info = get_monitor_info_from_window(hwnd)?;
    let placement = get_window_placement(hwnd).ok()?;
    let screen_center = get_center_of_rect(&monitor_info.rcWork);
    let window_center = get_center_of_rect(&placement.rcNormalPosition);
    Some(window_center == screen_center)
}

/// Centers the *restored* position (`rcNormalPosition`) of a maximized or
/// minimized window on the monitor work area via `SetWindowPlacement`,
/// without changing the window's current show state.
pub fn center_restored_to_screen(hwnd: HWND) -> Result<()> {
    let Some(monitor_info) = get_monitor_info_from_window(hwnd) else {
        return Err(Error::empty());
    };
    let mut placement = get_window_placement(hwnd)?;
    let rc = &placement.rcNormalPosition;
    let w = rc.right - rc.left;
    let h = rc.bottom - rc.top;
    let work = &monitor_info.rcWork;
    placement.rcNormalPosition = RECT {
        left:   work.left + (work.right  - work.left - w) / 2,
        top:    work.top  + (work.bottom - work.top  - h) / 2,
        right:  work.left + (work.right  - work.left - w) / 2 + w,
        bottom: work.top  + (work.bottom - work.top  - h) / 2 + h,
    };
    set_window_placement(hwnd, &placement)
}

/// Resizes the *restored* client area of a maximized or minimized window to
/// `(width, height)` and re-centers the result around the old window center,
/// without changing the window's current show state.
pub fn resize_restored_client(hwnd: HWND, width: i32, height: i32) -> Result<()> {
    let mut placement = get_window_placement(hwnd)?;
    let frame = get_normal_frame(hwnd)?;
    let old_rc = &placement.rcNormalPosition;
    let old_center = get_center_of_rect(old_rc);

    // Desired window size = desired client size + frame insets.
    let new_w = width  + (frame.right - frame.left);
    let new_h = height + (frame.bottom - frame.top);

    placement.rcNormalPosition = RECT {
        left:   old_center.x - new_w / 2,
        top:    old_center.y - new_h / 2,
        right:  old_center.x - new_w / 2 + new_w,
        bottom: old_center.y - new_h / 2 + new_h,
    };
    set_window_placement(hwnd, &placement)
}
