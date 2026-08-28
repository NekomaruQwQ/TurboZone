use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;

use anyhow::{Context as _, Result};
use turbozone_core::{Config, RuntimeConfig};

/// Active validated configuration plus any load diagnostic shown by the UI.
pub struct ConfigState {
    /// Discovered TurboRnR.config.toml path, when program discovery succeeded.
    pub path: Option<PathBuf>,
    /// Validated runtime state, or an empty fallback after a load failure.
    pub runtime: RuntimeConfig,
    /// Human-readable load or validation failure.
    pub error: Option<String>,
}

impl ConfigState {
    /// Loads the active configuration without allowing a bad file to crash the UI.
    pub fn load() -> Self {
        match load_config() {
            Ok((path, runtime)) => Self {
                path: Some(path),
                runtime,
                error: None,
            },
            Err(error) => {
                let path = discover_config_path().ok();
                log::error!("{error:#}");
                Self {
                    path,
                    runtime: RuntimeConfig::default(),
                    error: Some(format!("{error:#}")),
                }
            },
        }
    }
}

fn discover_config_path() -> Result<PathBuf> {
    let program = env::current_exe().context("failed to locate TurboRnR program")?;
    let program_name = program.file_stem()
        .context("TurboRnR program has no filename")?
        .to_string_lossy();
    Ok(program.with_file_name(format!("{program_name}.config.toml")))
}

fn load_config() -> Result<(PathBuf, RuntimeConfig)> {
    let path = discover_config_path()?;
    let source = match fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(error).with_context(|| {
                format!("configuration file not found: {}", path.display())
            });
        },
        Err(error) => {
            return Err(error).with_context(|| {
                format!("failed to read configuration: {}", path.display())
            });
        },
    };
    let config = toml::from_str::<Config>(&source)
        .with_context(|| format!("failed to parse configuration: {}", path.display()))?;
    let runtime = config.validate()
        .with_context(|| format!("invalid configuration: {}", path.display()))?;
    Ok((path, runtime))
}

use std::path::Path;

/// Writes UTF-8 JSON to the repository's documented schema location.
///
/// Serializes before opening the destination so generation errors leave it intact.
/// Returns serialization or filesystem errors to the command's caller.
pub fn generate_schema(path: &Path) -> anyhow::Result<()> {
    let schema = Config::schema();
    let mut json = serde_json::to_string_pretty(&schema)?;
    json.push('\n');
    fs::write(path, json)?;
    println!("Generated {}", path.display());
    Ok(())
}
