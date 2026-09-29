//! Public rendering, interaction, and refresh behavior exercised through a fake Engine.

use eframe::egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape, Vec2};
use euclid::default::Size2D;
use smol_str::SmolStr;
use turbozone_core::*;
use turbozone_ui::app::{App, TURBOZONE_WINDOW_SIZE};
use eframe::App as _;
use std::cell::RefCell;
use std::fmt::{Debug, Display};
use std::hash::Hash;
use std::rc::Rc;
use std::time::{Duration, Instant};

#[path = "support/window.rs"]
mod fixture;
use fixture::{Calls, Scene, TestEngine, TestPlatform, window};

/// Drives public callbacks and records engine commands without depending on EngineImpl.
struct View<H: Debug + Display + Copy + Eq + Hash + 'static = u64> {
    context: Context,
    app: App<TestEngine<H>, TestPlatform<H>>,
    calls: Rc<RefCell<Calls<H>>>,
    labels: Vec<(SmolStr, Rect)>,
}

impl View {
    fn new(actions: ActionAvailability, windows: Vec<WindowInfo<u64>>) -> Self {
        Self::with_engine(TestEngine::new(vec![Ok(Scene::group(windows, actions))]))
    }
}

impl<H: Debug + Display + Copy + Eq + Hash + 'static> View<H> {
    fn with_engine(engine: TestEngine<H>) -> Self {
        let calls = Rc::clone(engine.calls());
        let app = App::new(engine);
        let context = Context::default();
        App::<TestEngine<H>, TestPlatform<H>>::setup_egui(&context);
        let mut view = Self { context, app, calls, labels: Vec::new() };
        view.frame(Vec::new());
        view
    }

    fn frame(&mut self, events: Vec<Event>) {
        let updates_before = self.calls.borrow().updates;
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::from(TURBOZONE_WINDOW_SIZE))),
            events,
            ..RawInput::default()
        };
        let mut output = self.context.run_ui(input, |ui| {
            self.app.ui(ui, &mut eframe::Frame::_new_kittest());
        });
        // No renderer consumes texture uploads in this integration test.
        output.textures_delta.clear();
        self.labels = output.shapes.into_iter().filter_map(|shape| match shape.shape {
            Shape::Text(text) => Some((
                SmolStr::new(text.galley.text()),
                text.galley.rect.translate(text.pos.to_vec2()))),
            _ => None,
        }).collect();
        assert_eq!(self.calls.borrow().updates, updates_before, "painting must not capture application state");
        assert!(self.calls.borrow().actions.is_empty(), "painting must defer actions to logic");
    }

    fn logic(&mut self) {
        self.app.logic(&self.context, &mut eframe::Frame::_new_kittest());
    }

    /// Public logic observes real elapsed time; no clock state is exposed or overwritten.
    fn refresh(&mut self) {
        let before = self.calls.borrow().updates;
        std::thread::sleep(Duration::from_millis(350));
        self.logic();
        assert_eq!(self.calls.borrow().updates, before + 1, "elapsed polling interval must trigger capture");
    }

    /// Occurrence zero addresses group controls; subsequent occurrences address windows.
    fn click(&mut self, label: &str, occurrence: usize) -> Vec<(H, WindowAction)> {
        let position = self.labels.iter()
            .filter(|entry| entry.0 == label)
            .nth(occurrence)
            .unwrap_or_else(|| panic!("missing label {label} occurrence {occurrence}: {:?}", self.labels))
            .1.center();
        for pressed in [true, false] {
            self.frame(vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::default(),
                },
            ]);
        }
        self.logic();
        std::mem::take(&mut self.calls.borrow_mut().actions)
    }
}


const fn actions() -> ActionAvailability {
    ActionAvailability { center: ActionState::Available, resize: ResizeAvailability::Disabled }
}

const fn exact(satisfied: bool) -> ActionAvailability {
    ActionAvailability {
        center: if satisfied { ActionState::Satisfied } else { ActionState::Available },
        resize: ResizeAvailability::Exact(ResizeTarget { size: Size2D::new(1280, 720), satisfied }),
    }
}

