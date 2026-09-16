//! Exercises engine results and backend effects through the public contract.

use turbozone_core::config_parser::parse_config;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use euclid::default::{Point2D, Rect, Size2D};
use turbozone_core::{
    ProgramInfo, WindowAction, Backend, Engine, WindowInfo, WindowState,
};

/// Records the action boundary independently of action variants and supplies scripted snapshots.
/// Snapshot checkpoints verify that each complete batch executes before refresh and is drained.
#[derive(Default)]
struct FakeBackend {
    #[expect(clippy::type_complexity, reason = "scripted observations mirror the backend's direct result contract")]
    snapshots: VecDeque<anyhow::Result<Vec<(u64, anyhow::Result<WindowInfo<u64>>)>>>,
    calls: Rc<RefCell<BackendCalls>>,
    failing_handle: Option<u64>,
}

/// The test retains the recorder while Engine owns its backend; no engine fields are exposed.
#[derive(Default)]
struct BackendCalls {
    attempted: Vec<(u64, WindowAction)>,
    attempts_at_snapshot: Vec<usize>,
}

impl Backend for FakeBackend {
    type Handle = u64;

    fn snapshot(&mut self) -> anyhow::Result<Vec<(Self::Handle, anyhow::Result<WindowInfo<Self::Handle>>)>> {
        let mut calls = self.calls.borrow_mut();
        let attempted = calls.attempted.len();
        calls.attempts_at_snapshot.push(attempted);
        self.snapshots.pop_front().unwrap_or_else(|| Ok(Vec::new()))
    }

    fn perform(&mut self, target: Self::Handle, action: WindowAction) -> anyhow::Result<()> {
        self.calls.borrow_mut().attempted.push((target, action));
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
    backend.snapshots.push_back(Ok(vec![(1, Ok(window(1)))]));
    let calls = Rc::clone(&backend.calls);
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
    assert_eq!(calls.borrow().attempted, [
        (1, WindowAction::Center),
        (2, WindowAction::Resize(Size2D::new(1280, 720))),
    ]);
    assert_eq!(calls.borrow().attempts_at_snapshot, [2, 2]);
}

#[test]
fn failed_action_does_not_prevent_later_targets_or_refresh() {
    let rules = parse_config("[[rules]]\nname = 'all'").unwrap().rules;
    let mut backend = FakeBackend { failing_handle: Some(1), ..Default::default() };
    backend.snapshots.push_back(Ok(vec![(2, Ok(window(2)))]));
    let calls = Rc::clone(&backend.calls);
    let mut engine = Engine::new(rules, backend);
    engine.queue([
        (1, WindowAction::Center),
        (2, WindowAction::Center),
    ]);

    engine.update();

    assert_eq!(engine.view().unwrap().groups[0].windows[0].handle, 2);
    assert_eq!(calls.borrow().attempted, [
        (1, WindowAction::Center),
        (2, WindowAction::Center),
    ]);
}

#[test]
fn failed_snapshot_clears_previously_visible_sections() {
    let rules = parse_config("[[rules]]\nname = 'all'").unwrap().rules;
    let mut backend = FakeBackend::default();
    backend.snapshots.push_back(Ok(vec![(1, Ok(window(1)))]));
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
    backend.snapshots.push_back(Ok(vec![(1, Ok(window(1)))]));
    let mut engine = Engine::new(rules, backend);
    engine.update();

    assert_eq!(engine.rule("fallback").unwrap().name, "fallback");
    assert_eq!(engine.view().unwrap().groups[0].rule_name, "tool");
    let rule = engine.rule("tool").unwrap();
    assert_eq!(rule.description, "  Tool  ");
    assert_eq!(rule.program.name, Some(turbozone_core::Pattern::Exact("APP.EXE".into())));
    assert!(engine.rule("unknown").is_none());
}

/// Interleaved failures and an unmatched success must survive in raw enumeration order.
/// Repeated queries preserve selected values and native causes without another backend call.
#[test]
fn raw_snapshot_and_views_preserve_interleaved_outcomes_and_error_causes() {
    let rules = parse_config("[[rules]]\nname = 'selected'\nwindow.title = 'Selected'").unwrap().rules;
    let mut first = window(4);
    first.title = "Selected".into();
    let mut second = window(1);
    second.title = "Selected".into();
    let error = anyhow::Error::new(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
        .context("program path");
    let backend = FakeBackend {
        snapshots: [Ok(vec![
            (9, Err(anyhow::anyhow!("gone"))),
            (4, Ok(first)),
            (3, Err(error)),
            (2, Ok(window(2))),
            (1, Ok(second)),
        ])].into(),
        ..Default::default()
    };
    let calls = Rc::clone(&backend.calls);
    let mut engine = Engine::new(rules, backend);
    engine.update();
    let raw = engine.snapshot().as_ref().unwrap();
    assert_eq!(raw.iter().map(|&(handle, ref result)| (handle, result.is_ok()))
        .collect::<Vec<_>>(), [(9, false), (4, true), (3, false), (2, true), (1, true)]);
    assert_eq!(raw[2].1.as_ref().unwrap_err().downcast_ref::<std::io::Error>().unwrap().kind(),
        std::io::ErrorKind::PermissionDenied);
    for _ in 0..2 {
        let view = engine.view().unwrap();
        assert_eq!(view.groups[0].windows.iter().map(|window| window.handle)
            .collect::<Vec<_>>(), [4, 1]);
        assert_eq!(view.groups[0].program.path, "C:/Apps/app.exe");
        assert!(view.groups[0].windows.iter().all(|window| window.title == "Selected"));
        assert_eq!(view.errors[0].1.to_string(), "gone");
        assert_eq!(view.errors[1].1.downcast_ref::<std::io::Error>().unwrap().kind(),
            std::io::ErrorKind::PermissionDenied);
        assert_eq!(view.errors.iter().map(|&(handle, _)| handle).collect::<Vec<_>>(), [9, 3]);
    }
    assert_eq!(calls.borrow().attempts_at_snapshot, [0]);
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
                Ok(vec![(1, Err(anyhow::anyhow!("client size unavailable")))]),
                Ok(vec![(1, Ok(recovered))]),
                Ok(vec![(1, Err(anyhow::anyhow!("failed again")))]),
                Err(anyhow::anyhow!("enumeration unavailable")),
                Ok(Vec::new()),
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
        assert_eq!(engine.view().unwrap_err().to_string(), "enumeration unavailable");
        engine.update();
        assert!(engine.view().unwrap().errors.is_empty());
    }
}

#[test]
fn construction_is_inert_and_updates_replace_failure_with_recovery() {
    let backend = FakeBackend {
        snapshots: [Err(anyhow::anyhow!("unavailable")), Ok(Vec::new())].into(),
        ..Default::default()
    };
    let calls = Rc::clone(&backend.calls);
    let mut engine = Engine::new(Vec::new(), backend);
    assert!(engine.view().unwrap().groups.is_empty());
    assert_eq!(calls.borrow().attempts_at_snapshot, [] as [usize; 0]);
    engine.update();
    assert_eq!(engine.view().unwrap_err().to_string(), "unavailable");
    engine.update();
    assert!(engine.view().unwrap().groups.is_empty());
    assert_eq!(calls.borrow().attempts_at_snapshot, [0, 0]);
}
