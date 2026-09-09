use std::rc::Rc;
use euclid::default::Rect;
use euclid::default::Size2D;
use smol_str::SmolStr;

/// One native side effect accepted from a rendered snapshot.
///
/// Actions describe an operation independently of its target. The engine queues each
/// operation with one handle, preserving ordering and per-target failures. New variants
/// may be added without presentation crates assuming the complete backend operation set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum WindowAction {
    /// Centers the live or restored client area in its current monitor work area.
    Center,
    /// Sets the client area to an exact positive physical-pixel size.
    Resize(Size2D<i32>),
}

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

/// A complete immutable native observation used by classification and rendering.
/// Failed required queries appear beside their handle in [`crate::Backend::snapshot`].
#[derive(Debug)]
pub struct WindowInfo<H> {
    /// The native handle identifying the exact enumerated window.
    pub handle: H,

    /// The title of the window, or an empty string if the window has no title or
    /// the title could not be obtained.
    pub title: SmolStr,

    /// The current normal, maximized, or minimized state.
    pub state: WindowState,

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
    /// an error and will be reported as a failure to capture this window.
    pub program: Rc<ProgramInfo>,
}

impl<H> WindowInfo<H> {
    /// Compares integer client/work-area centers, matching native centering actions.
    pub fn is_centered(&self) -> bool {
        self.content_rect.center() == self.monitor_rect.center()
    }
}

/// Information about a program executable owning one or more windows.
#[derive(Debug, Clone)]
pub struct ProgramInfo {
    /// Platform-provided path to the program executable, normalized to use
    /// forward slashes.
    pub path: SmolStr,
    /// Platform-provided filename of the program executable.
    pub name: SmolStr,
    /// Platform-provided description of the program executable, or an empty
    /// string if not available.
    pub description: SmolStr,
}
