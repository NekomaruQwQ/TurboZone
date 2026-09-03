use std::rc::Rc;

use euclid::default::{Point2D, Rect, Size2D};
use turbozone_core::{
    ProgramDetail, WindowDetail, WindowInfo, WindowState, group_windows, parse_config,
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
            program: Rc::new(ProgramDetail {
                path: path.into(),
                name: path.rsplit('/').next().unwrap().into(),
                description: "Tool".into(),
            }),
        }),
    }
}

#[test]
fn grouping_keeps_only_complete_matches_and_uses_case_insensitive_program_identity() {
    let config = parse_config(r#"
        [[rules]]
        name = "tool"
        program.name = "TOOL.EXE"
        window.title.starts_with = "Tool"
    "#).unwrap().runtime;
    let mut failed = window(5, "C:/Apps/Tool.exe", "Tool failed");
    failed.detail = Err(anyhow::anyhow!("Client query failed"));
    let sections = group_windows(&config, vec![
        window(1, "C:/Apps/Tool.exe", "Tool one"),
        window(2, "c:/apps/tool.EXE", "Tool two"),
        window(3, "C:/Apps/Tool.exe", "tool lowercase title"),
        window(4, "C:/Other/Tool.exe", "Tool other installation"),
        failed,
    ]);
    assert_eq!(sections.len(), 2);
    assert_eq!(sections[0].rule_name, "tool");
    assert_eq!(sections[0].program_path, "c:/apps/tool.exe");
    assert_eq!(sections[0].windows.len(), 2);
    assert_eq!(sections[0].windows[0].detail.as_ref().unwrap().program.path, "C:/Apps/Tool.exe");
    assert_eq!(sections[1].program_path, "c:/other/tool.exe");
}

#[test]
fn failed_details_never_match_even_unfiltered_rules_and_recovery_uses_new_details() {
    let config = parse_config("[[rules]]\nname = 'all'").unwrap().runtime;
    let mut failed = window(1, "C:/Apps/Tool.exe", "Tool");
    failed.detail = Err(anyhow::anyhow!("Program access denied"));
    assert!(group_windows(&config, vec![failed]).is_empty());
    let sections = group_windows(&config, vec![window(2, "C:/Apps/Other.exe", "Recovered")]);
    assert_eq!(sections[0].program_path, "c:/apps/other.exe");
}

#[test]
fn sections_retain_stable_names_and_source_order_after_rule_reordering() {
    let config = parse_config(r#"
        [[rules]]
        name = "fallback"
        [[rules]]
        name = "specific"
        priority = 10
        window.title = "Specific"
    "#).unwrap().runtime;
    let sections = group_windows(&config, vec![
        window(1, "C:/Apps/Tool.exe", "Specific"),
        window(2, "C:/Apps/Tool.exe", "Fallback"),
    ]);
    assert_eq!(sections.iter().map(|section| section.rule_name.as_str())
        .collect::<Vec<_>>(), ["fallback", "specific"]);

    let reordered = parse_config(r#"
        [[rules]]
        name = "specific"
        priority = 10
        window.title = "Specific"
        [[rules]]
        name = "fallback"
    "#).unwrap().runtime;
    assert_eq!(reordered.rule(&sections[0].rule_name).unwrap().name, "fallback");
}
