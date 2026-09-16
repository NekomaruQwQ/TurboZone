//! Public rendering, interaction, and refresh behavior exercised without native GUI startup.

use turbozone_core::config_parser::parse_config;
use eframe::egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape, Vec2};
use euclid::default::{Point2D, Size2D};
use smol_str::{SmolStr, format_smolstr};
use turbozone_core::{Backend, WindowAction, WindowInfo};
use turbozone_ui::app::{App, TURBOZONE_WINDOW_SIZE};
use eframe::App as _;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

#[path = "support/window.rs"]
mod fixture;
use fixture::{TestBackend, window};

/// Drives the public framework callbacks with pointer input and observes backend effects.
/// Text positions come from egui output, so no private widget IDs or app fields are needed.
struct View<B: Backend = TestBackend> {
    context: Context,
    app: App<RecordingBackend<B>>,
    calls: Rc<RefCell<BackendCalls<B::Handle>>>,
    labels: Vec<(SmolStr, Rect)>,
}

/// Records only interactions exposed by the backend contract, independently of App storage.
struct BackendCalls<H> {
    actions: Vec<(H, WindowAction)>,
    snapshots: usize,
}

struct RecordingBackend<B: Backend> {
    inner: B,
    calls: Rc<RefCell<BackendCalls<B::Handle>>>,
}

impl<B: Backend> Backend for RecordingBackend<B> {
    type Handle = B::Handle;

    fn snapshot(&mut self) -> anyhow::Result<Vec<(Self::Handle, anyhow::Result<WindowInfo<Self::Handle>>)>> {
        self.calls.borrow_mut().snapshots += 1;
        self.inner.snapshot()
    }

    fn perform(&mut self, target: Self::Handle, action: WindowAction) -> anyhow::Result<()> {
        self.calls.borrow_mut().actions.push((target, action));
        self.inner.perform(target, action)
    }
}

impl View {
    fn new(source: &str, windows: Vec<WindowInfo<u64>>) -> Self {
        Self::with_backend(source, TestBackend {
            snapshots: [Ok(windows.into_iter().map(|window| (window.handle, Ok(window))).collect())].into(),
        })
    }
}

impl<B: Backend> View<B> {
    fn with_backend(source: &str, backend: B) -> Self {
        let rules = parse_config(source).unwrap().rules;
        let calls = Rc::new(RefCell::new(BackendCalls { actions: Vec::new(), snapshots: 0 }));
        let app = App::new(rules, RecordingBackend { inner: backend, calls: Rc::clone(&calls) });
        let context = Context::default();
        App::<B>::setup_egui(&context);
        let mut view = Self { context, app, calls, labels: Vec::new() };
        view.frame(Vec::new());
        view
    }

    fn frame(&mut self, events: Vec<Event>) {
        let snapshots_before = self.calls.borrow().snapshots;
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
        assert_eq!(self.calls.borrow().snapshots, snapshots_before, "painting must not capture native state");
        assert!(self.calls.borrow().actions.is_empty(), "painting must defer actions to logic");
    }

    fn logic(&mut self) {
        self.app.logic(&self.context, &mut eframe::Frame::_new_kittest());
    }

    /// Public logic observes real elapsed time; no clock state is exposed or overwritten.
    fn refresh(&mut self) {
        let before = self.calls.borrow().snapshots;
        std::thread::sleep(Duration::from_millis(350));
        self.logic();
        assert_eq!(self.calls.borrow().snapshots, before + 1, "elapsed polling interval must trigger capture");
    }

