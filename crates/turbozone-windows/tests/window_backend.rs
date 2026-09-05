//! Public-contract tests for Windows snapshots, actions, diagnostics, and geometry.

use euclid::default::Size2D;
use turbozone_core::{Backend as _, WindowAction, WindowState};
use turbozone_windows::{Backend, Handle, NativeError};
use windows::Win32::Foundation::{E_INVALIDARG, HWND};

#[path = "support/test_window.rs"]
mod test_window;
use test_window::TestWindow;

#[test]
fn snapshot_returns_complete_details_for_an_owned_visible_window() {
    let window = TestWindow::visible_offscreen();
    let expected_content = window.content_rect(WindowState::Normal);
    let expected_monitor = window.monitor_rect();

    let captured = Backend::default().snapshot().unwrap()
        .into_iter()
        .find(|candidate| candidate.handle == window.handle())
        .expect("snapshot must retain the fixture window");
    let detail = captured.detail.unwrap();

    assert_eq!(
        (
            captured.title.as_str(),
            detail.process_id,
            detail.monitor_rect,
            detail.content_rect,
            detail.program.path.is_empty(),
            detail.program.name.is_empty(),
            detail.program.description.is_empty(),
        ),
        (
            window.title(),
            std::process::id(),
            expected_monitor,
            expected_content,
            false,
            false,
            false,
        ));
}

#[test]
fn snapshot_uses_program_name_when_executable_has_no_description() {
    let window = TestWindow::visible_offscreen();
    let detail = Backend::default().snapshot().unwrap()
        .into_iter()
        .find(|candidate| candidate.handle == window.handle())
        .expect("snapshot must retain the fixture window")
        .detail
        .unwrap();

    assert_eq!(detail.program.description, detail.program.name);
}

#[test]
fn resize_action_preserves_integer_center_and_sets_exact_client_size() {
    let window = TestWindow::hidden();
    let before = window.content_rect(WindowState::Normal);
    let target = Size2D::new(641, 481);

    Backend::default().perform(window.handle(), WindowAction::Resize(target)).unwrap();
    let resized = window.content_rect(WindowState::Normal);

    assert_eq!((resized.center(), resized.size), (before.center(), target));
}

#[test]
fn center_action_aligns_client_with_monitor_without_resizing() {
    let window = TestWindow::hidden();
    let before = window.content_rect(WindowState::Normal);

    Backend::default().perform(window.handle(), WindowAction::Center).unwrap();
    let centered = window.content_rect(WindowState::Normal);

    assert_eq!(
        (centered.center(), centered.size),
        (window.monitor_rect().center(), before.size));
}

#[test]
fn oversized_resize_retains_the_native_invalid_argument_error() {
    let window = TestWindow::hidden();
    let error = Backend::default()
        .perform(window.handle(), WindowAction::Resize(Size2D::new(i32::MAX, 100)))
        .unwrap_err();

    assert_eq!(error.downcast_ref::<NativeError>().unwrap().code(), E_INVALIDARG);
}

#[test]
fn nonpositive_resize_retains_the_native_invalid_argument_error() {
    let error = Backend::default()
        .perform(Handle(HWND::default()), WindowAction::Resize(Size2D::new(0, 100)))
        .unwrap_err();

    assert_eq!(error.downcast_ref::<NativeError>().unwrap().code(), E_INVALIDARG);
}

#[test]
fn invalid_handle_action_retains_context_and_native_cause() {
    let error = Backend::default()
        .perform(Handle(HWND::default()), WindowAction::Center)
        .unwrap_err();

    assert!(error.to_string().contains("failed to center window 0x0"));
    assert!(error.downcast_ref::<NativeError>().is_some());
}

#[test]
fn restored_state_query_matches_the_fixture_normal_geometry() {
    let window = TestWindow::hidden();

    assert_eq!(
        window.content_rect(WindowState::Minimized),
        window.content_rect(WindowState::Normal));
}
