use eframe::egui::{Context, RawInput, Shape};
use turbozone_core::{group_windows, parse_config};
use turbozone_ui::ui::app_ui;

#[path = "support/window.rs"]
mod fixture;
use fixture::window;

/// Runs the public view headlessly; snapshots are never sent to native actions.
fn rendered_text(source: &str) -> Vec<String> {
    let config = parse_config(source).unwrap().runtime;
    let sections = group_windows(&config, vec![window("C:/Apps/App.exe", "Application")]);
    let mut actions = None;
    let mut output = Context::default().run_ui(RawInput::default(), |ui| {
        actions = Some(app_ui(ui, &sections, &config));
    });
    assert!(actions.unwrap().is_empty(), "rendering without interaction must not queue native actions");
    // No renderer consumes texture uploads in this integration test.
    output.textures_delta.clear();
    output.shapes.into_iter().filter_map(|shape| match shape.shape {
        Shape::Text(text) => Some(text.galley.text().to_owned()),
        _ => None,
    }).collect()
}

#[test]
fn empty_config_renders_the_matched_view_without_diagnostic_navigation() {
    assert_eq!(rendered_text(""), ["TurboZone", "No matched windows found"]);
}

#[test]
fn matched_sections_keep_metadata_and_actionless_rules() {
    let text = rendered_text("[[rules]]\nname = 'app'\ndescription = ' My Application '");
    for expected in ["READ ONLY", "My Application", "C:/Apps/App.exe", "Application", "PID 42", "App.exe", "640x480"] {
        assert!(text.iter().any(|text| text == expected), "missing {expected}: {text:?}");
    }
    assert!(!text.iter().any(|text| text.contains("RESIZE") || text.contains("CENTER")));
}

#[test]
fn resize_modes_preserve_primary_and_selector_controls() {
    for (resize, primary, selector) in [
        ("resize = false", None, false),
        ("resize = true", None, true),
        ("resize = [1280, 720]", Some("RESIZE 1280x720"), true),
        ("resize.exact = [1280, 720]", Some("RESIZE 1280x720"), false),
        ("resize.default = [1280, 720]", Some("RESIZE 1280x720"), true),
    ] {
        let source = format!("[[rules]]\nname = 'app'\nmove = true\n{resize}");
        let text = rendered_text(&source);
        assert!(text.iter().any(|text| text == "CENTER ALL"));
        assert!(text.iter().any(|text| text == "CENTER"));
        if let Some(primary) = primary {
            assert!(text.iter().any(|text| text == primary), "{resize}");
            assert_eq!(text.iter().any(|text| text == "\u{25bc}"), selector, "{resize}");
        } else {
            assert_eq!(text.iter().any(|text| text == "RESIZE"), selector, "{resize}");
        }
    }
}
