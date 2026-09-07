use turbozone_core::util::Cache;
use turbozone_core::{
    Backend      as CoreBackend,
    WindowInfo   as CoreWindowInfo,
    Snapshot,
    WindowAction,
    ProgramInfo,
};

use crate::window::*;
use crate::native;
use crate::native::Convert as _;
use crate::{
    Handle,
    center_window,
    resize_window,
};

use std::rc::Rc;
use std::ffi::OsString;
use std::path::Path;

use anyhow::anyhow;
use anyhow::Result;
use anyhow::Context as _;
use smol_str::SmolStr;
use smol_str::StrExt as _;

use windows::core::Result as NativeResult;

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{HMONITOR, MONITORINFO};

type WindowInfo = CoreWindowInfo<Handle<HWND>>;

/// Failed capture retains only native identity; Display supplies optional diagnostics.
type WindowSnapshot = Result<WindowInfo, (Handle<HWND>, anyhow::Error)>;

/// Type for caching [`MONITORINFO`] for the lifetime of a snapshot.
///
/// This cache is not shared across snapshots because:
/// - Monitor handles may be reused for different monitors between enumerations.
/// - Monitor work areas may change between enumerations.
type MonitorCache = Cache<u64, Handle<HMONITOR>, NativeResult<MONITORINFO>>;

/// Type for caching process paths for the lifetime of a snapshot.
///
/// This cache is not shared across snapshots because process IDs may be reused
/// for different processes between enumerations.
type ProcessCache = Cache<u64, u32, NativeResult<Rc<Path>>>;

/// Type for caching program details across snapshots.
type ProgramCache = Cache<u64, OsString, Result<Rc<ProgramInfo>, &'static str>>;

const TICK_BEFORE_EVICT: u64 = 600;

/// Applies native relevance rules even when required capture failed.
/// Empty titles are excluded independently of capture success. Identifying the shell
/// requires complete program metadata, so a failed Program Manager capture is retained.
fn include_window(empty_title: bool, window: &WindowSnapshot) -> bool {
    !empty_title && !window.as_ref().is_ok_and(|window| {
        window.title == "Program Manager" &&
        window.program.name.eq_ignore_ascii_case("explorer.exe")
    })
}

/// Owns query caches and program-path liveness bookkeeping for one snapshot.
struct SnapshotContext<'a> {
    /// The current snapshot sequence, incremented before each enumeration attempt.
    seq: u64,
    /// Temporary cache of [`MONITORINFO`] results for the lifetime of this snapshot.
    monitor_cache: MonitorCache,
    /// Temporary cache of process paths for the lifetime of this snapshot.
    process_cache: ProcessCache,
    /// Persistent cache of program details, shared with the backend across snapshots.
    program_cache: &'a mut ProgramCache,
}

/// The Win32 backend for TurboZone.
#[derive(Debug, Clone)]
#[derive(Default)]
#[non_exhaustive]
pub struct Backend {
    next_seq: u64,
    program_cache: ProgramCache,
}

impl CoreBackend for Backend {
    type Handle = Handle<HWND>;

    fn snapshot(&mut self) -> Result<Snapshot<Self::Handle>> {
        self.next_seq += 1;

        let mut ctx = SnapshotContext {
            seq: self.next_seq,
            monitor_cache: Cache::new(),
            process_cache: Cache::new(),
            program_cache: &mut self.program_cache,
        };

        let mut snapshot = Snapshot::default();
        for handle in native::enumerate_windows()?
            .into_iter()
            .filter(|&handle| native::is_app_window(handle)) {
            let title = native::get_window_text(handle);
            let empty_title = title.is_empty();
            let window = snapshot_window(&mut ctx, handle, title);
            // Filtering follows capture so ignored windows retain the existing cache
            // refresh behavior. The optional title stays a backend-local concern.
            if !include_window(empty_title, &window) {
                continue;
            }
            match window {
                Ok(window) => snapshot.windows.push(window),
                Err((handle, error)) => { snapshot.errors.insert(handle, error); },
            }
        }

        self.program_cache.evict_before(
            self.next_seq.saturating_sub(TICK_BEFORE_EVICT));

        Ok(snapshot)
    }

