//! Deterministic capture and filtering cases independent of desktop enumeration.

use super::BackendTestExt as _;
use crate::{Backend, Handle, NativeError};
use std::rc::Rc;
use turbozone_core::{ProgramInfo, WindowInfo, WindowState};
use windows::Win32::Foundation::HWND;

#[test]
fn invalid_handle_capture_retains_context_and_native_cause() {
    let handle = Handle(HWND::default());
    let error = Backend::default().capture_handle(handle, "Gone").unwrap_err();
    assert!(error.to_string().starts_with("failed to get"));
    assert!(error.downcast_ref::<NativeError>().is_some());
}

#[test]
fn empty_titles_are_ignored_even_when_capture_failed() {
    let failed = Err(anyhow::anyhow!("denied"));
    assert!(!Backend::includes(true, &failed));
    assert!(Backend::includes(false, &failed));
}

/// Synthetic successes vary only shell identity; required native queries are tested above.
#[test]
fn shell_filter_requires_both_the_title_and_successful_executable_identification() {
    for (title, program_name, expected) in [
        ("Program Manager", "explorer.exe", false),
        ("Program Manager", "EXPLORER.EXE", false),
        ("Program Manager", "other.exe", true),
        ("Other", "explorer.exe", true),
        ("", "other.exe", false),
    ] {
        let captured = Ok(WindowInfo {
            handle: Handle(HWND::default()),
            title: title.into(),
            state: WindowState::Normal,
            monitor_rect: euclid::default::Rect::zero(),
            content_rect: euclid::default::Rect::zero(),
            program: Rc::new(ProgramInfo {
                path: program_name.into(),
                name: program_name.into(),
                description: program_name.into(),
            }),
        });
        assert_eq!(Backend::includes(title.is_empty(), &captured), expected);
    }
}
