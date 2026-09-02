//! Public-contract tests for deterministic geometry shared by Windows queries and mutations.

use euclid::default::{Point2D, Rect, Size2D, Vector2D};
use turbozone_windows::native::{checked_size_sum, resize_rect};
use turbozone_windows::window::get_restored_content_rect;
use windows::Win32::Foundation::{E_INVALIDARG, RECT};

#[test]
fn resize_rect_preserves_integer_center_across_odd_dimensions() {
    let original = Rect::new(Point2D::new(-1000, 123), Size2D::new(641, 481));
    let resized = resize_rect(original, Size2D::new(800, 600)).unwrap();

    assert_eq!(
        (resized.center(), resized.size),
        (original.center(), Size2D::new(800, 600)));
}

#[test]
fn resize_rect_rejects_nonpositive_dimensions() {
    let original = Rect::new(Point2D::zero(), Size2D::new(100, 100));

    assert_eq!(
        resize_rect(original, Size2D::new(0, 100)).unwrap_err().code(),
        E_INVALIDARG);
}

#[test]
fn resize_rect_rejects_coordinate_overflow() {
    let original = Rect::new(Point2D::new(i32::MAX - 10, 0), Size2D::new(10, 10));

    assert_eq!(
        resize_rect(original, Size2D::new(100, 100)).unwrap_err().code(),
        E_INVALIDARG);
}

#[test]
fn checked_size_sum_adds_frame_overhead() {
    assert_eq!(
        checked_size_sum(Size2D::new(800, 600), Size2D::new(16, 39)).unwrap(),
        Size2D::new(816, 639));
}

#[test]
fn checked_size_sum_rejects_overflow() {
    assert_eq!(
        checked_size_sum(Size2D::new(i32::MAX, 100), Size2D::new(16, 39))
            .unwrap_err()
            .code(),
        E_INVALIDARG);
}

#[test]
fn checked_size_sum_rejects_nonpositive_dimensions() {
    assert_eq!(
        checked_size_sum(Size2D::new(16, 39), Size2D::new(-16, 0))
            .unwrap_err()
            .code(),
        E_INVALIDARG);
}

#[test]
fn restored_content_rect_removes_frame_and_applies_workspace_offset() {
    let outer = RECT { left: 100, top: 200, right: 916, bottom: 839 };
    let frame = RECT { left: -8, top: -31, right: 8, bottom: 8 };
    let offset = Vector2D::new(5, 7);

    assert_eq!(
        get_restored_content_rect(outer, frame, offset).unwrap(),
        Rect::new(Point2D::new(113, 238), Size2D::new(800, 600)));
}

#[test]
fn restored_content_rect_rejects_frame_larger_than_outer_rect() {
    let outer = RECT { left: 0, top: 0, right: 10, bottom: 10 };
    let frame = RECT { left: -8, top: -31, right: 8, bottom: 8 };

    assert_eq!(
        get_restored_content_rect(outer, frame, Vector2D::zero())
            .unwrap_err()
            .code(),
        E_INVALIDARG);
}
