use std::sync::Mutex;

use log::{LevelFilter, Log, Metadata, Record};
use turbozone::data::group_windows;
use turbozone::diagnostics::SnapshotDiagnostics;
use turbozone_core::parse_config;

#[path = "support/window.rs"]
mod fixture;
use fixture::window;

#[test]
fn grouping_keeps_only_complete_matches_and_uses_case_insensitive_program_identity() {
    let config = parse_config(r#"
        [[rules]]
        name = "tool"
        program.name = "TOOL.EXE"
        window.title.starts_with = "Tool"
    "#).unwrap().runtime;
    let mut failed = window("C:/Apps/Tool.exe", "Tool failed");
    failed.detail = Err(anyhow::anyhow!("Client query failed"));
    let sections = group_windows(&config, vec![
        window("C:/Apps/Tool.exe", "Tool one"),
        window("c:/apps/tool.EXE", "Tool two"),
        window("C:/Apps/Tool.exe", "tool lowercase title"),
        window("C:/Other/Tool.exe", "Tool other installation"),
        failed,
    ]);
    assert_eq!(sections.len(), 2);
    assert_eq!(sections[0].program_path, "c:/apps/tool.exe");
    assert_eq!(sections[0].windows.len(), 2);
    assert_eq!(sections[0].windows[0].detail.as_ref().unwrap().program_path, "C:/Apps/Tool.exe");
    assert_eq!(sections[1].program_path, "c:/other/tool.exe");
}

#[test]
fn failed_details_never_match_even_unfiltered_rules_and_recovery_uses_new_details() {
    let config = parse_config("[[rules]]\nname = 'all'").unwrap().runtime;
    let mut failed = window("C:/Apps/Tool.exe", "Tool");
    failed.detail = Err(anyhow::anyhow!("Program access denied"));
    assert!(group_windows(&config, vec![failed]).is_empty());
    let sections = group_windows(&config, vec![window("C:/Apps/Other.exe", "Recovered")]);
    assert_eq!(sections[0].program_path, "c:/apps/other.exe");
}

#[test]
fn sections_keep_valid_source_order_and_priority_winners_after_recovery() {
    let config = parse_config(r#"
        [[rules]]
        name = "broken"
        window.title = {}
        [[rules]]
        name = "fallback"
        [[rules]]
        name = "specific"
        priority = 10
        window.title = "Specific"
        [[rules]]
        name = "later-tie"
        priority = 10
        window.title = "Specific"
    "#).unwrap().runtime;
    let sections = group_windows(&config, vec![
        window("C:/Apps/Tool.exe", "Specific"),
        window("C:/Apps/Tool.exe", "Fallback"),
    ]);
    assert_eq!(sections.iter().map(|section| config.rules[section.rule_index].name.as_str())
        .collect::<Vec<_>>(), ["fallback", "specific"]);
}

/// Captures logging in this test executable, without installing a terminal logger.
struct CapturedLogs(Mutex<Vec<String>>);

impl Log for CapturedLogs {
    fn enabled(&self, _: &Metadata<'_>) -> bool { true }
    fn log(&self, record: &Record<'_>) { self.0.lock().unwrap().push(record.args().to_string()); }
    fn flush(&self) {}
}

/// A logger must remain alive for the entire integration-test process.
static LOGS: CapturedLogs = CapturedLogs(Mutex::new(Vec::new()));

/// Drains each transition so an unchanged error is observable as no new output.
fn take_logs() -> Vec<String> { std::mem::take(&mut *LOGS.0.lock().unwrap()) }

#[test]
fn snapshot_diagnostics_log_changes_and_recurrence_without_periodic_spam() {
    log::set_logger(&LOGS).unwrap();
    log::set_max_level(LevelFilter::Trace);
    let mut diagnostics = SnapshotDiagnostics::default();
    let mut snapshot = window("C:/Private/Tool.exe", "PRIVATE_WINDOW_TITLE");
    snapshot.detail = Err(anyhow::anyhow!("access denied").context("Program path"));
    diagnostics.update(std::slice::from_ref(&snapshot));
    let messages = take_logs();
    assert_eq!(messages, ["window 0x0: Program path: access denied"]);
    diagnostics.update(std::slice::from_ref(&snapshot));
    assert_eq!(take_logs(), Vec::<String>::new());

    snapshot.detail = Err(anyhow::anyhow!("window disappeared").context("Client geometry"));
    diagnostics.update(std::slice::from_ref(&snapshot));
    assert_eq!(take_logs(), ["window 0x0: Client geometry: window disappeared"]);
    diagnostics.enumeration_failed("enumeration unavailable".to_owned());
    assert_eq!(take_logs(), ["window enumeration failed: enumeration unavailable"]);
    diagnostics.enumeration_failed("enumeration unavailable".to_owned());
    assert_eq!(take_logs(), Vec::<String>::new());
    diagnostics.update(std::slice::from_ref(&snapshot));
    assert_eq!(take_logs(), Vec::<String>::new());
    diagnostics.enumeration_failed("enumeration unavailable".to_owned());
    assert_eq!(take_logs().len(), 1);

    diagnostics.update(&[window("C:/Private/Tool.exe", "Recovered")]);
    assert_eq!(take_logs(), Vec::<String>::new());
    diagnostics.update(std::slice::from_ref(&snapshot));
    assert_eq!(take_logs().len(), 1);
    diagnostics.update(&[]);
    diagnostics.update(std::slice::from_ref(&snapshot));
    assert_eq!(take_logs().len(), 1);
}
