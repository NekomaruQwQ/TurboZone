use std::collections::VecDeque;
use std::rc::Rc;

use euclid::default::{Point2D, Rect, Size2D};
use turbozone_core::{
    ProgramInfo, WindowAction, Backend, Engine, WindowDetail, WindowInfo, WindowState,
    parse_config,
};

/// Records the action boundary independently of action variants and supplies scripted snapshots.
#[derive(Default)]
struct FakeBackend {
    snapshots: VecDeque<anyhow::Result<Vec<WindowInfo<u64>>>>,
    attempted: Vec<WindowAction<u64>>,
    failing_handle: Option<u64>,
}

impl Backend for FakeBackend {
    type Handle = u64;

    fn snapshot(&mut self) -> anyhow::Result<Vec<WindowInfo<Self::Handle>>> {
        self.snapshots.pop_front().unwrap_or_else(|| Ok(Vec::new()))
    }

    fn perform(&mut self, action: WindowAction<Self::Handle>) -> anyhow::Result<()> {
        let handle = action.handle();
        self.attempted.push(action);
        if self.failing_handle == Some(handle) {
            anyhow::bail!("scripted action failure");
        }
        Ok(())
    }
}

fn window(handle: u64) -> WindowInfo<u64> {
    WindowInfo {
        handle,
        title: "Application".into(),
        state: WindowState::Normal,
        detail: Ok(WindowDetail {
            monitor_rect: Rect::new(Point2D::zero(), Size2D::new(1920, 1080)),
            content_rect: Rect::new(Point2D::zero(), Size2D::new(640, 480)),
            process_id: 42,
            program: Rc::new(ProgramInfo {
                path: "C:/Apps/app.exe".into(),
                name: "app.exe".into(),
                description: "Application".into(),
            }),
        }),
    }
}

#[test]
fn tick_forwards_actions_in_queue_order_before_refreshing() {
    let rules = parse_config("[[rules]]\nname = 'all'").unwrap().rules;
    let mut backend = FakeBackend::default();
    backend.snapshots.push_back(Ok(vec![window(1)]));
    let mut engine = Engine::new(rules, backend);
    engine.queue([
        WindowAction::MoveToCenter(1),
        WindowAction::Resize(1, Size2D::new(1280, 720)),
    ]);

    engine.tick();

    assert_eq!(engine.into_backend().attempted, [
        WindowAction::MoveToCenter(1),
        WindowAction::Resize(1, Size2D::new(1280, 720)),
    ]);
}

#[test]
fn failed_action_does_not_prevent_later_targets_or_refresh() {
    let rules = parse_config("[[rules]]\nname = 'all'").unwrap().rules;
    let mut backend = FakeBackend { failing_handle: Some(1), ..Default::default() };
    backend.snapshots.push_back(Ok(vec![window(2)]));
    let mut engine = Engine::new(rules, backend);
    engine.queue([
        WindowAction::MoveToCenter(1),
        WindowAction::MoveToCenter(2),
    ]);

    engine.tick();

    assert_eq!(engine.groups()[0].windows[0].handle, 2);
    assert_eq!(engine.into_backend().attempted.len(), 2);
}

#[test]
fn failed_snapshot_clears_previously_visible_sections() {
    let rules = parse_config("[[rules]]\nname = 'all'").unwrap().rules;
    let mut backend = FakeBackend::default();
    backend.snapshots.push_back(Ok(vec![window(1)]));
    backend.snapshots.push_back(Err(anyhow::anyhow!("enumeration unavailable")));
    let mut engine = Engine::new(rules, backend);
    engine.tick();
    assert_eq!(engine.groups().len(), 1);

    engine.tick();

    assert!(engine.groups().is_empty());
}
