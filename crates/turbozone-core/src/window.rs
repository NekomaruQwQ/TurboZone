use euclid::default::*;
use smol_str::SmolStr;

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
pub struct WindowInfo<H> {
    /// Native handle identifying the exact enumerated window.
    pub handle: H,
    /// Case-sensitive window title.
    pub title: SmolStr,
    /// Current normal, maximized, or minimized state.
    pub state: WindowState,
    /// Complete window details from this snapshot, or an error if any detail
    /// could not be obtained.
    pub detail: anyhow::Result<WindowDetail>,
}

/// Complete geometry and program identity for matching and native controls.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WindowDetail {
    /// Current monitor work area in physical screen coordinates, excluding taskbars.
    pub monitor_rect: Rect<i32>,
    /// Controllable client-area rectangle in physical screen coordinates.
    /// Minimized/maximized windows use their restored geometry.
    pub content_rect: Rect<i32>,

    /// Successfully queried owning process identifier.
    pub process_id: u32,
    /// Windows-supplied program path with backslashes replaced by forward slashes.
    pub program_path: SmolStr,
    /// Program filename used by configuration matching.
    pub program_name: SmolStr,
}

impl WindowDetail {
    /// Returns whether the controllable client area is centered in the work area.
    /// Integer centers allow the unavoidable half-pixel difference for odd sizes.
    pub fn is_centered(&self) -> bool {
        self.content_rect.center() == self.monitor_rect.center()
    }
}