const fn selector(min: Option<Size2D<i32>>, max: Option<Size2D<i32>>) -> ActionAvailability {
    ActionAvailability {
        center: ActionState::Unavailable,
        resize: ResizeAvailability::Selector {
            primary: None,
            settings: ResizeSelector { default: None, min, max },
        },
    }
}

fn rendered_text(actions: ActionAvailability) -> Vec<SmolStr> {
    View::new(actions, vec![window("C:/Apps/App.exe", "Application")])
        .labels.into_iter().map(|(text, _)| text).collect()
}

/// Two handles share the same rule and executable so group actions must fan out to both.
fn grouped_windows() -> Vec<WindowInfo<u64>> {
    let first = window("C:/Apps/App.exe", "First");
    let mut second = window("C:/Apps/App.exe", "Second");
    second.handle = 2;
    vec![first, second]
}

#[test]
fn empty_matches_render_the_quiet_inline_state() {
    let view = View::<u64>::with_engine(TestEngine::new(vec![Ok(Scene::empty())]));
    assert_eq!(view.labels.iter().map(|entry| entry.0.as_str()).collect::<Vec<_>>(), ["- nothing here -"]);
}

#[test]
fn viewport_keeps_the_approved_comfortable_size() {
    let viewport = App::<TestEngine<u64>, TestPlatform<u64>>::viewport();
    let expected = Some(Vec2::new(450.0, 720.0));
    assert_eq!(viewport.inner_size, expected);
    assert_eq!(viewport.min_inner_size, expected);
    assert_eq!(viewport.max_inner_size, expected);
    assert_eq!(viewport.resizable, Some(false));
    assert_eq!(viewport.maximize_button, Some(false));
}

#[test]
fn actionless_groups_keep_only_the_flat_program_and_window_hierarchy() {
    let text = rendered_text(ActionAvailability {
        center: ActionState::Unavailable, resize: ResizeAvailability::Disabled,
    });
    for expected in [
        "App Description",
        "app",
        "C:/Apps/App.exe",
        "\u{1f5d6}",
        "Application",
        "MOVE DISABLED",
        "RESIZE DISABLED",
    ] {
        assert!(text.iter().any(|text| text == expected), "missing {expected}: {text:?}");
    }
    for removed in ["TurboZone", "READ ONLY", "PID 42", "App.exe", "My Application"] {
        assert!(!text.iter().any(|text| text == removed), "unexpected {removed}: {text:?}");
    }
    assert!(!text.iter().any(|text| text == "RESIZE" || text.contains("CENTER")));
}

#[test]
fn resize_modes_preserve_primary_and_selector_controls() {
    let target = ResizeTarget { size: Size2D::new(1280, 720), satisfied: false };
    for (resize, primary, selector) in [
        (ResizeAvailability::Disabled, false, false),
        (ResizeAvailability::Exact(target), true, false),
        (ResizeAvailability::Selector { primary: None, settings: ResizeSelector::default() }, false, true),
        (ResizeAvailability::Selector { primary: Some(target), settings: ResizeSelector::default() }, true, true),
    ] {
        let text = rendered_text(ActionAvailability { center: ActionState::Available, resize });
        assert_eq!(text.iter().filter(|text| text.ends_with(" CENTER")).count(), 2);
        assert_eq!(text.iter().filter(|text| text.ends_with(" 1280x720")).count(),
            if primary { 2 } else { 0 }, "{text:?}");
        assert_eq!(text.iter().filter(|text| text.as_str() == "SELECT").count(),
            if selector { 2 } else { 0 }, "{text:?}");
    }
}

#[test]
fn group_and_individual_buttons_capture_their_rendered_targets() {
    let mut view = View::new(exact(false), grouped_windows());
    for (label, action) in [
        ("\u{26ab} CENTER", WindowAction::Center),
        ("\u{26ab} 1280x720", WindowAction::Resize(Size2D::new(1280, 720))),
    ] {
        assert_eq!(view.click(label, 0), [(1, action), (2, action)]);
        assert_eq!(view.click(label, 2), [(2, action)]);
    }
}

#[test]
fn satisfied_controls_are_disabled_for_windows_and_complete_groups() {
    let mut view = View::new(exact(true), grouped_windows());
    for label in ["\u{2705} CENTER", "\u{2705} 1280x720"] {
        for occurrence in 0..3 {
            assert_eq!(view.click(label, occurrence), []);
        }
    }
}

