use turbozone_core::config_parser::parse_config;
use eframe::egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape, Vec2};
use euclid::default::{Point2D, Size2D};
use smol_str::{SmolStr, format_smolstr};
use turbozone_core::{Backend, WindowAction, WindowInfo};
use crate::app::{App, TURBOZONE_WINDOW_SIZE};
use super::AppTestExt as _;

#[path = "../support/window.rs"]
mod fixture;
use fixture::{TestBackend, window};

/// Drives the real App through its private rendering seam with real pointer events against one stable engine snapshot.
/// Text positions come from egui's output so tests do not depend on hard-coded coordinates
/// or private widget IDs. The backend rejects native effects during rendering.
struct View<B: Backend = TestBackend> {
    context: Context,
    app: App<B>,
    labels: Vec<(SmolStr, Rect)>,
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
        let app = App::new(rules, backend);
        let context = Context::default();
        App::<B>::setup_egui(&context);
        let mut view = Self { context, app, labels: Vec::new() };
        assert_eq!(view.frame(Vec::new()), [], "rendering without input must not emit actions");
        view
    }

    fn frame(&mut self, events: Vec<Event>) -> Vec<(B::Handle, WindowAction)> {
        let mut actions = Vec::new();
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::from(TURBOZONE_WINDOW_SIZE))),
            events,
            ..RawInput::default()
        };
        let mut output = self.context.run_ui(input, |ui| {
            self.app.render(ui, &mut |target, action| {
                actions.push((target, action));
            });
        });
        // No renderer consumes texture uploads in this integration test.
        output.textures_delta.clear();
        self.labels = output.shapes.into_iter().filter_map(|shape| match shape.shape {
            Shape::Text(text) => Some((
                SmolStr::new(text.galley.text()),
                text.galley.rect.translate(text.pos.to_vec2()))),
            _ => None,
        }).collect();
        actions
    }

    /// Occurrence zero addresses group controls; subsequent occurrences address windows.
    fn click(&mut self, label: &str, occurrence: usize) -> Vec<(B::Handle, WindowAction)> {
        let position = self.labels.iter()
            .filter(|entry| entry.0 == label)
            .nth(occurrence)
            .unwrap_or_else(|| panic!("missing label {label} occurrence {occurrence}: {:?}", self.labels))
            .1.center();
        let mut actions = Vec::new();
        for pressed in [true, false] {
            actions.extend(self.frame(vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::default(),
                },
            ]));
        }
        actions
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
    assert_eq!(
        TURBOZONE_WINDOW_SIZE.map(f32::to_bits),
        [450.0f32.to_bits(), 720.0f32.to_bits()]);
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
        assert_eq!(view.frame(Vec::new()), []);
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
    assert_eq!(view.frame(Vec::new()), []);
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
    assert_eq!(view.click("\u{26ab} CENTER", 0), [
        (2, WindowAction::Center), (1, WindowAction::Center),
    ]);
    assert_eq!(view.click("7", 0), [], "failed rows only select their diagnostic text");
    assert_eq!(view.labels.iter().filter(|entry| entry.0.ends_with(" CENTER")).count(), 3);

    view.app.refresh();
    assert_eq!(view.frame(Vec::new()), []);
    let text: Vec<_> = view.labels.iter().map(|entry| entry.0.as_str()).collect();
    assert_eq!(text, [
        "Couldn't read windows", "enumeration: desktop unavailable", "Retrying automatically.",
    ]);

    view.app.refresh();
    assert_eq!(view.frame(Vec::new()), []);
    assert!(!view.labels.iter().any(|entry| entry.0.contains("errors") || entry.0.contains("unavailable")));
    assert_eq!(view.click("\u{26ab} CENTER", 0).len(), 2);

    view.app.refresh();
    assert_eq!(view.frame(Vec::new()), []);
    assert_eq!(view.labels.iter().map(|entry| entry.0.as_str()).collect::<Vec<_>>(),
        ["Windows with errors (1)", "7", "still unavailable"]);
    view.app.refresh();
    assert_eq!(view.frame(Vec::new()), []);
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
    assert_eq!(view.frame(Vec::new()), []);
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
        assert_eq!(view.frame(Vec::new()), []);
    }
    assert_eq!(CALLS.get(), initial_calls);
    view.app.refresh();
    let refreshed_calls = CALLS.get();
    assert!(refreshed_calls > initial_calls);
    assert_eq!(view.frame(Vec::new()), []);
    assert_eq!(CALLS.get(), refreshed_calls);
}

#[test]
fn failed_attempts_use_the_same_completion_based_cadence_and_pending_actions_bypass_it() {
    use std::time::Duration;
    let mut app = App::new(Vec::new(), TestBackend {
        snapshots: [Err(anyhow::anyhow!("enumeration failed"))].into(),
    });
    let completed = app.completed_at();
    let interval = Duration::from_nanos(1_000_000_000 / 3);
    assert!(!app.update_due(completed));
    assert!(!app.update_due(completed + interval.checked_sub(Duration::from_nanos(1)).unwrap()));
    assert!(app.update_due(completed + interval));
    app.queue_action(1, WindowAction::Center);
    assert!(app.update_due(completed));
}
