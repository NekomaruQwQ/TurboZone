use std::rc::Rc;

use euclid::default::{Point2D, Rect, Size2D};
use turbozone_core::{
    ProgramInfo, WindowInfo, WindowState,
};

/// Makes complete snapshots whose handles remain opaque to core grouping.
fn window(handle: u64, path: &str, title: &str) -> WindowInfo<u64> {
    WindowInfo {
        handle,
        title: title.into(),
        state: WindowState::Normal,
        monitor_rect: Rect::new(Point2D::zero(), Size2D::new(1920, 1080)),
        content_rect: Rect::new(Point2D::zero(), Size2D::new(640, 480)),
        program: Rc::new(ProgramInfo {
            path: path.into(),
            name: path.rsplit('/').next().unwrap().into(),
            description: "Tool".into(),
        }),
    }
}

#[test]
fn centered_query_uses_integer_centers_without_requiring_equal_rectangles() {
    let mut candidate = window(1, "C:/Apps/Tool.exe", "Tool");
    candidate.monitor_rect = Rect::new(Point2D::new(-100, 50), Size2D::new(1001, 801));
    candidate.content_rect = Rect::new(Point2D::new(150, 250), Size2D::new(501, 401));
    assert!(candidate.is_centered());
    candidate.content_rect.origin.x += 1;
    assert!(!candidate.is_centered());
}

