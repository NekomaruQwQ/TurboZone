use super::EngineTestExt as _;

use crate::config_parser::parse_config;
use std::collections::VecDeque;
use std::rc::Rc;

use euclid::default::{Point2D, Rect, Size2D};
use crate::{
    ProgramInfo, WindowAction, Backend, Engine, Snapshot, WindowInfo, WindowState,
};

/// Records the action boundary independently of action variants and supplies scripted snapshots.
/// Snapshot checkpoints verify that each complete batch executes before refresh and is drained.
#[derive(Default)]
struct FakeBackend {
    snapshots: VecDeque<anyhow::Result<Snapshot<u64>>>,
    attempted: Vec<(u64, WindowAction)>,
    attempts_at_snapshot: Vec<usize>,
    failing_handle: Option<u64>,
}

impl Backend for FakeBackend {
    type Handle = u64;

    fn snapshot(&mut self) -> anyhow::Result<Snapshot<Self::Handle>> {
        self.attempts_at_snapshot.push(self.attempted.len());
        self.snapshots.pop_front().unwrap_or_else(|| Ok(Snapshot::default()))
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
        monitor_rect: Rect::new(Point2D::zero(), Size2D::new(1920, 1080)),
        content_rect: Rect::new(Point2D::zero(), Size2D::new(640, 480)),
        process_id: 42,
        program: Rc::new(ProgramInfo {
            path: "C:/Apps/app.exe".into(),
            name: "app.exe".into(),
            description: "Application".into(),
        }),
    }
}

#[test]
fn update_forwards_actions_in_queue_order_before_refreshing() {
    let rules = parse_config("[[rules]]\nname = 'all'").unwrap().rules;
    let mut backend = FakeBackend::default();
    backend.snapshots.push_back(Ok(Snapshot { windows: vec![window(1)], ..Default::default() }));
    let mut engine = Engine::new(rules, backend);
    engine.queue([
        (1, WindowAction::Center),
        (2, WindowAction::Resize(Size2D::new(1280, 720))),
    ]);
    assert!(engine.has_pending_actions());

    engine.update();

    assert!(!engine.has_pending_actions());
    // A second update must refresh without replaying the drained batch.
    engine.update();
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
    backend.snapshots.push_back(Ok(Snapshot { windows: vec![window(2)], ..Default::default() }));
    let mut engine = Engine::new(rules, backend);
    engine.queue([
        (1, WindowAction::Center),
        (2, WindowAction::Center),
    ]);

    engine.update();

    assert_eq!(engine.view().unwrap().groups[0].windows[0].handle, 2);
    assert_eq!(engine.into_backend().attempted, [
        (1, WindowAction::Center),
        (2, WindowAction::Center),
    ]);
}

#[test]
fn failed_snapshot_clears_previously_visible_sections() {
    let rules = parse_config("[[rules]]\nname = 'all'").unwrap().rules;
    let mut backend = FakeBackend::default();
    backend.snapshots.push_back(Ok(Snapshot { windows: vec![window(1)], ..Default::default() }));
    backend.snapshots.push_back(Err(anyhow::anyhow!("enumeration unavailable")));
    let mut engine = Engine::new(rules, backend);
    engine.update();
    assert_eq!(engine.view().unwrap().groups.len(), 1);

    engine.update();

    assert!(engine.snapshot().is_err());
    assert_eq!(engine.view().unwrap_err().to_string(), "enumeration unavailable");
}

/// Engine ownership preserves source values and stable-name lookup through updates.
#[test]
fn engine_exposes_verified_authored_rules_without_normalization() {
    let rules = parse_config(
        "[[rules]]\nname = 'tool'\ndescription = '  Tool  '\nprogram.name = 'APP.EXE'\n\
         [[rules]]\nname = 'fallback'").unwrap().rules;
    let mut backend = FakeBackend::default();
    backend.snapshots.push_back(Ok(Snapshot { windows: vec![window(1)], ..Default::default() }));
    let mut engine = Engine::new(rules, backend);
    engine.update();

    assert_eq!(engine.rules().iter().map(|rule| rule.name.as_str())
        .collect::<Vec<_>>(), ["tool", "fallback"]);
    assert_eq!(engine.view().unwrap().groups[0].rule_name, "tool");
    let rule = engine.rule("tool").unwrap();
    assert_eq!(rule.description, "  Tool  ");
    assert_eq!(rule.display_name(), "Tool");
    assert_eq!(rule.program.name, Some(crate::Pattern::Exact("APP.EXE".into())));
    assert!(engine.rule("unknown").is_none());
}

