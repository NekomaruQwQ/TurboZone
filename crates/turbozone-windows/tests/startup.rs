use std::fs;
use std::process::{Command, Output};

use smol_str::SmolStr;
use turbozone_core::Config;
use turbozone_ui::config::load_config;

#[path = "support/temp_dir.rs"]
mod temp_dir;
use temp_dir::TempDir;

/// Isolates CLI environment in a child process; no GUI is launched by these failure cases.
fn command(directory: &TempDir) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_turbozone"));
    command.current_dir(directory.path())
        .env_remove("TURBOZONE_CONFIG")
        .env_remove("RUST_LOG");
    command
}

/// Captures UTF-8 diagnostics separately from normal CLI output.
fn stderr(output: &Output) -> SmolStr {
    SmolStr::new(std::str::from_utf8(&output.stderr).unwrap())
}

#[test]
fn missing_config_is_created_empty_with_its_current_sibling_schema() {
    let directory = TempDir::new();
    let path = directory.path().join("local.config.toml");
    assert!(load_config(&path).unwrap().rules.is_empty());
    assert_eq!(fs::read_to_string(&path).unwrap(), "#:schema ./local.config.schema.json\n\n");
    let schema: serde_json::Value = serde_json::from_slice(
        &fs::read(path.with_extension("schema.json")).unwrap()).unwrap();
    assert_eq!(schema, serde_json::to_value(Config::schema()).unwrap());
}

#[test]
fn existing_config_bytes_and_schema_directives_are_preserved() {
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
        let schema: serde_json::Value = serde_json::from_slice(
            &fs::read(path.with_extension("schema.json")).unwrap()).unwrap();
        assert_eq!(schema, serde_json::to_value(Config::schema()).unwrap());
    }
}

#[test]
fn schema_write_failure_does_not_prevent_config_creation_or_rule_recovery() {
    let directory = TempDir::new();
    let path = directory.path().join("private.toml");
    fs::create_dir_all(path.with_extension("schema.json")).unwrap();
    assert!(load_config(&path).unwrap().rules.is_empty());
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
fn cli_overrides_environment_and_relative_paths_use_the_working_directory() {
    let directory = TempDir::new();
    for name in ["cli.toml", "env.toml"] {
        fs::write(directory.path().join(name), "[[rules").unwrap();
    }
    let output = command(&directory).env("TURBOZONE_CONFIG", "env.toml")
        .args(["--config", "cli.toml"]).output().unwrap();
    assert_eq!(output.status.code(), Some(101));
    assert!(directory.path().join("cli.schema.json").is_file());
    assert!(!directory.path().join("env.schema.json").exists());
    assert!(stderr(&output).contains("cli.toml"));

    let output = command(&directory).env("TURBOZONE_CONFIG", "env.toml").output().unwrap();
    assert_eq!(output.status.code(), Some(101));
    assert!(directory.path().join("env.schema.json").is_file());
    assert!(stderr(&output).contains("env.toml"));
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
fn configured_stderr_reports_warnings_and_panic_chains_without_source_excerpts() {
    let directory = TempDir::new();
    fs::write(directory.path().join("private.toml"), "rules = [ # PRIVATE_SOURCE_SENTINEL").unwrap();
    fs::create_dir_all(directory.path().join("private.schema.json")).unwrap();
    let output = command(&directory).env("RUST_LOG", "warn")
        .args(["--config", "private.toml"]).output().unwrap();
    assert_eq!(output.status.code(), Some(101));
    assert_eq!(output.stdout, b"");
    let diagnostics = stderr(&output);
    assert!(diagnostics.contains("failed to refresh schema"));
    assert!(diagnostics.contains("failed to parse configuration"));
    assert!(!diagnostics.contains("PRIVATE_SOURCE_SENTINEL"));

    let output = command(&directory).env("RUST_LOG", "off")
        .args(["--config", "private.toml"]).output().unwrap();
    assert_eq!(output.status.code(), Some(101));
    let diagnostics = stderr(&output);
    assert!(diagnostics.contains("failed to load configuration"));
    assert!(diagnostics.contains("failed to parse configuration"));
    assert!(!diagnostics.contains("PRIVATE_SOURCE_SENTINEL"));
}
