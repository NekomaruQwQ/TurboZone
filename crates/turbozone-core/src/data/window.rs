use std::path::Path;

use euclid::default::*;

/// The current visual state of a native window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[derive(Default)]
pub enum WindowState {
    /// The live window uses its normal rectangle.
    #[default]
    Normal,
    /// The live window is maximized and controls operate on its restored rectangle.
    Maximized,
    /// The live window is minimized and controls operate on its restored rectangle.
    Minimized,
}

/// An immutable native window snapshot used by classification and rendering.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WindowInfo<H> {
    /// Native handle identifying the exact enumerated window.
    pub handle: H,
    /// Case-sensitive window title.
    pub title: String,
    /// Current normal, maximized, or minimized state.
    pub state: WindowState,
    /// Complete details, or a nonempty list of failures from this snapshot.
    /// Failed details must not participate in matching or expose native actions.
    pub detail: Result<WindowDetail, Vec<String>>,
}

/// Complete geometry and executable identity for matching and native controls.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WindowDetail {
    /// Current monitor work area in physical screen coordinates, excluding taskbars.
    pub monitor_rect: Rect<i32>,
    /// Controllable client-area rectangle in physical screen coordinates.
    /// Minimized/maximized windows use their restored geometry.
    pub content_rect: Rect<i32>,

    /// Successfully queried owning process identifier.
    pub process_id: u32,
    /// Lexically normalized native executable path with forward slashes.
    pub executable_path: String,
    /// Executable filename used by configuration matching.
    pub executable_name: String,
}

impl WindowDetail {
    /// Returns whether the controllable client area is centered in the work area.
    /// Integer centers allow the unavoidable half-pixel difference for odd sizes.
    pub fn is_centered(&self) -> bool {
        self.content_rect.center() == self.monitor_rect.center()
    }
}

/// Normalizes a native path lexically without accessing the filesystem.
///
/// If lexical normalization fails, keeps the original path. Non-Unicode names
/// are converted lossily, and native backslashes become forward slashes.
pub fn normalize_native_path(path: &Path) -> String {
    path.normalize_lexically()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(windows)]
    fn native_path_normalization_resolves_components_and_separators() {
        assert_eq!(
            normalize_native_path(Path::new(r"C:\Apps\.\Edge\..\Browser\app.exe")),
            "C:/Apps/Browser/app.exe");
    }

    #[test]
    fn centered_content_supports_negative_monitor_origins_and_odd_sizes() {
        let detail = WindowDetail {
            monitor_rect: Rect::new(Point2D::new(-1920, 40), Size2D::new(1920, 1040)),
            content_rect: Rect::new(Point2D::new(-1280, 320), Size2D::new(641, 481)),
            process_id: 1,
            executable_path: "C:/app.exe".to_owned(),
            executable_name: "app.exe".to_owned(),
        };
        assert!(detail.is_centered());
    }
}