#[test]
fn one_satisfied_window_does_not_disable_actions_for_the_group() {
    let mut scene = Scene::group(grouped_windows(), exact(false));
    scene.groups[0].windows[0].1 = exact(true);
    let mut view = View::with_engine(TestEngine::new(vec![Ok(scene)]));
    assert_eq!(view.click("\u{2705} CENTER", 0), []);
    assert_eq!(view.click("\u{2705} 1280x720", 0), []);
    assert_eq!(view.click("\u{26ab} CENTER", 0), [
        (1, WindowAction::Center), (2, WindowAction::Center),
    ]);
    assert_eq!(view.click("\u{26ab} 1280x720", 0), [
        (1, WindowAction::Resize(Size2D::new(1280, 720))),
        (2, WindowAction::Resize(Size2D::new(1280, 720))),
    ]);
}

#[test]
fn selectors_keep_bounds_and_emit_group_or_individual_actions() {
    let controls = selector(Some(Size2D::new(960, 540)), Some(Size2D::new(960, 600)));
    let action = WindowAction::Resize(Size2D::new(960, 540));
    for (occurrence, targets) in [(0, vec![(1, action), (2, action)]), (2, vec![(2, action)])] {
        let mut view = View::new(controls.clone(), grouped_windows());
        assert_eq!(view.click("SELECT", occurrence), []);
        // Popups settle their layout on the frame after opening.
        view.frame(Vec::new());
        let choices: Vec<_> = view.labels.iter().filter(|entry| entry.0.contains('\u{00d7}'))
            .map(|entry| entry.0.as_str()).collect();
        assert_eq!(choices, ["960\u{00d7}600", "960\u{00d7}540"]);
        assert_eq!(view.click("960\u{00d7}540", 0), targets);
    }
}

#[test]
fn selector_with_no_manifest_choices_explains_its_empty_state() {
    let mut view = View::new(selector(None, Some(Size2D::new(100, 100))), grouped_windows());
    assert_eq!(view.click("SELECT", 0), []);
    view.frame(Vec::new());
    assert!(view.labels.iter().any(|entry| entry.0 == "No sizes within configured limits"));
}

#[test]
fn window_failures_coexist_with_controls_and_recover_without_stale_diagnostics() {
    let mut initial = Scene::group(grouped_windows().into_iter().rev().collect(), actions());
    initial.errors.push((7, anyhow::anyhow!("access denied").context("program path")));
    let engine = TestEngine::new(vec![
        Ok(initial),
        Err(anyhow::anyhow!("desktop unavailable").context("enumeration")),
        Ok(Scene::group(grouped_windows(), actions())),
        Ok(Scene { groups: Vec::new(), errors: vec![(7, anyhow::anyhow!("still unavailable"))] }),
        Ok(Scene::empty()),
    ]);
    let mut view = View::with_engine(engine);
    assert!(view.labels.iter().any(|entry| entry.0 == "Windows with errors (1)"));
    assert!(view.labels.iter().any(|entry| entry.0 == "program path: access denied"));
    assert_eq!(view.labels.iter().filter(|entry| entry.0 == "First" || entry.0 == "Second")
        .map(|entry| entry.0.as_str()).collect::<Vec<_>>(), ["Second", "First"]);
    assert_eq!(view.click("7", 0), [], "failed rows only select their diagnostic text");
    assert_eq!(view.labels.iter().filter(|entry| entry.0.ends_with(" CENTER")).count(), 3);
    assert_eq!(view.click("\u{26ab} CENTER", 0), [
        (2, WindowAction::Center), (1, WindowAction::Center),
    ]);
    view.frame(Vec::new());
    let text: Vec<_> = view.labels.iter().map(|entry| entry.0.as_str()).collect();
    assert_eq!(text, [
        "Couldn't read windows", "enumeration: desktop unavailable", "Retrying automatically.",
    ]);

    view.refresh();
    view.frame(Vec::new());
    assert!(!view.labels.iter().any(|entry| entry.0.contains("errors") || entry.0.contains("unavailable")));
    assert_eq!(view.labels.iter().filter(|entry| entry.0.ends_with(" CENTER")).count(), 3);

    view.refresh();
    view.frame(Vec::new());
    assert_eq!(view.labels.iter().map(|entry| entry.0.as_str()).collect::<Vec<_>>(),
        ["Windows with errors (1)", "7", "still unavailable"]);
    view.refresh();
    view.frame(Vec::new());
    assert_eq!(view.labels.iter().map(|entry| entry.0.as_str()).collect::<Vec<_>>(),
        ["- nothing here -"]);
}

