use std::rc::Rc;

use euclid::default::{Point2D, Rect, Size2D};
use turbozone_core::{
    ProgramInfo, WindowDetail, WindowInfo, WindowState, find_rule, group_windows, parse_config,
};

/// Makes complete snapshots whose handles remain opaque to core grouping.
fn window(handle: u64, path: &str, title: &str) -> WindowInfo<u64> {
    WindowInfo {
        handle,
        title: title.into(),
        state: WindowState::Normal,
        detail: Ok(WindowDetail {
            monitor_rect: Rect::new(Point2D::zero(), Size2D::new(1920, 1080)),
            content_rect: Rect::new(Point2D::zero(), Size2D::new(640, 480)),
            process_id: 42,
            program: Rc::new(ProgramInfo {
                path: path.into(),
                name: path.rsplit('/').next().unwrap().into(),
                description: "Tool".into(),
            }),
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
    let mut failed = window(5, "C:/Apps/Tool.exe", "Tool failed");
    failed.detail = Err(anyhow::anyhow!("Client query failed"));
    let groups = group_windows(&rules, vec![
        window(1, "C:/Apps/Tool.exe", "Tool one"),
        window(2, "c:/apps/tool.EXE", "Tool two"),
        window(3, "C:/Apps/Tool.exe", "tool lowercase title"),
        window(4, "C:/Other/Tool.exe", "Tool other installation"),
        failed,
    ]);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].rule_name, "tool");
    assert_eq!(groups[0].program.path, "C:/Apps/Tool.exe");
    assert_eq!(groups[0].windows.len(), 2);
    assert!(Rc::ptr_eq(
        &groups[0].program,
        &groups[0].windows[0].detail.as_ref().unwrap().program));
    assert_eq!(groups[1].program.path, "C:/Other/Tool.exe");
}

#[test]
fn failed_details_never_match_even_unfiltered_rules_and_recovery_uses_new_details() {
    let rules = parse_config("[[rules]]\nname = 'all'").unwrap().rules;
    let mut failed = window(1, "C:/Apps/Tool.exe", "Tool");
    failed.detail = Err(anyhow::anyhow!("Program access denied"));
    assert!(group_windows(&rules, vec![failed]).is_empty());
    let groups = group_windows(&rules, vec![window(2, "C:/Apps/Other.exe", "Recovered")]);
    assert_eq!(groups[0].program.path, "C:/Apps/Other.exe");
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
    let groups = group_windows(&rules, vec![
        window(1, "C:/Apps/Tool.exe", "Specific"),
        window(2, "C:/Apps/Tool.exe", "Fallback"),
    ]);
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
    assert_eq!(find_rule(&reordered, &groups[0].rule_name).unwrap().name, "fallback");
}
