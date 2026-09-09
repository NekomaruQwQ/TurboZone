//! Exercises snapshot diagnostics through the real engine, in an isolated logger process.

use std::collections::VecDeque;
use std::fmt;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use log::{Level, LevelFilter, Log, Metadata, Record};
use smol_str::{SmolStr, format_smolstr};
use turbozone_core::{Backend, Engine, WindowAction, WindowInfo};

struct CapturedLogs(Mutex<Vec<(Level, SmolStr)>>);

impl Log for CapturedLogs {
    fn enabled(&self, _: &Metadata<'_>) -> bool { true }
    fn log(&self, record: &Record<'_>) {
        self.0.lock().unwrap().push((record.level(), format_smolstr!("{}", record.args())));
    }
    fn flush(&self) {}
}

static LOGS: CapturedLogs = CapturedLogs(Mutex::new(Vec::new()));
const NO_LOGS: [(Level, SmolStr); 0] = [];
static DESCRIPTION: AtomicUsize = AtomicUsize::new(0);

/// Display deliberately differs from Debug and can change without changing identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct TestHandle(u64);

impl fmt::Display for TestHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "native-window-{}-title-{}", self.0, DESCRIPTION.load(Ordering::Relaxed))
    }
}

#[expect(clippy::type_complexity, reason = "scripted observations mirror the backend's direct result contract")]
struct ScriptedBackend(VecDeque<anyhow::Result<Vec<(TestHandle, anyhow::Result<WindowInfo<TestHandle>>)>>>);

impl Backend for ScriptedBackend {
    type Handle = TestHandle;
    fn snapshot(&mut self) -> anyhow::Result<Vec<(TestHandle, anyhow::Result<WindowInfo<TestHandle>>)>> {
        self.0.pop_front().expect("each update needs a scripted observation")
    }
    fn perform(&mut self, _: TestHandle, _: WindowAction) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("action denied").context("native action"))
    }
}

fn failures(entries: &[(u64, &str)]) -> Vec<(TestHandle, anyhow::Result<WindowInfo<TestHandle>>)> {
    entries.iter().map(|&(handle, cause)| {
        (TestHandle(handle), Err(anyhow::anyhow!("{cause}").context("capture")))
    }).collect()
}

fn enumeration(cause: &str) -> anyhow::Result<Vec<(TestHandle, anyhow::Result<WindowInfo<TestHandle>>)>> {
    Err(anyhow::anyhow!("{cause}").context("enumerate"))
}

fn advance(engine: &mut Engine<ScriptedBackend>) -> Vec<(Level, SmolStr)> {
    engine.update();
    std::mem::take(&mut *LOGS.0.lock().unwrap())
}

/// Ordered transitions share one global logger; no other test can consume these records.
#[test]
fn engine_deduplicates_adjacent_snapshots_without_hiding_current_errors() {
    log::set_logger(&LOGS).unwrap();
    log::set_max_level(LevelFilter::Trace);
    let backend = ScriptedBackend([
        Ok(failures(&[(7, "denied")])),
        Ok(failures(&[(7, "denied")])),
        Ok(failures(&[(7, "denied")])),
        Ok(failures(&[(7, "gone")])),
        Ok(failures(&[(7, "gone"), (8, "denied")])),
        Ok(failures(&[(8, "denied"), (7, "gone")])),
        Ok(failures(&[(7, "gone")])),
        Ok(failures(&[(7, "gone"), (8, "denied")])),
        enumeration("outage"),
        enumeration("outage"),
        enumeration("different cause"),
        Ok(failures(&[(8, "denied"), (7, "gone")])),
        Ok(failures(&[])),
        Ok(failures(&[(7, "gone")])),
        Ok(failures(&[])),
        Ok(failures(&[(7, "gone")])),
        Ok(failures(&[])),
        Ok(failures(&[(7, "denied")])),
        Ok(failures(&[(7, "denied")])),
    ].into());
    let mut engine = Engine::new(Vec::new(), backend);
    assert_eq!(advance(&mut engine), [(Level::Warn,
        "failed to snapshot window native-window-7-title-0: capture: denied".into())]);
    assert_eq!(advance(&mut engine), NO_LOGS);
    DESCRIPTION.store(1, Ordering::Relaxed);
    assert_eq!(advance(&mut engine), NO_LOGS, "identity description does not enter the dedup key");
    assert_eq!(advance(&mut engine), [(Level::Warn,
        "failed to snapshot window native-window-7-title-1: capture: gone".into())]);
    assert_eq!(advance(&mut engine), [(Level::Warn,
        "failed to snapshot window native-window-8-title-1: capture: denied".into())]);
    assert_eq!(advance(&mut engine), NO_LOGS, "moving a failure changes neither its identity nor its cause");
    assert_eq!(engine.snapshot().as_ref().unwrap().iter().map(|&(handle, _)| handle.0)
        .collect::<Vec<_>>(), [8, 7]);
    assert_eq!(advance(&mut engine), NO_LOGS);
    assert_eq!(advance(&mut engine).len(), 1, "an absent handle can recur");
    assert_eq!(advance(&mut engine), [(Level::Error,
        "window enumeration failed: enumerate: outage".into())]);
    assert_eq!(advance(&mut engine), NO_LOGS);
    assert_eq!(advance(&mut engine), [(Level::Error,
        "window enumeration failed: enumerate: different cause".into())]);
    assert_eq!(advance(&mut engine), [
        (Level::Warn, "failed to snapshot window native-window-8-title-1: capture: denied".into()),
        (Level::Warn, "failed to snapshot window native-window-7-title-1: capture: gone".into()),
    ], "an outage resets dedup history; new warnings follow enumeration order");
    assert_eq!(advance(&mut engine), NO_LOGS);
    assert_eq!(advance(&mut engine).len(), 1);
    assert_eq!(advance(&mut engine), NO_LOGS);
    assert_eq!(advance(&mut engine).len(), 1);

    engine.queue([(TestHandle(7), WindowAction::Center); 2]);
    let actions = advance(&mut engine);
    assert_eq!(actions.len(), 2, "repeated user action failures are never suppressed");
    assert!(actions.iter().all(|&(level, ref message)| {
        level == Level::Error && message.as_str() ==
            "failed to perform action Center on native-window-7-title-1: native action: action denied"
    }));

    log::set_max_level(LevelFilter::Off);
    assert_eq!(advance(&mut engine), NO_LOGS);
    assert_eq!(engine.snapshot().as_ref().unwrap().iter().filter(|&&(_, ref result)| result.is_err()).count(), 1);
    log::set_max_level(LevelFilter::Trace);
    assert_eq!(advance(&mut engine), NO_LOGS, "disabled output still advances dedup state");
    assert_eq!(engine.view().unwrap().errors.len(), 1);
}