#[test]
fn error_only_view_sorts_rows_and_wraps_long_context_without_hiding_the_cause() {
    let cause = "A long executable path could not be read. ".repeat(12);
    let engine = TestEngine::new(vec![Ok(Scene {
        groups: Vec::new(),
        errors: vec![
            (9, anyhow::anyhow!("{cause}").context("capture")),
            (3, anyhow::anyhow!("short cause")),
        ],
    })]);
    let mut view = View::with_engine(engine);
    view.frame(Vec::new());
    let text: Vec<_> = view.labels.iter().map(|entry| entry.0.as_str()).collect();
    assert_eq!(&text[..4], ["Windows with errors (2)", "3", "short cause", "9"]);
    assert_eq!(text[4], format!("capture: {cause}"));
    assert!(view.labels[4].1.height() > view.labels[2].1.height());
    assert!(view.labels.iter().all(|entry| entry.1.max.x <= TURBOZONE_WINDOW_SIZE[0]));
}

/// Counting Display calls is thread-local because the library suite runs in parallel.
#[test]
fn repeated_paints_reuse_handle_descriptions_until_the_next_update() {
    use std::cell::Cell;
    use std::fmt;

    thread_local! { static CALLS: Cell<usize> = const { Cell::new(0) }; }
    #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
    struct NativeHandle;
    impl fmt::Display for NativeHandle {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            CALLS.set(CALLS.get() + 1);
            f.write_str("native-window-description")
        }
    }
    let mut view = View::with_engine(TestEngine::new(vec![Ok(Scene {
        groups: Vec::new(),
        errors: vec![(NativeHandle, anyhow::anyhow!("capture unavailable"))],
    })]));
    let initial_calls = CALLS.get();
    assert!(initial_calls > 0);
    assert!(view.labels.iter().any(|entry| entry.0 == "native-window-description"));
    for _ in 0..3 {
        view.frame(Vec::new());
    }
    assert_eq!(CALLS.get(), initial_calls);
    view.refresh();
    let refreshed_calls = CALLS.get();
    assert!(refreshed_calls > initial_calls);
    view.frame(Vec::new());
    assert_eq!(CALLS.get(), refreshed_calls);
}

/// A slow engine makes start-based scheduling observably wrong even without reading a clock field.
/// Calls immediately after completion must wait, whether capture failed or recovered.
#[test]
fn slow_failed_and_successful_captures_are_paced_from_completion() {
    let mut engine = TestEngine::new(vec![
        Err(anyhow::anyhow!("enumeration failed")),
        Err(anyhow::anyhow!("enumeration failed")),
        Ok(Scene::<u64>::empty()),
    ]);
    engine.set_delay(Duration::from_millis(350));
    let mut view = View::with_engine(engine);
    assert_eq!(view.calls.borrow().updates, 1, "startup captures immediately");
    for expected in [1, 2, 3] {
        view.logic();
        assert_eq!(view.calls.borrow().updates, expected, "completion starts a new polling interval");
        if expected < 3 { view.refresh(); }
    }
    view.frame(Vec::new());
    assert_eq!(view.labels.iter().map(|entry| entry.0.as_str()).collect::<Vec<_>>(),
        ["- nothing here -"]);
}

#[test]
fn a_clicked_action_bypasses_the_polling_delay_and_is_not_replayed() {
    let mut view = View::new(actions(), grouped_windows());
    let before = view.calls.borrow().updates;
    let started = Instant::now();
    assert_eq!(view.click("\u{26ab} CENTER", 0), [
        (1, WindowAction::Center), (2, WindowAction::Center),
    ]);
    assert!(started.elapsed() < Duration::from_millis(333), "the action must not wait for periodic capture");
    assert_eq!(view.calls.borrow().updates, before + 1);
    view.logic();
    assert_eq!(view.calls.borrow().actions, []);
    assert_eq!(view.calls.borrow().updates, before + 1);
}
