//! Verifies the recoverable resize-default diagnostic independently of global loggers
//! installed by other integration-test executables. Repeated UI queries must stay silent.

use std::sync::Mutex;

use euclid::default::Size2D;
use log::{Level, LevelFilter, Log, Metadata, Record};
use smol_str::{SmolStr, format_smolstr};
use turbozone_core::{parse_config, verify_config};

struct CapturedLogs(Mutex<Vec<(Level, SmolStr)>>);

impl Log for CapturedLogs {
    fn enabled(&self, _: &Metadata<'_>) -> bool { true }
    fn log(&self, record: &Record<'_>) {
        self.0.lock().unwrap().push((record.level(), format_smolstr!("{}", record.args())));
    }
    fn flush(&self) {}
}

static LOGS: CapturedLogs = CapturedLogs(Mutex::new(Vec::new()));

fn take_logs() -> Vec<(Level, SmolStr)> { std::mem::take(&mut *LOGS.0.lock().unwrap()) }

/// Well-formed defaults outside either bound recover, while malformed sizes and bounds
/// still reject the whole document. Only explicit verification owns warning emission.
#[test]
fn verification_warns_once_per_unusable_default_and_preserves_other_rules() {
    log::set_logger(&LOGS).unwrap();
    log::set_max_level(LevelFilter::Trace);

    for bounds in [
        "min = [1281, 720]",
        "min = [1280, 721]",
        "max = [1279, 720]",
        "max = [1280, 719]",
        "max = [960, 540]",
    ] {
        let source = format_smolstr!(
            "# PRIVATE_SOURCE_SENTINEL\n[[rules]]\nname = 'app'\n\
             resize = {{ default = [1280, 720], {bounds} }}\n\
             [[rules]]\nname = 'other'\nresize.exact = [640, 480]");
        let config = parse_config(&source).unwrap();
        let warnings = take_logs();
        assert_eq!(warnings.len(), 1, "{bounds}: {warnings:?}");
        assert_eq!(warnings[0].0, Level::Warn);
        for expected in ["rules[0].resize.default", "[1280, 720]", "rule 'app'", "ignoring default"] {
            assert!(warnings[0].1.contains(expected), "missing {expected}: {warnings:?}");
        }
        assert!(!warnings[0].1.contains("PRIVATE_SOURCE_SENTINEL"));
        assert_eq!(config.rules.len(), 2);
        assert_eq!(config.rules[1].resize.primary_size().unwrap().width, 640);

        let authored = serde_json::to_value(&config).unwrap();
        for _ in 0..3 {
            assert_eq!(config.rules[0].resize.primary_size(), None);
            assert_eq!(config.rules[0].resize.selector().unwrap().default, Some(Size2D::new(1280, 720)));
        }
        assert_eq!(take_logs(), []);
        assert_eq!(verify_config(&config), Some(()));
        assert_eq!(take_logs(), warnings);
        assert_eq!(serde_json::to_value(&config).unwrap(), authored);
    }

    for resize in [
        "resize = true",
        "resize = [1280, 720]",
        "resize.exact = [1280, 720]",
        "resize = { min = [960, 540], max = [1280, 720] }",
        "resize = { default = [1280, 720], min = [1280, 720], max = [1280, 720] }",
    ] {
        let source = format_smolstr!("[[rules]]\nname = 'app'\n{resize}");
        assert!(parse_config(&source).is_some());
        assert_eq!(take_logs(), []);
    }

    for resize in [
        "resize = { default = [0, 720], max = [960, 540] }",
        "resize = { default = [16385, 720], max = [960, 540] }",
        "resize = { default = [1280, 720], min = [1280, 720], max = [960, 540] }",
    ] {
        let source = format_smolstr!("[[rules]]\nname = 'app'\n{resize}");
        assert!(parse_config(&source).is_none());
        let errors = take_logs();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].0, Level::Error);
    }
}
