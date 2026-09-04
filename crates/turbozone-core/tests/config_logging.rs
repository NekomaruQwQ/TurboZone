use std::sync::Mutex;

use log::{LevelFilter, Log, Metadata, Record};
use smol_str::{SmolStr, format_smolstr};
use turbozone_core::parse_config;

/// Captures the parser's sole diagnostic channel without installing a terminal logger.
struct CapturedLogs(Mutex<Vec<SmolStr>>);

impl Log for CapturedLogs {
    fn enabled(&self, _: &Metadata<'_>) -> bool { true }
    fn log(&self, record: &Record<'_>) {
        self.0.lock().unwrap().push(format_smolstr!("{}", record.args()));
    }
    fn flush(&self) {}
}

/// A logger must remain alive for the entire integration-test process.
static LOGS: CapturedLogs = CapturedLogs(Mutex::new(Vec::new()));

/// Drains each parse attempt so its one failure is independently observable.
fn take_logs() -> Vec<SmolStr> { std::mem::take(&mut *LOGS.0.lock().unwrap()) }

#[test]
fn parser_logs_one_private_safe_error_at_the_failing_stage() {
    log::set_logger(&LOGS).unwrap();
    log::set_max_level(LevelFilter::Trace);

    assert!(parse_config("# Header\nrules = [ # PRIVATE_SOURCE_SENTINEL").is_none());
    let logs = take_logs();
    assert_eq!(logs.len(), 1);
    assert!(logs[0].contains("failed to deserialize configuration"));
    assert!(!logs[0].contains("PRIVATE_SOURCE_SENTINEL"));

    assert!(parse_config(
        r#"[[rules]]
name = "app"
program.path = 'C:\Tool.exe'"#).is_none());
    assert_eq!(
        take_logs(),
        ["rules[0].program.path must use forward slashes; backslashes are not accepted"]);

    assert!(parse_config(
        r#"[[rules]]
name = "semantic-error"
resize.exact = [0, 900]
[[rules]]
name = "structural-error"
move = "yes""#).is_none());
    let logs = take_logs();
    assert_eq!(logs.len(), 1);
    assert!(logs[0].contains("failed to deserialize configuration"));
    assert!(!logs[0].contains("must be between"));
}
