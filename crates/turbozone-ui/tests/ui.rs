use eframe::egui::{Context, RawInput, Shape};
use smol_str::{SmolStr, format_smolstr};
use turbozone_core::{group_windows, parse_config};
use turbozone_ui::app::{App, TURBOZONE_WINDOW_SIZE};

#[path = "support/window.rs"]
mod fixture;
use fixture::{TestBackend, window};

/// Runs the public pure view boundary; no renderer or native backend consumes its output.
fn rendered_text(source: &str) -> Vec<SmolStr> {
    let rules = parse_config(source).unwrap().rules;
    let groups = group_windows(&rules, vec![window("C:/Apps/App.exe", "Application")]);
    let mut actions = None;
    let mut output = Context::default().run_ui(RawInput::default(), |ui| {
        actions = Some(App::<TestBackend>::app_ui(ui, &groups, &rules));
    });
    assert!(
        actions.unwrap().is_empty(),
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
        "My Application",
        "C:/Apps/App.exe",
        "\u{1f5d6}",
        "Application",
        "\u{26ab} 640x480",
    ] {
        assert!(text.iter().any(|text| text == expected), "missing {expected}: {text:?}");
    }
    for removed in ["TurboZone", "READ ONLY", "PID 42", "App.exe"] {
        assert!(!text.iter().any(|text| text == removed), "unexpected {removed}: {text:?}");
    }
    assert!(!text.iter().any(|text| text.contains("RESIZE") || text.contains("CENTER")));
}

#[test]
fn resize_modes_map_to_the_m0_primary_and_selector_controls() {
    for (resize, group_primary, selector, window_primary) in [
        ("resize = false", None, false, false),
        ("resize = true", None, true, false),
        ("resize = [1280, 720]", Some("RESIZE ALL 1280x720"), true, true),
        ("resize.exact = [1280, 720]", Some("RESIZE ALL 1280x720"), false, true),
        ("resize.default = [1280, 720]", Some("RESIZE ALL 1280x720"), true, true),
    ] {
        let source = format_smolstr!("[[rules]]\nname = 'app'\nmove = true\n{resize}");
        let text = rendered_text(&source);
        assert!(text.iter().any(|text| text == "CENTER ALL"), "{resize}: {text:?}");
        assert!(text.iter().any(|text| text == "CENTER"), "{resize}: {text:?}");
        assert_eq!(
            text.iter().any(|text| text == "Resize All"),
            selector,
            "{resize}: {text:?}");
        assert_eq!(
            text.iter().any(|text| text == "RESIZE"),
            window_primary,
            "{resize}: {text:?}");
        assert_eq!(
            text.iter().find(|text| text.starts_with("RESIZE ALL")).map(SmolStr::as_str),
            group_primary,
            "{resize}: {text:?}");
        assert!(text.iter().any(|text| text == "\u{26ab} 640x480"), "{resize}: {text:?}");
    }
}
