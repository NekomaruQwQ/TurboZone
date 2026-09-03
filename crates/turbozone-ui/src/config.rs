//! Explicit config selection and startup-only filesystem operations.
//!
//! Relative paths follow the installed executable so shortcuts and launchers do not
//! silently redirect configuration through their process working directory. Schema
//! generation remains an explicit development task rather than a startup side effect.

use std::fs::{self, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use smol_str::{SmolStr, format_smolstr};
use turbozone_core::RuntimeConfig;

const CONFIG_SCHEMA_URL: &str =
    "https://raw.githubusercontent.com/NekomaruQwQ/TurboZone/refs/heads/main/data/config.schema.json";

/// Resolves the selected path, creates a missing config, and loads all usable rules.
///
/// Relative paths use the executable's directory so launch context does not change
/// configuration identity. Existing config bytes are never modified. Unreadable configs
/// and malformed documents are fatal; rejected rules are logged individually and never
/// written back to the file. Parent directories must exist.
pub fn load_config(path: &Path) -> Result<RuntimeConfig> {
    anyhow::ensure!(!path.as_os_str().is_empty(), "configuration path must not be empty");
    let path = resolve_config_path(path)?;
    anyhow::ensure!(path.file_name().is_some(), "configuration path must name a file");
    log::info!("configuration: {}", path.display());

    let source = read_or_create_config(&path)?;
    let report = turbozone_core::parse_config(&source)
        .with_context(|| format_smolstr!("failed to parse configuration: {}", path.display()))?;
    for diagnostic in &report.diagnostics {
        log::warn!("skipping rules[{}]: {}", diagnostic.index, diagnostic.error);
    }
    log::info!("loaded {} rules; skipped {}", report.runtime.rules.len(), report.diagnostics.len());
    Ok(report.runtime)
}

/// Resolves relative configuration paths against the executable that owns startup.
fn resolve_config_path(path: &Path) -> Result<PathBuf> {
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        let executable = std::env::current_exe()
            .context("failed to locate executable for relative configuration path")?;
        let directory = executable.parent()
            .context("executable path has no containing directory")?;
        directory.join(path)
    };
    std::path::absolute(path).context("failed to resolve configuration path")
}

/// Reads existing contents or creates only a remote schema directive and blank line.
///
/// Exclusive creation protects files that appear after the initial read attempt. The
/// remote directive keeps editor support independent from runtime-generated sibling files.
fn read_or_create_config(path: &Path) -> Result<SmolStr> {
    match fs::read_to_string(path) {
        Ok(source) => return Ok(source.into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {},
        Err(error) => return Err(error).with_context(|| {
            format_smolstr!("failed to read configuration: {}", path.display())
        }),
    }

    let source = format_smolstr!("#:schema {CONFIG_SCHEMA_URL}\n\n");
    let mut file = match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            return fs::read_to_string(path)
                .map(SmolStr::from)
                .with_context(|| {
                    format_smolstr!(
                        "failed to read concurrently created configuration: {}",
                        path.display())
                });
        },
        Err(error) => return Err(error).with_context(|| {
            format_smolstr!("failed to create configuration: {}", path.display())
        }),
    };
    file.write_all(source.as_bytes())
        .with_context(|| {
            format_smolstr!("failed to write new configuration: {}", path.display())
        })?;
    log::info!("created empty configuration: {}", path.display());
    Ok(source)
}
