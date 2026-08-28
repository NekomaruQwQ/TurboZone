use euclid::default::{Point2D, Rect, Size2D};
use turbozone_core::{WindowDetail, WindowInfo, WindowState};
use turbozone_windows::WindowHandle;

/// Makes a complete snapshot with a null handle that tests never pass to native actions.
pub fn window(path: &str, title: &str) -> WindowInfo<WindowHandle> {
    WindowInfo {
        handle: WindowHandle::default(),
        title: title.to_owned(),
        state: WindowState::Normal,
        detail: Ok(WindowDetail {
            monitor_rect: Rect::new(Point2D::zero(), Size2D::new(1920, 1080)),
            content_rect: Rect::new(Point2D::zero(), Size2D::new(640, 480)),
            process_id: 42,
            program_name: path.rsplit('/').next().unwrap().to_owned(),
            program_path: path.to_owned(),
        }),
    }
}
