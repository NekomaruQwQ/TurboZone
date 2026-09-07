use std::rc::Rc;
use std::collections::VecDeque;

use euclid::default::{Point2D, Rect, Size2D};
use turbozone_core::{ProgramInfo, Snapshot, WindowAction, WindowInfo, WindowState};

/// Supplies scripted observations so UI transitions use the real engine boundary.
#[derive(Default)]
pub struct TestBackend {
    pub snapshots: VecDeque<anyhow::Result<Snapshot<u64>>>,
}

impl turbozone_core::Backend for TestBackend {
    type Handle = u64;

    fn snapshot(&mut self) -> anyhow::Result<Snapshot<Self::Handle>> {
        self.snapshots.pop_front().unwrap_or_else(|| Ok(Snapshot::default()))
    }

    #[expect(clippy::panic_in_result_fn, reason = "a render-side effect must fail the test, not be logged as an operational error")]
    fn perform(&mut self, _: Self::Handle, _: WindowAction) -> anyhow::Result<()> {
        panic!("headless rendering must emit actions without performing native effects");
    }
}

/// Makes a complete framework-independent snapshot for headless UI tests.
pub fn window(path: &str, title: &str) -> WindowInfo<u64> {
    WindowInfo {
        handle: 1,
        title: title.into(),
        state: WindowState::Normal,
        monitor_rect: Rect::new(Point2D::zero(), Size2D::new(1920, 1080)),
        content_rect: Rect::new(Point2D::zero(), Size2D::new(640, 480)),
        process_id: 42,
        program: Rc::new(ProgramInfo {
            path: path.into(),
            name: path.rsplit('/').next().unwrap().into(),
            description: "App Description".into(),
        }),
    }
}
