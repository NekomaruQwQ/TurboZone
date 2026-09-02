//! Adaptation of core's backend contract to the Win32 implementation.

use anyhow::Context as _;
use turbozone_core::{Action, Backend, WindowInfo};

use crate::{WindowEnumerator, WindowHandle, center_window, resize_window};

/// Owns per-snapshot Win32 caches and executes core actions against live windows.
#[derive(Debug, Default)]
pub struct WindowsBackend {
    enumerator: WindowEnumerator,
}

impl Backend for WindowsBackend {
    type Handle = WindowHandle;

    fn snapshot(&mut self) -> anyhow::Result<Vec<WindowInfo<Self::Handle>>> {
        self.enumerator.snapshot().map_err(anyhow::Error::from)
    }

    #[expect(
        clippy::panic_in_result_fn,
        reason = "an unknown action is a core/backend contract mismatch, not an operational failure")]
    fn perform(&mut self, action: Action<Self::Handle>) -> anyhow::Result<()> {
        match action {
            Action::Resize(handle, size) => resize_window(handle, size)
                .with_context(|| format!(
                    "failed to resize client area to {}x{}",
                    size.width,
                    size.height)),
            Action::MoveToCenter(handle) => center_window(handle)
                .context("failed to center client area"),
            _ => panic!("Windows backend received an unsupported TurboZone action"),
        }
    }
}
