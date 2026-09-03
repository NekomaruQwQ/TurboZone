use turbozone_core::{
    Backend      as CoreBackend,
    WindowInfo   as CoreWindowInfo,
    WindowState,
    WindowDetail,
    WindowAction as CoreWindowAction,
};

use crate::window::*;
use crate::native;
use crate::native::Convert as _;
use crate::{
    Handle,
    center_window,
    resize_window,
};

use std::collections::{HashMap, HashSet};

use anyhow::Context as _;
use euclid::default::Size2D;
use smol_str::{SmolStr, StrExt as _};

use windows::core::Result;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{HMONITOR, MONITORINFO};

/// Predicate type for general filtering operations.
type Pred<T> = fn(&T) -> bool;

type WindowInfo   = CoreWindowInfo<Handle<HWND>>;
type WindowAction = CoreWindowAction<Handle<HWND>>;

type MonitorInfoCache = HashMap<Handle<HMONITOR>, Result<MONITORINFO>>;

/// Reuses human-facing executable metadata while its program path remains observable.
///
/// Version-resource lookup is optional display work and may fail for valid executables.
/// Caching the filename fallback avoids repeating that failure at the snapshot cadence.
#[derive(Debug, Default)]
struct ProgramDescriptions(HashMap<SmolStr, SmolStr>);

impl ProgramDescriptions {
    /// Resolves one case-insensitive Windows path without repeating its metadata query.
    fn get_or_insert_with(
        &mut self,
        program_path: &str,
        program_name: &SmolStr,
        lookup: impl FnOnce() -> Option<SmolStr>) -> SmolStr {
        self.0
            .entry(program_path.to_lowercase_smolstr())
            .or_insert_with(|| {
                lookup()
                    .filter(|description| !description.is_empty())
                    .unwrap_or_else(|| program_name.clone())
            })
            .clone()
    }

    /// Evicts entries only after successful enumeration establishes the live path set.
    fn retain_observed<'a>(&mut self, program_paths: impl Iterator<Item = &'a str>) {
        let observed = program_paths
            .map(str::to_lowercase_smolstr)
            .collect::<HashSet<_>>();
        self.0.retain(|program_path, _| observed.contains(program_path));
    }
}

const IGNORE_WINDOWS: &[Pred<WindowInfo>] = &[
    |window| window.title.is_empty(),
    |window| {
        window.title == "Program Manager" &&
        window.detail.as_ref().is_ok_and(|detail| {
            detail.program_name.eq_ignore_ascii_case("explorer.exe")
        })},
];

/// Adapts Win32 snapshots and actions while retaining only derived display metadata.
///
/// Program descriptions persist across snapshots to avoid repeated version-resource
/// queries; live-path pruning owns their bounded lifetime independently of core state.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct Backend {
    program_descriptions: ProgramDescriptions,
}

impl CoreBackend for Backend {
    type Handle = Handle<HWND>;

    fn snapshot(&mut self) -> anyhow::Result<Vec<WindowInfo>> {
        let mut monitor_info_cache = HashMap::new();
        let mut windows = native::enumerate_windows()?
            .into_iter()
            .filter(|&handle| native::is_app_window(handle))
            .map(|handle| snapshot_window(
                &mut monitor_info_cache,
                &mut self.program_descriptions,
                handle))
            .collect::<Vec<_>>();
        // Prune before product filtering so an observable ignored window does not
        // repeatedly query the same executable metadata on every snapshot.
        self.program_descriptions.retain_observed(
            windows.iter()
                .filter_map(|window| window.detail.as_ref().ok())
                .map(|detail| detail.program_path.as_str()));
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

/// Retains basic identity even when a detail query fails.
fn snapshot_window(
    monitor_info_cache: &mut MonitorInfoCache,
    program_descriptions: &mut ProgramDescriptions,
    handle: HWND)
 -> WindowInfo {
    let title = native::get_window_text(handle);
    let state = get_window_state(handle);
    WindowInfo {
        handle: Handle(handle),
        title,
        state,
        detail: snapshot_window_detail(
            monitor_info_cache,
            program_descriptions,
            handle,
            state),
    }
}

/// Stops at the first required query failure while treating display metadata as optional.
fn snapshot_window_detail(
    monitor_info_cache: &mut MonitorInfoCache,
    program_descriptions: &mut ProgramDescriptions,
    handle: HWND,
    state: WindowState)
    -> anyhow::Result<WindowDetail> {
    let monitor_handle = native::get_monitor(handle);
    let monitor =
        monitor_info_cache
            .entry(Handle(monitor_handle))
            .or_insert_with(|| native::get_monitor_info(monitor_handle))
            .clone()
            .context("failed to get monitor info")?;
    let content_rect =
        get_content_rect(handle, state, &monitor)
            .context("failed to get content rect")?;
    let process_id =
        native::get_process_id(handle)
            .context("failed to get process ID")?;
    let native_path =
        native::get_program_path(process_id)
            .context("failed to get program path")?;
    let program_name = SmolStr::new(
        native_path
            .file_name()
            .context("program path has no filename")?
            .to_string_lossy());
    let program_path =
        native_path
            .to_string_lossy()
            .replace_smolstr("\\", "/");
    let program_description =
        program_descriptions.get_or_insert_with(
            &program_path,
            &program_name,
            || win32_version_info::VersionInfo::from_file(&native_path)
                .ok()
                .map(|metadata| SmolStr::new(metadata.file_description)));
    Ok(WindowDetail {
        monitor_rect: monitor.rcWork.convert(),
        content_rect,
        process_id,
        program_path,
        program_name,
        program_description,
    })
}

#[cfg(test)]
#[path = "../tests/support/program_description_cache.rs"]
mod program_description_cache_tests;

