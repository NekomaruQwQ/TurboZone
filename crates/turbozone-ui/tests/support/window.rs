use std::rc::Rc;

use euclid::default::{Point2D, Rect, Size2D};
use turbozone_core::{ProgramDetail, WindowDetail, WindowInfo, WindowState};

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
            program: Rc::new(ProgramDetail {
                path: path.into(),
                name: path.rsplit('/').next().unwrap().into(),
                description: "App Description".into(),
            }),
        }),
    }
}
