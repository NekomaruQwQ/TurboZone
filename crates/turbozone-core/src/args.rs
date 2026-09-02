//! Platform-independent command-line shape without startup filesystem policy.

use std::path::PathBuf;

use clap::Parser;

/// Command-line configuration shared by every platform executable.
///
/// The selected path is explicit so startup never searches platform-specific
/// locations. Resolving, creating, and loading that path belongs to the UI crate.
#[derive(Debug, Parser)]
#[command(version, about = "Rule-driven window positioning and resizing")]
pub struct Args {
    /// Config file to load or create; relative paths use the current working directory.
    #[arg(long, env = "TURBOZONE_CONFIG", value_name = "FILE", hide_env_values = true)]
    pub config: PathBuf,
}
