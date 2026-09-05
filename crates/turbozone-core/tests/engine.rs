use std::collections::VecDeque;
use std::rc::Rc;

use euclid::default::{Point2D, Rect, Size2D};
use turbozone_core::{
    ProgramInfo, WindowAction, Backend, Engine, WindowDetail, WindowInfo, WindowState,
    parse_config,
};

/// Records the action boundary independently of action variants and supplies scripted snapshots.
/// Snapshot checkpoints verify that each complete batch executes before refresh and is drained.
#[derive(Default)]
struct FakeBackend {
    snapshots: VecDeque<anyhow::Result<Vec<WindowInfo<u64>>>>,
    attempted: Vec<(u64, WindowAction)>,
    attempts_at_snapshot: Vec<usize>,
    failing_handle: Option<u64>,
}

impl Backend for FakeBackend {
    type Handle = u64;

    fn snapshot(&mut self) -> anyhow::Result<Vec<WindowInfo<Self::Handle>>> {
        self.attempts_at_snapshot.push(self.attempted.len());
        self.snapshots.pop_front().unwrap_or_else(|| Ok(Vec::new()))
    }

    fn perform(&mut self, target: Self::Handle, action: WindowAction) -> anyhow::Result<()> {
        self.attempted.push((target, action));
        if self.failing_handle == Some(target) {
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
        (1, WindowAction::Center),
        (2, WindowAction::Resize(Size2D::new(1280, 720))),
    ]);
    assert!(engine.has_pending_actions());

    engine.tick();

    assert!(!engine.has_pending_actions());
    // A second tick must refresh without replaying the drained batch.
    engine.tick();
    let backend = engine.into_backend();
    assert_eq!(backend.attempted, [
        (1, WindowAction::Center),
        (2, WindowAction::Resize(Size2D::new(1280, 720))),
    ]);
    assert_eq!(backend.attempts_at_snapshot, [2, 2]);
}

#[test]
fn failed_action_does_not_prevent_later_targets_or_refresh() {
    let rules = parse_config("[[rules]]\nname = 'all'").unwrap().rules;
    let mut backend = FakeBackend { failing_handle: Some(1), ..Default::default() };
    backend.snapshots.push_back(Ok(vec![window(2)]));
    let mut engine = Engine::new(rules, backend);
    engine.queue([
        (1, WindowAction::Center),
        (2, WindowAction::Center),
    ]);

    engine.tick();

    assert_eq!(engine.groups()[0].windows[0].handle, 2);
    assert_eq!(engine.into_backend().attempted, [
        (1, WindowAction::Center),
        (2, WindowAction::Center),
    ]);
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

/// Engine ownership preserves source values and stable-name lookup through ticks.
#[test]
fn engine_exposes_verified_authored_rules_without_normalization() {
    let rules = parse_config(
        "[[rules]]\nname = 'tool'\ndescription = '  Tool  '\nprogram.name = 'APP.EXE'\n\
         [[rules]]\nname = 'fallback'").unwrap().rules;
    let mut backend = FakeBackend::default();
    backend.snapshots.push_back(Ok(vec![window(1)]));
    let mut engine = Engine::new(rules, backend);
    engine.tick();

    assert_eq!(engine.rules().iter().map(|rule| rule.name.as_str())
        .collect::<Vec<_>>(), ["tool", "fallback"]);
    assert_eq!(engine.groups()[0].rule_name, "tool");
    let rule = engine.rule("tool").unwrap();
    assert_eq!(rule.description, "  Tool  ");
    assert_eq!(rule.display_name(), "Tool");
    assert_eq!(rule.program.name, Some(turbozone_core::Pattern::Exact("APP.EXE".into())));
    assert!(engine.rule("unknown").is_none());
}
