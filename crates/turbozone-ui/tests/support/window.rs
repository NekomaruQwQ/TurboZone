use std::rc::Rc;
use std::collections::VecDeque;

use euclid::default::{Point2D, Rect, Size2D};
use turbozone_core::{ProgramInfo, WindowAction, WindowInfo, WindowState};

/// Supplies scripted observations so UI transitions use the real engine boundary.
/// The final observation repeats for action-triggered refreshes of a stable desktop.
#[derive(Default)]
pub struct TestBackend {
    #[expect(clippy::type_complexity, reason = "scripted observations mirror the backend's direct result contract")]
    pub snapshots: VecDeque<anyhow::Result<Vec<(u64, anyhow::Result<WindowInfo<u64>>)>>>,
}

impl turbozone_core::Backend for TestBackend {
    type Handle = u64;

    fn snapshot(&mut self) -> anyhow::Result<Vec<(Self::Handle, anyhow::Result<WindowInfo<Self::Handle>>)>> {
        if self.snapshots.len() > 1 {
            return self.snapshots.pop_front().unwrap();
        }
        self.snapshots.front().map_or_else(|| Ok(Vec::new()), |snapshot| {
            snapshot.as_ref().map(|windows| windows.iter().map(|&(handle, ref result)| {
                (handle, result.as_ref().map(|window| WindowInfo {
                    handle: window.handle,
                    title: window.title.clone(),
                    state: window.state,
                    monitor_rect: window.monitor_rect,
                    content_rect: window.content_rect,
                    program: Rc::clone(&window.program),
                }).map_err(|error| anyhow::anyhow!("{error:#}")))
            }).collect()).map_err(|error| anyhow::anyhow!("{error:#}"))
        })
    }

    fn perform(&mut self, _: Self::Handle, _: WindowAction) -> anyhow::Result<()> {
        Ok(())
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
        program: Rc::new(ProgramInfo {
            path: path.into(),
            name: path.rsplit('/').next().unwrap().into(),
            description: "App Description".into(),
        }),
    }
}
