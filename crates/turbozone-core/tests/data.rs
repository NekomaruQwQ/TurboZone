use turbozone_core::config_parser::parse_config;
use std::rc::Rc;

use euclid::default::{Point2D, Rect, Size2D};
use turbozone_core::{
    ProgramInfo, WindowInfo, WindowState, group_windows,
};

/// Makes complete snapshots whose handles remain opaque to core grouping.
fn window(handle: u64, path: &str, title: &str) -> WindowInfo<u64> {
    WindowInfo {
        handle,
        title: title.into(),
        state: WindowState::Normal,
        monitor_rect: Rect::new(Point2D::zero(), Size2D::new(1920, 1080)),
        content_rect: Rect::new(Point2D::zero(), Size2D::new(640, 480)),
        process_id: 42,
        program: Rc::new(ProgramInfo {
            path: path.into(),
            name: path.rsplit('/').next().unwrap().into(),
            description: "Tool".into(),
        }),
    }
}

#[test]
fn grouping_keeps_only_complete_matches_and_uses_case_insensitive_program_identity() {
    let rules = parse_config(r#"
        [[rules]]
        name = "tool"
        program.name = "TOOL.EXE"
        window.title.starts_with = "Tool"
    "#).unwrap().rules;
    let windows = [
        window(1, "C:/Apps/Tool.exe", "Tool one"),
        window(2, "c:/apps/tool.EXE", "Tool two"),
        window(3, "C:/Apps/Tool.exe", "tool lowercase title"),
        window(4, "C:/Other/Tool.exe", "Tool other installation"),
    ];
    let groups = group_windows(&rules, &windows);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].rule_name, "tool");
    assert_eq!(groups[0].program.path, "C:/Apps/Tool.exe");
    assert_eq!(groups[0].windows.len(), 2);
    assert!(std::ptr::eq(
        groups[0].program,
        groups[0].windows[0].program.as_ref()));
    assert_eq!(groups[1].program.path, "C:/Other/Tool.exe");
}

#[test]
fn groups_retain_stable_names_and_source_order_after_rule_reordering() {
    let rules = parse_config(r#"
        [[rules]]
        name = "fallback"
        [[rules]]
        name = "specific"
        priority = 10
        window.title = "Specific"
    "#).unwrap().rules;
    let windows = [
        window(1, "C:/Apps/Tool.exe", "Specific"),
        window(2, "C:/Apps/Tool.exe", "Fallback"),
    ];
    let groups = group_windows(&rules, &windows);
    assert_eq!(groups.iter().map(|group| group.rule_name.as_str())
        .collect::<Vec<_>>(), ["fallback", "specific"]);

    let reordered = parse_config(r#"
        [[rules]]
        name = "specific"
        priority = 10
        window.title = "Specific"
        [[rules]]
        name = "fallback"
    "#).unwrap().rules;
    let windows = [
        window(1, "C:/Apps/Tool.exe", "Specific"),
        window(2, "C:/Apps/Tool.exe", "Fallback"),
    ];
    let regrouped = group_windows(&reordered, &windows);
    assert_eq!(regrouped.iter().map(|group| group.rule_name.as_str())
        .collect::<Vec<_>>(), ["specific", "fallback"]);
}

#[test]
fn centered_query_uses_integer_centers_without_requiring_equal_rectangles() {
    let mut candidate = window(1, "C:/Apps/Tool.exe", "Tool");
    candidate.monitor_rect = Rect::new(Point2D::new(-100, 50), Size2D::new(1001, 801));
    candidate.content_rect = Rect::new(Point2D::new(150, 250), Size2D::new(501, 401));
    assert!(candidate.is_centered());
    candidate.content_rect.origin.x += 1;
    assert!(!candidate.is_centered());
}

#[test]
fn grouping_borrows_members_without_cloning_program_snapshots() {
    let rules = parse_config("[[rules]]\nname = 'all'").unwrap().rules;
    let windows = [window(1, "C:/Apps/Tool.exe", "Tool")];
    let before = Rc::strong_count(&windows[0].program);
    let first = group_windows(&rules, &windows);
    let second = group_windows(&rules, &windows);
    assert!(std::ptr::eq(first[0].windows[0], &raw const windows[0]));
    assert!(std::ptr::eq(second[0].windows[0], &raw const windows[0]));
    assert!(std::ptr::eq(first[0].rule_name, &raw const rules[0].name));
    assert_eq!(Rc::strong_count(&windows[0].program), before);
}