    #[expect(clippy::panic_in_result_fn, reason = "an unknown action is a core/backend contract mismatch, not an operational failure")]
    fn perform(&mut self, target: Self::Handle, action: WindowAction) -> Result<()> {
        match action {
            WindowAction::Center =>
                center_window(target)
                    .with_context(|| format!("failed to center window {target}")),
            WindowAction::Resize(size) =>
                resize_window(target, size)
                    .with_context(|| {
                        format!(
                            "failed to resize window {target} to {}x{}",
                            size.width,
                            size.height)
                    }),
            _ => panic!("Windows backend received an unsupported TurboZone action"),
        }
    }
}

/// Associates the first required query failure with its native handle.
/// Display metadata is optional and remains separate from required capture success.
fn snapshot_window(ctx: &mut SnapshotContext, handle: HWND, title: SmolStr)
 -> WindowSnapshot {
    capture_window(ctx, handle, title).map_err(|error| (Handle(handle), error))
}

/// Captures complete geometry and program data in required-query order.
/// Per-snapshot caches share native results; program metadata can outlive this call.
fn capture_window(ctx: &mut SnapshotContext, handle: HWND, title: SmolStr)
 -> Result<WindowInfo> {
    let state = get_window_state(handle);
    let monitor_handle =
        native::get_monitor(handle);
    let monitor =
        ctx
            .monitor_cache
            .get_or_insert(
                0,
                Handle(monitor_handle),
                || native::get_monitor_info(monitor_handle))
            .clone()
            .context("failed to get monitor info")?;
    let content_rect =
        get_content_rect(handle, state, &monitor)
            .context("failed to get content rect")?;
    let process_id =
        native::get_process_id(handle)
            .context("failed to get process ID")?;
    let program_path =
        ctx
            .process_cache
            .get_or_insert(
                0,
                process_id,
                || native::get_program_path(process_id).map(Rc::from))
            .clone()
            .context("failed to get program path")?;
    let program =
        ctx
            .program_cache
            .get_or_insert(
                ctx.seq,
                program_path.as_os_str().to_os_string(),
                || snapshot_program(program_path.as_ref()).map(Rc::new))
            .as_ref()
            .map(Rc::clone)
            .map_err(|error| anyhow!("failed to get program detail of {:?}: {error}", program_path.display()))?;
    Ok(WindowInfo {
        handle: Handle(handle),
        title,
        state,
        monitor_rect: monitor.rcWork.convert(),
        content_rect,
        process_id,
        program,
    })
}

/// Snapshots the [`ProgramInfo`] for a given program.
///
/// The caller retains results in [`SnapshotContext::program_cache`] to avoid
/// repeated metadata queries for the same native path.
///
/// Encountering invalid utf-8 strings in any of the fields is considered
/// an error and will be reported as a failure to obtain the program detail.
///
/// Failures to obtain the program description are not considered fatal, and
/// the program name will be used instead.
fn snapshot_program(native_path: &Path) -> Result<ProgramInfo, &'static str> {
    let name: SmolStr =
        native_path
            .file_name()
            .ok_or("program path has no filename")?
            .to_str()
            .ok_or("program path is not valid UTF-8")?
            .into();
    let path: SmolStr =
        native_path
            .to_str()
            .ok_or("program path is not valid UTF-8")?
            .replace_smolstr("\\", "/");
    let description: SmolStr =
        win32_version_info::VersionInfo::from_file(native_path)
            .ok()
            .map(|info| info.file_description)
            .unwrap_or_default()
            .into();
    let description: SmolStr =
        if !description.is_empty() {
            description
        } else {
            name.clone()
        };
    Ok(ProgramInfo {
        path,
        name,
        description,
    })
}

#[cfg(test)]
#[path = "../tests/include/backend_ext.rs"]
mod test;