    /// Occurrence zero addresses group controls; subsequent occurrences address windows.
    fn click(&mut self, label: &str, occurrence: usize) -> Vec<(B::Handle, WindowAction)> {
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

fn rendered_text(source: &str) -> Vec<SmolStr> {
    View::new(source, vec![window("C:/Apps/App.exe", "Application")])
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
    assert_eq!(rendered_text(""), ["- nothing here -"]);
}

#[test]
fn viewport_keeps_the_approved_comfortable_size() {
    let viewport = App::<TestBackend>::viewport();
    let expected = Some(Vec2::new(450.0, 720.0));
    assert_eq!(viewport.inner_size, expected);
    assert_eq!(viewport.min_inner_size, expected);
    assert_eq!(viewport.max_inner_size, expected);
    assert_eq!(viewport.resizable, Some(false));
    assert_eq!(viewport.maximize_button, Some(false));
}

#[test]
fn actionless_groups_keep_only_the_flat_program_and_window_hierarchy() {
    let text = rendered_text("[[rules]]\nname = 'app'\ndescription = ' My Application '");
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
    for (resize, primary, selector) in [
        ("resize = false", false, false),
        ("resize = true", false, true),
        ("resize = {}", false, true),
        ("resize = [1280, 720]", true, true),
        ("resize.exact = [1280, 720]", true, false),
        ("resize.default = [1280, 720]", true, true),
        ("resize = { default = [1280, 720], max = [960, 540] }",
            false, true),
        ("resize = { default = [1280, 720], min = [1281, 720] }",
            false, true),
    ] {
        let source = format_smolstr!("[[rules]]\nname = 'app'\nmove = true\n{resize}");
        let text = rendered_text(&source);
        assert_eq!(text.iter().filter(|text| text.ends_with(" CENTER")).count(), 2);
        assert_eq!(
            text.iter().filter(|text| text.ends_with(" 1280x720")).count(),
            if primary { 2 } else { 0 },
            "{resize}: {text:?}");
        assert_eq!(
            text.iter().filter(|text| text.as_str() == "SELECT").count(),
            if selector { 2 } else { 0 },
            "{resize}: {text:?}");
    }
}

#[test]
fn group_and_individual_buttons_capture_their_rendered_targets() {
    let source = "[[rules]]\nname = 'app'\nmove = true\nresize.exact = [1280, 720]";
    let mut view = View::new(source, grouped_windows());
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
    let source = "[[rules]]\nname = 'app'\nmove = true\nresize.exact = [1280, 720]";
    let mut windows = grouped_windows();
    for window in &mut windows {
        window.content_rect.origin = Point2D::new(320, 180);
        window.content_rect.size = Size2D::new(1280, 720);
    }
    let mut view = View::new(source, windows);
    for label in ["\u{2705} CENTER", "\u{2705} 1280x720"] {
        for occurrence in 0..3 {
            assert_eq!(view.click(label, occurrence), []);
        }
    }
}

#[test]
fn one_satisfied_window_does_not_disable_actions_for_the_group() {
    let source = "[[rules]]\nname = 'app'\nmove = true\nresize = [1280, 720]";
    let mut windows = grouped_windows();
    windows[0].content_rect.origin = Point2D::new(320, 180);
    windows[0].content_rect.size = Size2D::new(1280, 720);
    let mut view = View::new(source, windows);
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
    let source = "[[rules]]\nname = 'app'\n\
        resize = { default = [1280, 720], min = [960, 540], max = [960, 600] }";
    let action = WindowAction::Resize(Size2D::new(960, 540));
    for (occurrence, targets) in [(0, vec![(1, action), (2, action)]), (2, vec![(2, action)])] {
        let mut view = View::new(source, grouped_windows());
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
    let source = "[[rules]]\nname = 'app'\nresize.max = [100, 100]";
    let mut view = View::new(source, grouped_windows());
    assert_eq!(view.click("SELECT", 0), []);
    view.frame(Vec::new());
    assert!(view.labels.iter().any(|entry| entry.0 == "No sizes within configured limits"));
}

#[test]
fn window_failures_coexist_with_controls_and_recover_without_stale_diagnostics() {
    let source = "[[rules]]\nname = 'app'\nmove = true";
    // Reverse handle order and interleave a failure to catch accidental sorting or
    // partition-based reconstruction of the usable rows and group action targets.
    let mut initial: Vec<_> = grouped_windows().into_iter().rev()
        .map(|window| (window.handle, Ok(window))).collect();
    initial.insert(1, (7, Err(anyhow::anyhow!("access denied").context("program path"))));
    let backend = TestBackend {
        snapshots: [
            Ok(initial),
            Err(anyhow::anyhow!("desktop unavailable").context("enumeration")),
            Ok(grouped_windows().into_iter().map(|window| (window.handle, Ok(window))).collect()),
            Ok(vec![(7, Err(anyhow::anyhow!("still unavailable")))]),
            Ok(Vec::new()),
        ].into(),
    };
    let mut view = View::with_backend(source, backend);
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
    let backend = TestBackend {
        snapshots: [Ok(vec![
            (9, Err(anyhow::anyhow!("{cause}").context("capture"))),
            (3, Err(anyhow::anyhow!("short cause"))),
        ])].into(),
    };
    let mut view = View::with_backend("", backend);
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
    struct ErrorBackend;
    impl Backend for ErrorBackend {
        type Handle = NativeHandle;
        fn snapshot(&mut self) -> anyhow::Result<Vec<(NativeHandle, anyhow::Result<WindowInfo<NativeHandle>>)>> {
            Ok(vec![(NativeHandle, Err(anyhow::anyhow!("capture unavailable")))])
        }
        #[expect(clippy::panic_in_result_fn, reason = "painting a diagnostic must never perform a native action")]
        fn perform(&mut self, _: NativeHandle, _: WindowAction) -> anyhow::Result<()> {
            panic!("diagnostics have no native action");
        }
    }
    let mut view = View::with_backend("", ErrorBackend);
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

/// A slow backend makes start-based scheduling observably wrong even without reading a clock field.
/// Calls immediately after completion must wait, whether capture failed or recovered.
#[test]
fn slow_failed_and_successful_captures_are_paced_from_completion() {
    struct SlowBackend { attempt: usize }
    impl Backend for SlowBackend {
        type Handle = u64;

        fn snapshot(&mut self) -> anyhow::Result<Vec<(u64, anyhow::Result<WindowInfo<u64>>)>> {
            std::thread::sleep(Duration::from_millis(350));
            self.attempt += 1;
            if self.attempt < 3 { anyhow::bail!("enumeration failed"); }
            Ok(Vec::new())
        }

        fn perform(&mut self, _: u64, _: WindowAction) -> anyhow::Result<()> { Ok(()) }
    }
    let mut view = View::with_backend("", SlowBackend { attempt: 0 });
    assert_eq!(view.calls.borrow().snapshots, 1, "startup captures immediately");
    for expected in [1, 2, 3] {
        view.logic();
        assert_eq!(view.calls.borrow().snapshots, expected, "completion starts a new polling interval");
        if expected < 3 { view.refresh(); }
    }
    view.frame(Vec::new());
    assert_eq!(view.labels.iter().map(|entry| entry.0.as_str()).collect::<Vec<_>>(),
        ["- nothing here -"]);
}

#[test]
fn a_clicked_action_bypasses_the_polling_delay_and_is_not_replayed() {
    let mut view = View::new("[[rules]]\nname = 'app'\nmove = true", grouped_windows());
    let before = view.calls.borrow().snapshots;
    let started = Instant::now();
    assert_eq!(view.click("\u{26ab} CENTER", 0), [
        (1, WindowAction::Center), (2, WindowAction::Center),
    ]);
    assert!(started.elapsed() < Duration::from_millis(333), "the action must not wait for periodic capture");
    assert_eq!(view.calls.borrow().snapshots, before + 1);
    view.logic();
    assert_eq!(view.calls.borrow().actions, []);
    assert_eq!(view.calls.borrow().snapshots, before + 1);
}
