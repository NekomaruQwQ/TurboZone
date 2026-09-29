use turbozone_engine::config_parser::parse_config;
use turbozone_core::{ProgramInfo, WindowInfo, WindowState};
use turbozone_engine::matches_rule;
use std::rc::Rc;
use euclid::default::{Point2D, Rect, Size2D};
use smol_str::format_smolstr;

const STARTS_WITH: &str = "\u{c4}BCDEFGHIJKLMNOPQRSTUVWXYZ";

/// Matching preserves Unicode lowercase behavior even when a literal exceeds
/// inline capacity; authored text is retained independently of comparisons.
#[test]
fn matching_handles_long_unicode_patterns_without_truncation() {
    let source = format_smolstr!("[[rules]]\nname = 'app'\nprogram.name = '{STARTS_WITH}'");
    let config = parse_config(&source).unwrap();
    let rule = &config.rules[0];
    let window = WindowInfo {
        handle: (),
        title: "Tool".into(),
        state: WindowState::Normal,
        monitor_rect: Rect::new(Point2D::zero(), Size2D::new(1920, 1080)),
        content_rect: Rect::new(Point2D::zero(), Size2D::new(640, 480)),
        program: Rc::new(ProgramInfo {
            path: "c:/apps/tool.exe".into(),
            name: "\u{e4}bcdefghijklmnopqrstuvwxyz".into(),
            description: "Tool".into(),
        }),
    };

    assert!(matches_rule(rule, &window));
}
