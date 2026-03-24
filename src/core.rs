use std::path::PathBuf;

use euclid::default::*;
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

pub const RESOLUTION_GROUPS: &[(&str, &[Size2D<i32>])] = &[
    ("16:10", RESOLUTIONS_16_10),
];

pub const RESOLUTIONS_16_10: &[Size2D<i32>] = &[
    Size2D::new(3840, 2400),
    Size2D::new(2880, 1800),
    Size2D::new(2560, 1600),
    Size2D::new(1920, 1200),
    Size2D::new(1680, 1050),
    Size2D::new(1440,  900),
    Size2D::new(1280,  800),
    Size2D::new( 960,  600),
    Size2D::new( 800,  500),
    Size2D::new( 640,  400),
    Size2D::new( 480,  300),
];

pub fn is_known_resolution(size: Size2D<u32>) -> bool {
    RESOLUTION_GROUPS
        .iter()
        .flat_map(|&(_, arr)| arr)
        .any(|&resolution|
            resolution.width == size.width as i32 &&
            resolution.height == size.height as i32)
}

/// Converts a Win32 [`RECT`] to a [`Box2D`] for euclid-based geometry.
const fn box2d_from_rect(rect: &RECT) -> Box2D<i32> {
    Box2D::new(
        Point2D::new(rect.left, rect.top),
        Point2D::new(rect.right, rect.bottom))
}

/// Converts a [`Box2D`] to a Win32 [`RECT`] for API calls.
const fn box2d_into_rect(box2d: &Box2D<i32>) -> RECT {
    RECT {
        left:   box2d.min.x,
        top:    box2d.min.y,
        right:  box2d.max.x,
        bottom: box2d.max.y,
    }
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

fn get_display_name_for_executable(path: &PathBuf) -> Option<String> {
    VersionInfo::from_file(path)
        .map(|info| info.file_description)
        .ok()
        .or_else(|| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
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
    let screen_center = box2d_from_rect(&monitor_info.rcWork).center();
    let window_center = box2d_from_rect(&window_rect).center();
    Some(window_center == screen_center)
}

/// Centers this window on its screen.
/// For maximized/minimized windows, adjusts the restored position.
pub fn center_window(hwnd: HWND) -> Result<()> {
    match get_window_state(hwnd) {
        WindowState::Normal =>
            center_to_screen(hwnd),
        WindowState::Maximized | WindowState::Minimized =>
            center_restored_to_screen(hwnd),
    }
}

/// Resizes this window's client area to `size`.
/// For maximized/minimized windows, adjusts the restored size.
pub fn resize_window(hwnd: HWND, size: Size2D<i32>) -> Result<()> {
    match get_window_state(hwnd) {
        WindowState::Normal =>
            resize_client(hwnd, size),
        WindowState::Maximized | WindowState::Minimized =>
            resize_restored_client(hwnd, size),
    }
}

pub fn center_to_screen(hwnd: HWND) -> Result<()> {
    let Some(monitor_info) = get_monitor_info_from_window(hwnd) else {
        return Err(Error::empty());
    };

    let mut window_rect = RECT::default();

    // SAFETY: `window_rect` is stack-local; its raw pointer is valid for the
    // duration of the call.
    unsafe { GetWindowRect(hwnd, &raw mut window_rect) }?;

    let screen_center =
        box2d_from_rect(&monitor_info.rcWork).center();
    let window_size =
        box2d_from_rect(&window_rect).size();
    let window_position =
        screen_center - window_size.to_vector() / 2;

    // SAFETY: Positional arguments are computed from prior successful API
    // calls (`get_monitor_info_from_window`, `GetWindowRect`); `SWP_NOSIZE`
    // makes the width/height arguments (0, 0) ignored; flag constants are valid.
    unsafe {
        SetWindowPos(
            hwnd,
            None,
            window_position.x,
            window_position.y,
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
    let monitor_info =
        get_monitor_info_from_window(hwnd)?;
    let placement =
        get_window_placement(hwnd).ok()?;
    let screen_center =
        box2d_from_rect(&monitor_info.rcWork).center();
    let window_center =
        box2d_from_rect(&placement.rcNormalPosition).center();
    Some(window_center == screen_center)
}

/// Centers the *restored* position (`rcNormalPosition`) of a maximized or
/// minimized window on the monitor work area via `SetWindowPlacement`,
/// without changing the window's current show state.
pub fn center_restored_to_screen(hwnd: HWND) -> Result<()> {
    let Some(monitor_info) = get_monitor_info_from_window(hwnd) else {
        return Err(Error::empty());
    };

    let mut placement =
        get_window_placement(hwnd)?;
    let window_size =
        box2d_from_rect(&placement.rcNormalPosition).size();
    let screen_center =
        box2d_from_rect(&monitor_info.rcWork).center();

    // Derive max from min + size to preserve exact dimensions for odd sizes.
    let new_min =
        screen_center - window_size.to_vector() / 2;
    let new_max =
        new_min + window_size.to_vector();
    placement.rcNormalPosition =
        box2d_into_rect(&Box2D::new(new_min, new_max));
    set_window_placement(hwnd, &placement)
}

/// Resizes the *restored* client area of a maximized or minimized window to
/// `size` and re-centers the result around the old window center,
/// without changing the window's current show state.
pub fn resize_restored_client(hwnd: HWND, size: Size2D<i32>) -> Result<()> {
    let mut placement =
        get_window_placement(hwnd)?;
    let frame =
        get_normal_frame(hwnd)?;
    let old_center =
        box2d_from_rect(&placement.rcNormalPosition).center();

    // Desired window size = desired client size + frame overhead.
    // Derive max from min + size to preserve exact dimensions for odd sizes.
    let new_window_size = size + frame;
    let new_min = old_center - new_window_size.to_vector() / 2;
    let new_max = new_min + new_window_size.to_vector();
    placement.rcNormalPosition =
        box2d_into_rect(&Box2D::new(new_min, new_max));
    set_window_placement(hwnd, &placement)
}
