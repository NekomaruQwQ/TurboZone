//! Startup-only filesystem operations for an explicitly supplied absolute config path.
//!
//! The composition root owns path selection. Rejecting relative paths keeps the loader
//! independent of process working directories. Schema generation remains an explicit
//! development task rather than a startup side effect.

use std::fs::{self, OpenOptions};
use std::io::{self, Write as _};
use std::path::Path;

use anyhow::{Context as _, Result};
use smol_str::{SmolStr, format_smolstr};
use turbozone_core::Rule;

const CONFIG_SCHEMA_URL: &str =
    "https://raw.githubusercontent.com/NekomaruQwQ/TurboZone/refs/heads/main/data/config.schema.json";

/// Creates a missing config at the selected absolute path and loads its complete rule set.
///
/// Relative paths are rejected so launch context cannot change configuration identity.
/// Existing config bytes are never modified. Unreadable configs and invalid documents
/// are fatal; the core parser logs validation failures without writing back to the file.
/// Parent directories must exist.
pub fn load_config(path: &Path) -> Result<Vec<Rule>> {
    anyhow::ensure!(!path.is_empty(), "configuration path must not be empty");
    anyhow::ensure!(path.is_absolute(), "configuration path must be absolute");
    anyhow::ensure!(path.file_name().is_some(), "configuration path must name a file");
    log::info!("config_path: {}", path.display());

    let source = read_or_create_config(path)?;
    let config = turbozone_core::parse_config(&source)
        .with_context(|| format_smolstr!("failed to parse configuration: {}", path.display()))?;
    log::info!("loaded {} rules", config.rules.len());
    Ok(config.rules)
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
