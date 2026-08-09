//! Win32-originated executable path handling.

use std::path::Path;

/// Lexically normalizes a native path and renders it with forward slashes.
///
/// This function intentionally performs no filesystem access. Configuration
/// strings must not be passed through it; they are validated and consumed as written.
pub fn normalize_native_path(path: &Path) -> String {
    path
        .normalize_lexically()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_native_path_resolves_dot_components_and_replaces_separators() {
        let path = Path::new(r"C:\Apps\.\Edge\..\Browser\app.exe");

        assert_eq!(normalize_native_path(path), "C:/Apps/Browser/app.exe");
    }
}
