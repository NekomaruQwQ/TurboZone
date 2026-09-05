use eframe::egui::{Context, RawInput, Shape};
use smol_str::{SmolStr, format_smolstr};
use turbozone_core::{Engine, parse_config};
use turbozone_ui::app::{App, TURBOZONE_WINDOW_SIZE};

#[path = "support/window.rs"]
mod fixture;
use fixture::{TestBackend, window};

/// Exercises verified rules through the real engine and public headless view boundary.
/// The fixture backend supplies owned windows; rendering only records proposed actions.
fn rendered_text(source: &str) -> Vec<SmolStr> {
    let rules = parse_config(source).unwrap().rules;
    let backend = TestBackend { windows: vec![window("C:/Apps/App.exe", "Application")] };
    let mut engine = Engine::new(rules, backend);
    engine.tick();
    let mut actions = Vec::new();
    let mut output = Context::default().run_ui(RawInput::default(), |ui| {
        App::<TestBackend>::app_ui(ui, &mut engine, &mut |action| actions.push(action));
    });
    assert!(
        actions.is_empty(),
        "rendering without interaction must not queue native actions");
    // No renderer consumes texture uploads in this integration test.
    output.textures_delta.clear();
    output.shapes.into_iter().filter_map(|shape| match shape.shape {
        Shape::Text(text) => Some(SmolStr::new(text.galley.text())),
        _ => None,
    }).collect()
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
    for (resize, group_primary, selector, window_primary) in [
        ("resize = false", None, false, false),
        ("resize = true", None, true, false),
        ("resize = {}", None, true, false),
        ("resize = [1280, 720]", Some("RESIZE TO 1280x720"), true, true),
        ("resize.exact = [1280, 720]", Some("RESIZE TO 1280x720"), false, true),
        ("resize.default = [1280, 720]", Some("RESIZE TO 1280x720"), true, true),
        ("resize = { default = [1280, 720], max = [960, 540] }",
            Some("RESIZE TO 1280x720"), true, false),
    ] {
        let source = format_smolstr!("[[rules]]\nname = 'app'\nmove = true\n{resize}");
        let text = rendered_text(&source);
        assert!(text.iter().any(|text| text == "CENTER"), "{resize}: {text:?}");
        assert!(text.iter().any(|text| text.ends_with(" CENTER")), "{resize}: {text:?}");
        assert_eq!(
            text.iter().any(|text| text == "RESIZE"),
            selector,
            "{resize}: {text:?}");
        assert_eq!(
            text.iter().any(|text| text == "1280x720"),
            window_primary,
            "{resize}: {text:?}");
        assert_eq!(
            text.iter().find(|text| text.starts_with("RESIZE TO")).map(SmolStr::as_str),
            group_primary,
            "{resize}: {text:?}");
        assert_eq!(text.iter().any(|text| text == "SELECT"), selector, "{resize}: {text:?}");
    }
}
