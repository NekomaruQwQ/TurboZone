use std::rc::Rc;
use std::sync::Mutex;

use euclid::default::{Point2D, Rect, Size2D};
use log::{LevelFilter, Log, Metadata, Record};
use smol_str::{SmolStr, format_smolstr};
use turbozone_core::{ProgramDetail, SnapshotLogging, WindowDetail, WindowInfo, WindowState};

/// Captures logging transitions without installing a terminal logger.
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

fn window(title: &str) -> WindowInfo<u64> {
    WindowInfo {
        handle: 7,
        title: title.into(),
        state: WindowState::Normal,
        detail: Ok(WindowDetail {
            monitor_rect: Rect::new(Point2D::zero(), Size2D::new(1920, 1080)),
            content_rect: Rect::new(Point2D::zero(), Size2D::new(640, 480)),
            process_id: 42,
            program: Rc::new(ProgramDetail {
                path: "C:/Private/Tool.exe".into(),
                name: "tool.exe".into(),
                description: "Tool".into(),
            }),
        }),
    }
}

/// Drains each transition so unchanged failures are observable as no new output.
fn take_logs() -> Vec<SmolStr> { std::mem::take(&mut *LOGS.0.lock().unwrap()) }

#[test]
fn snapshot_logging_reports_changes_recovery_and_recurrence_without_periodic_spam() {
    log::set_logger(&LOGS).unwrap();
    log::set_max_level(LevelFilter::Trace);
    let mut logging = SnapshotLogging::default();
    let mut snapshot = window("Private Window");
    snapshot.detail = Err(anyhow::anyhow!("access denied").context("Program path"));
    logging.update(std::slice::from_ref(&snapshot));
    assert_eq!(take_logs(), ["window 7 title=\"Private Window\": Program path: access denied"]);

    logging.update(std::slice::from_ref(&snapshot));
    assert_eq!(take_logs(), Vec::<SmolStr>::new());

    snapshot.detail = Err(anyhow::anyhow!("window disappeared").context("Client geometry"));
    logging.update(std::slice::from_ref(&snapshot));
    assert_eq!(take_logs(), ["window 7 title=\"Private Window\": Client geometry: window disappeared"]);

    logging.enumeration_failed("enumeration unavailable");
    assert_eq!(take_logs(), ["window enumeration failed: enumeration unavailable"]);
    logging.enumeration_failed("enumeration unavailable");
    assert_eq!(take_logs(), Vec::<SmolStr>::new());

    logging.update(&[window("Recovered")]);
    logging.update(std::slice::from_ref(&snapshot));
    assert_eq!(take_logs().len(), 1);
}
