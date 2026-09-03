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

use std::collections::hash_map::{
    HashMap,
    Entry as HashMapEntry,
};

use std::rc::Rc;
use std::ffi::OsString;
use std::path::Path;

use tap::prelude::*;

use anyhow::anyhow;
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

type MonitorCache = HashMap<Handle<HMONITOR>, NativeResult<MONITORINFO>>;
type ProcessCache = HashMap<u32, NativeResult<Rc<Path>>>;
type ProgramCache = HashMap<OsString, (u64, Result<Rc<ProgramDetail>, &'static str>)>;

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
///
/// Monitor results remain call-local so work-area changes are visible to the next snapshot.
/// Program descriptions borrow the backend cache because executable version metadata can be
/// reused safely across snapshots, then [`Self::finish`] prunes paths this enumeration did not
/// observe.
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

impl SnapshotContext<'_> {
    /// Gets the [`MONITORINFO`] for the given monitor, or an error if the query
    /// failed.
    ///
    /// Results are cached for the lifetime of this snapshot and not shared across
    /// snapshots because:
    ///
    /// - Monitor handles may be reused for different monitors between enumerations.
    /// - Monitor work areas may change between enumerations.
    fn get_monitor_info(&mut self, handle: HMONITOR) -> NativeResult<MONITORINFO> {
        self.monitor_cache
            .entry(Handle(handle))
            .or_insert_with(|| native::get_monitor_info(handle))
            .clone()
    }

    /// Gets the program path for the given process, or an error if the query failed.
    ///
    /// Results are cached for the lifetime of this snapshot and not shared across
    /// snapshots because process IDs may be reused for different processes between
    /// enumerations.
    fn get_process_path(&mut self, process_id: u32) -> NativeResult<Rc<Path>> {
        self.process_cache
            .entry(process_id)
            .or_insert_with(|| native::get_program_path(process_id).map(Rc::from))
            .clone()
    }

    /// Gets the [`ProgramDetail`] for the given program path, or an error if the query
    /// failed.
    fn get_program_detail(&mut self, native_path: &Path)
     -> anyhow::Result<Rc<ProgramDetail>> {
        let entry =
            self.program_cache
                .entry(native_path.as_os_str().to_os_string());

        match entry {
            HashMapEntry::Occupied(mut entry) => {
                let &mut (ref mut cache_seq, ref program) = entry.get_mut();
                *cache_seq = self.seq;
                program
                    .as_ref()
                    .map(Rc::clone)
                    .map_err(|&error| anyhow!("failed to get program detail of {:?}: {error}", native_path.display()))
            },
            HashMapEntry::Vacant(entry) => {
                let program =
                    snapshot_program(native_path).map(Rc::new);
                let &mut (_, ref program) =
                    entry.insert((self.seq, program));
                program
                    .as_ref()
                    .map(Rc::clone)
                    .map_err(|&error| anyhow!("failed to get program detail of {:?}: {error}", native_path.display()))
            }
        }
    }
}

/// The Win32 backend for TurboZone.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct Backend {
    next_seq: u64,
    program_cache: ProgramCache,
}

impl CoreBackend for Backend {
    type Handle = Handle<HWND>;

    fn snapshot(&mut self) -> anyhow::Result<Vec<WindowInfo>> {
        self.next_seq += 1;

        let mut ctx = SnapshotContext {
            seq: self.next_seq,
            monitor_cache: HashMap::new(),
            process_cache: HashMap::new(),
            program_cache: &mut self.program_cache,
        };

        let mut windows =
            native::enumerate_windows()?
                .into_iter()
                .filter(|&handle| native::is_app_window(handle))
                .map(|handle| snapshot_window(&mut ctx, handle))
                .collect::<Vec<_>>();

        self.program_cache
            .retain(|_, &mut (entry_seq, _)| {
                entry_seq + TICK_BEFORE_EVICT <= self.next_seq
        });

        windows.retain(|window| !IGNORE_WINDOWS.iter().any(|pred| pred(window)));

        Ok(windows)
    }

    #[expect(clippy::panic_in_result_fn, reason = "an unknown action is a core/backend contract mismatch, not an operational failure")]
    fn perform(&mut self, action: WindowAction) -> anyhow::Result<()> {
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
/// Stops at the first required query failure while treating display metadata as optional.
fn snapshot_window_detail(
    ctx: &mut SnapshotContext,
    handle: HWND,
    window_state: WindowState)
    -> anyhow::Result<WindowDetail> {
    let monitor_handle =
        native::get_monitor(handle);
    let monitor =
        ctx
            .get_monitor_info(monitor_handle)
            .context("failed to get monitor info")?;
    let content_rect =
        get_content_rect(handle, window_state, &monitor)
            .context("failed to get content rect")?;
    let process_id =
        native::get_process_id(handle)
            .context("failed to get process ID")?;
    let program_path =
        ctx
            .get_process_path(process_id)
            .context("failed to get program path")?;
    let program =
        ctx
            .get_program_detail(&program_path)
            .context("failed to get program detail")?;
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
    let name =
        native_path
            .file_name()
            .ok_or("program path has no filename")?
            .to_str()
            .ok_or("program path is not valid UTF-8")?
            .pipe(SmolStr::new);
    let path =
        native_path
            .to_str()
            .ok_or("program path is not valid UTF-8")?
            .replace_smolstr("\\", "/");
    let description =
        win32_version_info::VersionInfo::from_file(native_path)
            .ok()
            .map(|info| info.file_description)
            .unwrap_or_default()
            .pipe(SmolStr::new);
    let description =
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
