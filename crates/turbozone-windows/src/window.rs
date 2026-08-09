//! Safe window snapshots and high-level centering and resizing operations.

use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::path::Path;

use euclid::default::{Box2D, Point2D};
use turbozone_core::{ExecutableMetadata, WindowCandidate, WindowSize};
use win32_version_info::VersionInfo;
use windows::core::{Error, Result};
use windows::Win32::Foundation::{HWND, RECT};

use crate::native;
use crate::normalize_native_path;

/// An opaque native window handle captured in a snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowHandle(HWND);

impl WindowHandle {
    /// Returns the raw address for diagnostic logging only.
    pub fn address(self) -> usize {
        self.0.0.addr()
    }
}

impl Hash for WindowHandle {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher {
        self.address().hash(state);
    }
}

/// The current visual state of a native window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WindowState {
    /// The live window uses its normal rectangle.
    Normal,
    /// The live window is maximized and controls operate on its restored rectangle.
    Maximized,
    /// The live window is minimized and controls operate on its restored rectangle.
    Minimized,
}

/// An immutable native window snapshot used by grouping and rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowInfo {
    /// Native handle identifying the exact enumerated window.
    pub handle: WindowHandle,
    /// Owning process identifier, or zero when Windows could not provide one.
    pub process_id: u32,
    /// Case-sensitive window title.
    pub window_title: String,
    /// Current normal, maximized, or minimized state.
    pub state: WindowState,
    /// Controllable client-area size in physical pixels.
    pub client_size: Option<WindowSize>,
    /// Whether the live or restored window rectangle is centered.
    pub is_centered: Option<bool>,
    /// Lexically normalized native executable path with forward slashes.
    pub executable_path: Option<String>,
    /// Executable filename used by configuration matching.
    pub executable_name: Option<String>,
    /// Friendly version-resource description used for display.
    pub executable_display_name: Option<String>,
}

impl WindowInfo {
    /// Converts this native snapshot into a platform-neutral grouping candidate.
    pub fn into_candidate(self) -> WindowCandidate<Self> {
        WindowCandidate {
            window_title: self.window_title.clone(),
            executable: ExecutableMetadata {
                name: self.executable_name.clone(),
                path: self.executable_path.clone(),
                display_name: self.executable_display_name.clone(),
            },
            payload: self,
        }
    }
}

/// Enumerates application windows while caching executable version metadata.
#[derive(Debug, Default)]
pub struct WindowSnapshotter {
    display_name_cache: BTreeMap<String, String>,
}

impl WindowSnapshotter {
    /// Captures relevant top-level application windows.
    ///
    /// A top-level enumeration failure is returned to the caller. Per-window
    /// metadata failures are represented by optional fields so other windows
    /// remain available.
    pub fn snapshot(&mut self) -> Result<Vec<WindowInfo>> {
        Ok(native::enumerate_windows()?
            .into_iter()
            .filter(|&handle| native::is_app_window(handle))
            .map(|handle| self.snapshot_window(handle))
            .filter(|window| !window.window_title.is_empty())
            .filter(|window| !(
                window.window_title == "Program Manager"
                && window.executable_name.as_deref() == Some("explorer.exe")))
            .collect())
    }

    fn snapshot_window(&mut self, handle: HWND) -> WindowInfo {
        let state = window_state(handle);
        let (client_size, is_centered) = match state {
            WindowState::Normal => (
                native::get_client_size(handle).ok(),
                is_centered_raw(handle)),
            WindowState::Maximized | WindowState::Minimized => (
                native::get_restored_client_size(handle).ok(),
                is_restored_centered_raw(handle)),
        };
        let process_id = native::get_process_id(handle);
        let native_path = native::get_executable_path(process_id);
        let executable_path = native_path.as_deref().map(normalize_native_path);
        let executable_name = native_path.as_deref().and_then(|path| {
            path.file_name().map(|name| name.to_string_lossy().into_owned())
        });
        let executable_display_name = native_path.as_deref().zip(executable_path.as_deref())
            .map(|(path, cache_key)| {
                self.display_name_cache.entry(cache_key.to_owned())
                    .or_insert_with(|| executable_display_name(path))
                    .clone()
            });

        WindowInfo {
            handle: WindowHandle(handle),
            process_id,
            window_title: native::get_window_text(handle),
            state,
            client_size,
            is_centered,
            executable_path,
            executable_name,
            executable_display_name,
        }
    }
}

