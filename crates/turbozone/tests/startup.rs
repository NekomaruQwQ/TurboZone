//! Integration coverage for configuration startup and the composed TurboZone executable.
//!
//! The subprocess cases live with the binary package so Cargo supplies its executable path;
//! direct configuration cases stay here to verify the same startup boundary without a GUI.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use smol_str::SmolStr;
use turbozone_ui::config::load_config;

#[path = "support/temp_dir.rs"]
mod temp_dir;
use temp_dir::TempDir;

/// Isolates CLI environment in a child process; no GUI is launched by these failure cases.
fn command(directory: &TempDir) -> Command {
    command_from(Path::new(env!("CARGO_BIN_EXE_turbozone")), directory.path())
}

/// Creates an isolated command for either Cargo's binary or a relocated executable fixture.
fn command_from(executable: &Path, directory: &Path) -> Command {
    let mut command = Command::new(executable);
    command.current_dir(directory)
        .env_remove("TURBOZONE_CONFIG")
        .env_remove("RUST_LOG");
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
    assert!(load_config(&path).unwrap().rules.is_empty());
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
        assert_eq!(load_config(&path).unwrap().rules[0].name, "app");
        assert_eq!(fs::read(&path).unwrap(), source.as_bytes());
        assert_eq!(fs::read_to_string(path.with_extension("schema.json")).unwrap(), "stale schema");
    }
}

#[test]
fn invalid_rules_do_not_prevent_valid_rules_from_loading() {
    let directory = TempDir::new();
    let path = directory.path().join("private.toml");
    let source = "[[rules]]\nname = 'broken'\nresize.exact = [0, 900]\n[[rules]]\nname = 'usable'\n";
    fs::write(&path, source).unwrap();
    let config = load_config(&path).unwrap();
    assert_eq!(config.rules.len(), 1);
    assert_eq!(config.rules[0].name, "usable");
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
fn cli_requires_a_nonempty_explicit_source_before_io() {
    let directory = TempDir::new();
    for output in [
        command(&directory).output().unwrap(),
        command(&directory).env("TURBOZONE_CONFIG", "").output().unwrap(),
        command(&directory).args(["--config", ""]).output().unwrap(),
        command(&directory).env("TURBOZONE_CONFIG", "environment.toml")
            .args(["--config", ""]).output().unwrap(),
    ] {
        assert_eq!(output.status.code(), Some(2));
        assert!(stderr(&output).contains("--config"));
    }
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn cli_overrides_environment_and_relative_paths_use_the_executable_directory() {
    let directory = TempDir::new();
    let executable_directory = directory.path().join("bin");
    let working_directory = directory.path().join("working");
    fs::create_dir_all(&executable_directory).unwrap();
    fs::create_dir_all(&working_directory).unwrap();
    let executable = executable_directory.join("turbozone.exe");
    fs::copy(env!("CARGO_BIN_EXE_turbozone"), &executable).unwrap();

    for name in ["cli.toml", "env.toml"] {
        fs::write(executable_directory.join(name), "[[rules").unwrap();
        fs::write(working_directory.join(name), "rules = [").unwrap();
    }
    let output = command_from(&executable, &working_directory)
        .env("TURBOZONE_CONFIG", "env.toml")
        .args(["--config", "cli.toml"]).output().unwrap();
    assert_eq!(output.status.code(), Some(101));
    let diagnostics = stderr(&output);
    assert!(diagnostics.contains(executable_directory.join("cli.toml").to_string_lossy().as_ref()));
    assert!(!diagnostics.contains(working_directory.join("cli.toml").to_string_lossy().as_ref()));

    let output = command_from(&executable, &working_directory)
        .env("TURBOZONE_CONFIG", "env.toml").output().unwrap();
    assert_eq!(output.status.code(), Some(101));
    let diagnostics = stderr(&output);
    assert!(diagnostics.contains(executable_directory.join("env.toml").to_string_lossy().as_ref()));
    assert!(!diagnostics.contains(working_directory.join("env.toml").to_string_lossy().as_ref()));
}

#[test]
fn help_hides_private_environment_values_and_performs_no_io() {
    let directory = TempDir::new();
    let output = command(&directory).env("TURBOZONE_CONFIG", "private-location.toml")
        .arg("--help").output().unwrap();
    assert!(output.status.success());
    let help = SmolStr::new(std::str::from_utf8(&output.stdout).unwrap());
    assert!(help.contains("TURBOZONE_CONFIG"));
    assert!(!help.contains("private-location"));
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn configured_stderr_reports_panic_chains_without_source_excerpts() {
    let directory = TempDir::new();
    let path = directory.path().join("private.toml");
    fs::write(&path, "rules = [ # PRIVATE_SOURCE_SENTINEL").unwrap();
    let output = command(&directory).env("RUST_LOG", "warn")
        .arg("--config").arg(&path).output().unwrap();
    assert_eq!(output.status.code(), Some(101));
    assert_eq!(output.stdout, b"");
    let diagnostics = stderr(&output);
    assert!(diagnostics.contains("failed to parse configuration"));
    assert!(!diagnostics.contains("PRIVATE_SOURCE_SENTINEL"));

    let output = command(&directory).env("RUST_LOG", "off")
        .arg("--config").arg(&path).output().unwrap();
    assert_eq!(output.status.code(), Some(101));
    let diagnostics = stderr(&output);
    assert!(diagnostics.contains("failed to load configuration"));
    assert!(diagnostics.contains("failed to parse configuration"));
    assert!(!diagnostics.contains("PRIVATE_SOURCE_SENTINEL"));
}