/// A retained native cause proves the engine does not rebuild errors from log strings.
#[test]
fn raw_snapshot_preserves_unmatched_windows_and_original_errors_without_view_queries() {
    let rules = parse_config("[[rules]]\nname = 'selected'\nwindow.title = 'Selected'").unwrap().rules;
    let mut matched = window(1);
    matched.title = "Selected".into();
    let error = anyhow::Error::new(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
        .context("program path");
    let backend = FakeBackend {
        snapshots: [Ok(Snapshot {
            windows: vec![matched, window(2)],
            errors: [(3, error)].into(),
        })].into(),
        ..Default::default()
    };
    let mut engine = Engine::new(rules, backend);
    engine.update();
    let raw = engine.snapshot().as_ref().unwrap();
    assert_eq!(raw.windows.len(), 2);
    assert_eq!(raw.errors[&3].downcast_ref::<std::io::Error>().unwrap().kind(),
        std::io::ErrorKind::PermissionDenied);
    for _ in 0..2 {
        let view = engine.view().unwrap();
        assert_eq!(view.groups[0].windows.len(), 1);
        assert!(std::ptr::eq(view.groups[0].windows[0], &raw const raw.windows[0]));
        assert!(std::ptr::eq(view.errors, &raw const raw.errors));
    }
    assert_eq!(engine.into_backend().attempts_at_snapshot, [0]);
}

#[test]
fn failures_never_match_unfiltered_or_size_filtered_rules_and_recovery_uses_new_data() {
    for source in ["", "[[rules]]\nname = 'all'", "[[rules]]\nname = 'large'\nwindow.min = [640, 480]"] {
        let mut recovered = window(1);
        recovered.program = Rc::new(ProgramInfo {
            path: "C:/Other/recovered.exe".into(),
            name: "recovered.exe".into(),
            description: "Recovered".into(),
        });
        recovered.content_rect.size = Size2D::new(800, 600);
        let backend = FakeBackend {
            snapshots: [
                Ok(Snapshot { errors: [(1, anyhow::anyhow!("client size unavailable"))].into(), ..Default::default() }),
                Ok(Snapshot { windows: vec![recovered], ..Default::default() }),
                Ok(Snapshot { errors: [(1, anyhow::anyhow!("failed again"))].into(), ..Default::default() }),
                Err(anyhow::anyhow!("enumeration unavailable")),
                Ok(Snapshot::default()),
            ].into(),
            ..Default::default()
        };
        let mut engine = Engine::new(parse_config(source).unwrap().rules, backend);
        engine.update();
        assert!(engine.view().unwrap().groups.is_empty());
        assert_eq!(engine.view().unwrap().errors.len(), 1);
        engine.update();
        let view = engine.view().unwrap();
        assert!(view.errors.is_empty());
        if !source.is_empty() {
            assert_eq!(view.groups[0].program.path, "C:/Other/recovered.exe");
            assert_eq!(view.groups[0].windows[0].content_rect.size, Size2D::new(800, 600));
        }
        engine.update();
        assert!(engine.view().unwrap().groups.is_empty());
        assert_eq!(engine.view().unwrap().errors.len(), 1);
        engine.update();
        assert!(engine.snapshot().is_err());
        assert!(std::ptr::eq(engine.view().unwrap_err(), engine.snapshot().as_ref().unwrap_err()));
        engine.update();
        assert!(engine.view().unwrap().errors.is_empty());
    }
}

#[test]
fn construction_is_inert_and_every_completed_attempt_updates_its_timestamp() {
    let before = std::time::Instant::now();
    let backend = FakeBackend {
        snapshots: [Err(anyhow::anyhow!("unavailable")), Ok(Snapshot::default())].into(),
        ..Default::default()
    };
    let mut engine = Engine::new(Vec::new(), backend);
    assert!(engine.snapshot_timestamp() >= before);
    assert!(engine.view().unwrap().groups.is_empty());
    for _ in 0..2 {
        let start = std::time::Instant::now();
        engine.update();
        let finish = std::time::Instant::now();
        assert!((start..=finish).contains(&engine.snapshot_timestamp()));
    }
    assert_eq!(engine.into_backend().attempts_at_snapshot, [0, 0]);
}
