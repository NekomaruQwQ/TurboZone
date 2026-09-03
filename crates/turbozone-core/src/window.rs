use std::rc::Rc;
use euclid::default::Rect;
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
    /// The native handle identifying the exact enumerated window.
    pub handle: H,
    /// The title of the window, or an empty string if the window has no title or
    /// the title could not be obtained.
    pub title: SmolStr,
    /// The current normal, maximized, or minimized state.
    pub state: WindowState,
    /// Complete window details from this snapshot, or an error if any detail
    /// could not be obtained.
    pub detail: anyhow::Result<WindowDetail>,
}

/// Extra information about a native window that may fail to be obtained.
#[derive(Debug, Clone)]
pub struct WindowDetail {
    /// Current monitor work area in physical screen coordinates, excluding
    /// taskbars.
    pub monitor_rect: Rect<i32>,
    /// Controllable client-area rectangle in physical screen coordinates.
    /// Minimized/maximized windows use their restored geometry.
    pub content_rect: Rect<i32>,
    /// Process ID owning the window.
    pub process_id: u32,
    /// Program details of the executable owning the window.
    ///
    /// [`Rc`] is here used to allow sharing between multiple windows of
    /// the same program. Exact sharing behavior depends on the platform
    /// [`Backend`](crate::Backend).
    ///
    /// Encountering invalid utf-8 strings in any of the fields is considered
    /// an error and will be reported as a failure to obtain the window detail.
    pub program: Rc<ProgramDetail>,
}

/// Information about a program executable owning one or more windows.
#[derive(Debug, Clone)]
pub struct ProgramDetail {
    /// Platform-provided path to the program executable, normalized to use
    /// forward slashes.
    pub path: SmolStr,
    /// Platform-provided filename of the program executable.
    pub name: SmolStr,
    /// Platform-provided description of the program executable, or an empty
    /// string if not available.
    pub description: SmolStr,
}
