//! Temporary-directory ownership for startup integration tests.
//!
//! Exclusive creation lets each parallel test clean up only the directory it owns without
//! changing process-global environment or the working directory.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Distinguishes parallel fixtures without changing process environment or working directory.
static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

/// Owns an exclusively created temporary directory and removes only that directory on drop.
pub struct TempDir(PathBuf);

impl TempDir {
    /// Retries stale names left by earlier processes; all other filesystem errors fail the test.
    #[expect(clippy::create_dir, reason = "exclusive creation establishes ownership for cleanup")]
    pub fn new() -> Self {
        loop {
            let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!("turbozone-test-{}-{sequence}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {},
                Err(error) => panic!("failed to create test directory: {error}"),
            }
        }
    }

    /// Borrows the owned directory without changing the current directory.
    pub fn path(&self) -> &Path { &self.0 }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // The path was created exclusively by this fixture, never supplied by a caller.
        let _ = fs::remove_dir_all(&self.0);
    }
}
