//! Explicit generator for the repository's canonical JSON Schema.
//!
//! Keeping generation in an explicitly invoked binary prevents ordinary
//! builds from mutating the checkout while preserving one discoverable,
//! validated developer command.

use std::fs;
use std::path::Path;

use anyhow::Context as _;
use schemars::generate::SchemaSettings;

fn main() {
    let workspace_root =
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..");

    generate_schema_of::<turbozone_core::Config>(
        workspace_root
            .join("data")
            .join("config.schema.json")
            .as_path())
        .expect("failed to generate schema for turbozone_core::Config");
}

fn generate_schema_of<T: schemars::JsonSchema>(path: &Path) -> anyhow::Result<()> {
    let schema =
        SchemaSettings::draft2020_12()
            .for_deserialize()
            .into_generator()
            .into_root_schema_for::<T>();
    let json =
        serde_json::to_string_pretty(&schema)
            .context("serde_json::to_string_pretty failed")?;
    fs::write(path, format!("{json}\n"))
        .context("std::fs::write failed")
}
