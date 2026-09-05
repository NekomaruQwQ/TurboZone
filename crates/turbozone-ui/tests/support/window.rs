use std::rc::Rc;

use euclid::default::{Point2D, Rect, Size2D};
use turbozone_core::{ProgramInfo, WindowAction, WindowDetail, WindowInfo, WindowState};

/// Supplies a one-shot owned snapshot so headless rendering uses the real engine boundary.
#[derive(Default)]
pub struct TestBackend {
    pub windows: Vec<WindowInfo<u64>>,
}

impl turbozone_core::Backend for TestBackend {
    type Handle = u64;

    fn snapshot(&mut self) -> anyhow::Result<Vec<WindowInfo<Self::Handle>>> {
        Ok(std::mem::take(&mut self.windows))
    }

    fn perform(&mut self, _: WindowAction<Self::Handle>) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Makes a complete framework-independent snapshot for headless UI tests.
pub fn window(path: &str, title: &str) -> WindowInfo<u64> {
    WindowInfo {
        handle: 1,
        title: title.into(),
        state: WindowState::Normal,
        detail: Ok(WindowDetail {
            monitor_rect: Rect::new(Point2D::zero(), Size2D::new(1920, 1080)),
            content_rect: Rect::new(Point2D::zero(), Size2D::new(640, 480)),
            process_id: 42,
            program: Rc::new(ProgramInfo {
                path: path.into(),
                name: path.rsplit('/').next().unwrap().into(),
                description: "App Description".into(),
            }),
        }),
    }
}
