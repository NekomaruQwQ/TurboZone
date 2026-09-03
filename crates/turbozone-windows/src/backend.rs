use turbozone_core::util::Cache;
use turbozone_core::{
    Backend      as CoreBackend,
    WindowInfo   as CoreWindowInfo,
    WindowState,
    WindowDetail,
    WindowAction as CoreWindowAction,
    ProgramDetail,
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
use euclid::default::Size2D;
use smol_str::SmolStr;
use smol_str::StrExt as _;

use windows::core::Result as NativeResult;

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{HMONITOR, MONITORINFO};

/// Predicate type for general filtering operations.
type Pred<T> = fn(&T) -> bool;

type WindowInfo   = CoreWindowInfo<Handle<HWND>>;
type WindowAction = CoreWindowAction<Handle<HWND>>;

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
type ProgramCache = Cache<u64, OsString, Result<Rc<ProgramDetail>, &'static str>>;

const TICK_BEFORE_EVICT: u64 = 600;

const IGNORE_WINDOWS: &[Pred<WindowInfo>] = &[
    |window| window.title.is_empty(),
    |window| {
        window.title == "Program Manager" &&
        window.detail
            .as_ref()
            .is_ok_and(|detail| {
                detail
                    .program
                    .name
                    .eq_ignore_ascii_case("explorer.exe")
            })},
];

/// Owns query caches and program-path liveness bookkeeping for one snapshot.
struct SnapshotContext<'a> {
    /// The current snapshot sequence, incremented for each successful enumeration.
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

    fn snapshot(&mut self) -> Result<Vec<WindowInfo>> {
        self.next_seq += 1;

        let mut ctx = SnapshotContext {
            seq: self.next_seq,
            monitor_cache: Cache::new(),
            process_cache: Cache::new(),
            program_cache: &mut self.program_cache,
        };

        let windows =
            native::enumerate_windows()?
                .into_iter()
                .filter(|&handle| native::is_app_window(handle))
                .map(|handle| snapshot_window(&mut ctx, handle))
                .filter(|window| !IGNORE_WINDOWS.iter().any(|pred| pred(window)))
                .collect::<Vec<_>>();

        self.program_cache.evict_before(
            self.next_seq.saturating_sub(TICK_BEFORE_EVICT));

        Ok(windows)
    }

    #[expect(clippy::panic_in_result_fn, reason = "an unknown action is a core/backend contract mismatch, not an operational failure")]
    fn perform(&mut self, action: WindowAction) -> Result<()> {
        match action {
            WindowAction::Resize(handle, size@Size2D { width, height, .. }) =>
                resize_window(handle, size)
                    .with_context(|| format!("failed to resize window {handle} to {width}x{height}")),
            WindowAction::MoveToCenter(handle) =>
                center_window(handle)
                    .with_context(|| format!("failed to center window {handle}")),
            _ => panic!("Windows backend received an unsupported TurboZone action"),
        }
    }
}

/// Snapshots the [`WindowInfo`] for a given window.
fn snapshot_window(ctx: &mut SnapshotContext, handle: HWND)
 -> WindowInfo {
    let title = native::get_window_text(handle);
    let state = get_window_state(handle);
    WindowInfo {
        handle: Handle(handle),
        title,
        state,
        detail: snapshot_window_detail(ctx, handle, state),
    }
}

/// Snapshots the [`WindowDetail`] for a given window.
///
/// Stops at the first required query failure while treating display metadata
/// as optional.
fn snapshot_window_detail(
    ctx: &mut SnapshotContext,
    handle: HWND,
    window_state: WindowState)
 -> Result<WindowDetail> {
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
        get_content_rect(handle, window_state, &monitor)
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
    Ok(WindowDetail {
        monitor_rect: monitor.rcWork.convert(),
        content_rect,
        process_id,
        program,
    })
}

/// Snapshots the [`ProgramDetail`] for a given program.
///
/// This function does not cache the result; use [`SnapshotContext::get_program_detail`]
/// to avoid repeated queries for the same path.
///
/// Encountering invalid utf-8 strings in any of the fields is considered
/// an error and will be reported as a failure to obtain the program detail.
///
/// Failures to obtain the program description are not considered fatal, and
/// the program name will be used instead.
fn snapshot_program(native_path: &Path) -> Result<ProgramDetail, &'static str> {
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
    Ok(ProgramDetail {
        path,
        name,
        description,
    })
}