/// Returns whether the live or restored window rectangle is centered.
pub fn is_centered(handle: WindowHandle) -> Option<bool> {
    match window_state(handle.0) {
        WindowState::Normal => is_centered_raw(handle.0),
        WindowState::Maximized | WindowState::Minimized => {
            is_restored_centered_raw(handle.0)
        },
    }
}

/// Centers a window without changing its current visual state.
pub fn center_window(handle: WindowHandle) -> Result<()> {
    match window_state(handle.0) {
        WindowState::Normal => center_normal_window(handle.0),
        WindowState::Maximized | WindowState::Minimized => {
            center_restored_window(handle.0)
        },
    }
}

/// Resizes a window client area without changing its current visual state.
pub fn resize_window(handle: WindowHandle, size: WindowSize) -> Result<()> {
    match window_state(handle.0) {
        WindowState::Normal => native::resize_client(handle.0, size),
        WindowState::Maximized | WindowState::Minimized => {
            resize_restored_window(handle.0, size)
        },
    }
}

fn executable_display_name(path: &Path) -> String {
    VersionInfo::from_file(path)
        .map(|info| info.file_description)
        .ok()
        .filter(|name| !name.is_empty())
        .or_else(|| path.file_name().map(|name| name.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "Unknown executable".to_owned())
}

fn window_state(handle: HWND) -> WindowState {
    if native::is_minimized(handle) {
        WindowState::Minimized
    } else if native::is_maximized(handle) {
        WindowState::Maximized
    } else {
        WindowState::Normal
    }
}

const fn box2d_from_rect(rect: &RECT) -> Box2D<i32> {
    Box2D::new(
        Point2D::new(rect.left, rect.top),
        Point2D::new(rect.right, rect.bottom))
}

const fn box2d_into_rect(box2d: &Box2D<i32>) -> RECT {
    RECT {
        left: box2d.min.x,
        top: box2d.min.y,
        right: box2d.max.x,
        bottom: box2d.max.y,
    }
}

fn is_centered_raw(handle: HWND) -> Option<bool> {
    let monitor_info = native::get_monitor_info_from_window(handle)?;
    let window_rect = native::get_window_rect(handle).ok()?;
    Some(box2d_from_rect(&monitor_info.rcWork).center() == box2d_from_rect(&window_rect).center())
}

fn is_restored_centered_raw(handle: HWND) -> Option<bool> {
    let monitor_info = native::get_monitor_info_from_window(handle)?;
    let placement = native::get_window_placement(handle).ok()?;
    Some(
        box2d_from_rect(&monitor_info.rcWork).center()
            == box2d_from_rect(&placement.rcNormalPosition).center())
}

fn center_normal_window(handle: HWND) -> Result<()> {
    let monitor_info = native::get_monitor_info_from_window(handle).ok_or(Error::empty())?;
    let screen_center = box2d_from_rect(&monitor_info.rcWork).center();
    let window_size = box2d_from_rect(&native::get_window_rect(handle)?).size();
    native::set_window_position(handle, screen_center - window_size.to_vector() / 2)
}

fn center_restored_window(handle: HWND) -> Result<()> {
    let monitor_info = native::get_monitor_info_from_window(handle).ok_or(Error::empty())?;
    let mut placement = native::get_window_placement(handle)?;
    let window_size = box2d_from_rect(&placement.rcNormalPosition).size();
    let screen_center = box2d_from_rect(&monitor_info.rcWork).center();

    // Deriving max from min preserves exact dimensions for odd window sizes.
    let new_min = screen_center - window_size.to_vector() / 2;
    let new_max = new_min + window_size.to_vector();
    placement.rcNormalPosition = box2d_into_rect(&Box2D::new(new_min, new_max));
    native::set_window_placement(handle, &placement)
}

fn resize_restored_window(handle: HWND, size: WindowSize) -> Result<()> {
    let mut placement = native::get_window_placement(handle)?;
    let frame = native::get_normal_frame(handle)?;
    let old_center = box2d_from_rect(&placement.rcNormalPosition).center();
    let new_window_size = size + frame;

    // Deriving max from min preserves exact dimensions for odd window sizes.
    let new_min = old_center - new_window_size.to_vector() / 2;
    let new_max = new_min + new_window_size.to_vector();
    placement.rcNormalPosition = box2d_into_rect(&Box2D::new(new_min, new_max));
    native::set_window_placement(handle, &placement)
}
