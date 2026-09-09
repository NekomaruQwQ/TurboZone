//! Accesses single-window capture and native filtering without enumerating user windows.

use super::*;

trait BackendTestExt {
    fn capture_handle(&mut self, handle: Handle<HWND>, title: &str) -> Result<WindowInfo>;
    fn includes(empty_title: bool, window: &Result<WindowInfo>) -> bool;
}

impl BackendTestExt for Backend {
    fn capture_handle(&mut self, handle: Handle<HWND>, title: &str) -> Result<WindowInfo> {
        let mut ctx = SnapshotContext {
            seq: self.next_seq,
            monitor_cache: Cache::new(),
            process_cache: Cache::new(),
            program_cache: &mut self.program_cache,
        };
        capture_window(&mut ctx, handle.0, title.into())
    }

    fn includes(empty_title: bool, window: &Result<WindowInfo>) -> bool {
        include_window(empty_title, window)
    }
}

#[path = "../unit/backend.rs"]
mod cases;
