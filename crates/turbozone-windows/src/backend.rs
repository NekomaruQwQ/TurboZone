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

use std::collections::HashMap;

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

const IGNORE_WINDOWS: &[Pred<WindowInfo>] = &[
    |window| window.title.is_empty(),
    |window| {
        window.title == "Program Manager" &&
        window.detail.as_ref().is_ok_and(|detail| {
            detail.program_name.eq_ignore_ascii_case("explorer.exe")
        })},
];

#[derive(Debug, Default)]
#[non_exhaustive]
pub struct Backend;

impl CoreBackend for Backend {
    type Handle = Handle<HWND>;

    fn snapshot(&mut self) -> anyhow::Result<Vec<WindowInfo>> {
        let mut monitor_info_cache = HashMap::new();

        Ok(native::enumerate_windows()?
            .into_iter()
            .filter(|&handle| native::is_app_window(handle))
            .map(|handle| snapshot_window(&mut monitor_info_cache, handle))
            .filter(|window| !IGNORE_WINDOWS.iter().any(|pred| pred(window)))
            .collect())
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
    handle: HWND) -> WindowInfo {
    let state = get_window_state(handle);
    WindowInfo {
        handle: Handle(handle),
        title: native::get_window_text(handle),
        state,
        detail: snapshot_window_detail(monitor_info_cache, handle, state),
    }
}

/// Stops at the first failed query while retaining the original native error.
fn snapshot_window_detail(
    monitor_info_cache: &mut MonitorInfoCache,
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
    // Windows supplies normalized paths; only the separator convention changes.
    let program_path =
        native_path
            .to_string_lossy()
            .replace_smolstr("\\", "/");
    Ok(WindowDetail {
        monitor_rect: monitor.rcWork.convert(),
        content_rect,
        process_id,
        program_path,
        program_name,
    })
}

