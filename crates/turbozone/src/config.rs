//! Explicit config selection and startup-only filesystem operations.

use std::fs::{self, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use clap::Parser;
use turbozone_core::{Config, RuntimeConfig};

/// Command-line configuration; no implicit path is selected or searched.
#[derive(Debug, Parser)]
#[command(version, about = "Rule-driven window positioning and resizing")]
pub struct Args {
    /// Config file to load or create; relative paths use the current working directory.
    #[arg(long, env = "TURBOZONE_CONFIG", value_name = "FILE", hide_env_values = true)]
    pub config: PathBuf,
}

/// Refreshes the schema, creates a missing config, and loads all usable rules.
///
/// Existing config bytes are never modified. Schema-write errors are warnings;
/// unreadable configs and malformed documents are fatal. Rejected rules are logged
/// individually and never written back to the file. Parent directories must exist.
pub fn load_config(path: &Path) -> Result<RuntimeConfig> {
    anyhow::ensure!(!path.as_os_str().is_empty(), "configuration path must not be empty");
    let path = std::path::absolute(path).context("failed to resolve configuration path")?;
    anyhow::ensure!(path.file_name().is_some(), "configuration path must name a file");
    log::info!("configuration: {}", path.display());

    let schema_path = path.with_extension("schema.json");
    if let Err(error) = generate_schema(&schema_path) {
        log::warn!("failed to refresh schema {}: {error:#}", schema_path.display());
    }
    let source = read_or_create_config(&path, &schema_path)?;
    let report = turbozone_core::parse_config(&source)
        .with_context(|| format!("failed to parse configuration: {}", path.display()))?;
    for diagnostic in &report.diagnostics {
        log::warn!("skipping rules[{}]: {}", diagnostic.index, diagnostic.error);
    }
    log::info!("loaded {} rules; skipped {}", report.runtime.rules.len(), report.diagnostics.len());
    Ok(report.runtime)
}

/// Reads existing contents or creates only a schema comment, which is a valid empty config.
/// Exclusive creation protects files that appear after the initial read attempt.
fn read_or_create_config(path: &Path, schema_path: &Path) -> Result<String> {
    match fs::read_to_string(path) {
        Ok(source) => return Ok(source),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {},
        Err(error) => return Err(error).with_context(|| format!("failed to read configuration: {}", path.display())),
    }

    let schema_name = schema_path.file_name().and_then(|name| name.to_str())
        .context("schema filename must be Unicode to create its TOML directive")?;
    let source = format!("#:schema ./{schema_name}\n\n");
    let mut file = match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            return fs::read_to_string(path)
                .with_context(|| format!("failed to read concurrently created configuration: {}", path.display()));
        },
        Err(error) => return Err(error).with_context(|| format!("failed to create configuration: {}", path.display())),
    };
    file.write_all(source.as_bytes())
        .with_context(|| format!("failed to write new configuration: {}", path.display()))?;
    log::info!("created empty configuration: {}", path.display());
    Ok(source)
}

/// Writes the current type-derived schema as UTF-8 JSON with a trailing newline.
/// Serialization finishes before opening the replaceable generated file.
fn generate_schema(path: &Path) -> Result<()> {
    let mut json = serde_json::to_string_pretty(&Config::schema())?;
    json.push('\n');
    fs::write(path, json)?;
    Ok(())
}
