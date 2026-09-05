//! Integration coverage for startup configuration loading and terminal diagnostics.
//!
//! The production executable selects a Windows known folder and launches a GUI. Tests call
//! the loader with fixture-owned absolute paths; diagnostic subprocesses run only this test
//! executable, isolating global logger state without reading the user's configuration.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use smol_str::SmolStr;
use turbozone_ui::config::load_config;

#[path = "support/temp_dir.rs"]
mod temp_dir;
use temp_dir::TempDir;

/// Runs just the loader probe, with an explicit path and no production startup code.
fn command(path: &Path) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.args(["--exact", "configuration_diagnostic_probe", "--nocapture"])
        .env("TURBOZONE_TEST_CONFIG", path)
        .env_remove("RUST_LOG")
        .env_remove("RUST_BACKTRACE");
    command
}

/// Captures UTF-8 diagnostics separately from normal CLI output.
fn stderr(output: &Output) -> SmolStr {
    SmolStr::new(std::str::from_utf8(&output.stderr).unwrap())
}

#[test]
fn missing_config_is_created_empty_with_the_remote_schema_directive() {
    let directory = TempDir::new();
    let path = directory.path().join("local.config.toml");
    assert!(load_config(&path).unwrap().is_empty());
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "#:schema https://raw.githubusercontent.com/NekomaruQwQ/TurboZone/refs/heads/main/data/config.schema.json\n\n");
    assert!(!path.with_extension("schema.json").exists());
}

#[test]
fn existing_config_and_unrelated_schema_bytes_are_preserved() {
    let directory = TempDir::new();
    let path = directory.path().join("private.toml");
    for source in [
        "# No schema, intentional formatting\r\n[[rules]]\r\nname = 'app'\r\n",
        "\u{feff}#:schema ./custom.json\r\n# Private notes\r\n[[rules]]\r\nname = 'app'\r\n",
    ] {
        fs::write(&path, source).unwrap();
        fs::write(path.with_extension("schema.json"), "stale schema").unwrap();
        assert_eq!(load_config(&path).unwrap()[0].name, "app");
        assert_eq!(fs::read(&path).unwrap(), source.as_bytes());
        assert_eq!(fs::read_to_string(path.with_extension("schema.json")).unwrap(), "stale schema");
    }
}

#[test]
fn one_invalid_rule_rejects_the_complete_configuration() {
    let directory = TempDir::new();
    let path = directory.path().join("private.toml");
    let source = "[[rules]]\nname = 'broken'\nresize.exact = [0, 900]\n[[rules]]\nname = 'usable'\n";
    fs::write(&path, source).unwrap();
    load_config(&path).unwrap_err();
    assert_eq!(fs::read_to_string(&path).unwrap(), source);
}

#[test]
fn unavailable_or_invalid_config_fails_without_creating_parent_directories() {
    let directory = TempDir::new();
    let missing_parent = directory.path().join("missing");
    load_config(&missing_parent.join("private.toml")).unwrap_err();
    assert!(!missing_parent.exists());
    let path = directory.path().join("private.toml");
    fs::create_dir_all(&path).unwrap();
    assert!(load_config(&path).unwrap_err().to_string().contains("failed to read"));
    fs::remove_dir(&path).unwrap();
    fs::write(&path, [0xff, 0xfe]).unwrap();
    assert!(load_config(&path).unwrap_err().to_string().contains("failed to read"));
}

#[test]
fn loader_rejects_empty_relative_and_directory_only_paths_before_io() {
    for (path, message) in [
        ("", "configuration path must not be empty"),
        ("relative.toml", "configuration path must be absolute"),
        ("C:/", "configuration path must name a file"),
    ] {
        assert_eq!(load_config(Path::new(path)).unwrap_err().to_string(), message);
    }
}

/// Logger initialization and panic hooks are process-global; the parent test invokes
/// this one probe in a fresh process. Ordinary test discovery never loads a config here.
#[test]
fn configuration_diagnostic_probe() {
    let Some(path) = std::env::var_os("TURBOZONE_TEST_CONFIG") else { return; };
    pretty_env_logger::init();
    load_config(Path::new(&path)).expect("failed to load configuration file");
}

#[test]
fn configured_stderr_reports_panic_chains_without_source_excerpts() {
    let directory = TempDir::new();
    let path = directory.path().join("private.toml");
    fs::write(&path, "rules = [ # PRIVATE_SOURCE_SENTINEL").unwrap();
    let output = command(&path).env("RUST_LOG", "warn").output().unwrap();
    assert_eq!(output.status.code(), Some(101));
    let diagnostics = stderr(&output);
    assert!(diagnostics.contains("failed to deserialize configuration"));
    assert!(diagnostics.contains("failed to parse configuration"));
    assert!(!diagnostics.contains("PRIVATE_SOURCE_SENTINEL"));

    let output = command(&path).env("RUST_LOG", "off").output().unwrap();
    assert_eq!(output.status.code(), Some(101));
    let diagnostics = stderr(&output);
    assert!(diagnostics.contains("failed to load configuration"));
    assert!(diagnostics.contains("failed to parse configuration"));
    assert!(!diagnostics.contains("failed to deserialize configuration"));
    assert!(!diagnostics.contains("PRIVATE_SOURCE_SENTINEL"));
}
