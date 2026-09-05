use eframe::egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape, Vec2};
use euclid::default::{Point2D, Size2D};
use smol_str::{SmolStr, format_smolstr};
use turbozone_core::{Engine, WindowAction, WindowInfo, parse_config};
use turbozone_ui::app::{App, TURBOZONE_WINDOW_SIZE};

#[path = "support/window.rs"]
mod fixture;
use fixture::{TestBackend, window};

/// Drives the public view with real pointer events against one stable engine snapshot.
/// Text positions come from egui's output so tests do not depend on hard-coded coordinates
/// or private widget IDs. The backend rejects native effects during rendering.
struct View {
    context: Context,
    engine: Engine<TestBackend>,
    labels: Vec<(SmolStr, Rect)>,
}

impl View {
    fn new(source: &str, windows: Vec<WindowInfo<u64>>) -> Self {
        let rules = parse_config(source).unwrap().rules;
        let mut engine = Engine::new(rules, TestBackend { windows });
        engine.tick();
        let context = Context::default();
        App::<TestBackend>::setup_egui(&context);
        let mut view = Self { context, engine, labels: Vec::new() };
        assert_eq!(view.frame(Vec::new()), [], "rendering without input must not emit actions");
        view
    }

    fn frame(&mut self, events: Vec<Event>) -> Vec<(u64, WindowAction)> {
        let mut actions = Vec::new();
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::from(TURBOZONE_WINDOW_SIZE))),
            events,
            ..RawInput::default()
        };
        let mut output = self.context.run_ui(input, |ui| {
            App::<TestBackend>::app_ui(ui, &mut self.engine, &mut |target, action| {
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
    fn click(&mut self, label: &str, occurrence: usize) -> Vec<(u64, WindowAction)> {
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
        let detail = window.detail.as_mut().unwrap();
        detail.content_rect.origin = Point2D::new(320, 180);
        detail.content_rect.size = Size2D::new(1280, 720);
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
    let detail = windows[0].detail.as_mut().unwrap();
    detail.content_rect.origin = Point2D::new(320, 180);
    detail.content_rect.size = Size2D::new(1280, 720);
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
