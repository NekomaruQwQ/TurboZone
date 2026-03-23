use std::collections::HashSet;
use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;

use anyhow::Context as _;
use serde::*;
use tap::prelude::*;

/// Persistent application configuration, serialized as TOML.
#[derive(Debug, Clone)]
#[derive(Default)]
#[derive(Serialize, Deserialize)]
pub struct Config {
    /// Executable paths for which window resizing is disabled.
    #[serde(default)]
    pub no_resize: HashSet<PathBuf>,
}

/// Returns the path to `rnr.toml` next to the running executable,
/// or `None` if the executable path cannot be determined.
fn get_config_path() -> Option<PathBuf> {
    env::current_exe()
        .ok()?
        .tap_mut(|path| { path.set_extension("toml"); })
        .pipe(Some)
}

/// Loads configuration from local file. Returns [`None`] if the file does
/// not exist, cannot be read, or cannot be parsed.
pub fn load_config() -> anyhow::Result<Option<Config>> {
    try {
        let path =
            get_config_path()
                .context("failed to determine path to config file")?;
        match fs::read_to_string(&path) {
            Ok(content) => {
                let content =
                    toml::from_str::<Config>(&content)
                        .context("toml::from_str failed")?;
                Some(content)
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound =>
                None,
            Err(e) =>{
                Err(e).context("fs::read_to_string failed")?;
                None
            }
        }
    }.context("failed to load config from file")
}


/// Saves the current configuration to local file.
pub fn save_config(config: &Config) -> anyhow::Result<()> {
    debug_assert!({
        config
            .no_resize
            .iter()
            .all(|path| !path.to_string_lossy().contains('\\'))
    }, "Config::no_resize paths must be normalized to forward slashes before saving");

    try {
        let path =
            get_config_path()
                .context("failed to determine config file path")?;
        let content =
            toml::to_string_pretty(config)
                .context("toml::to_string_pretty failed")?;
        fs::write(&path, content)
            .context("fs::write failed")?;
    }.context("failed to save config to file")
}
