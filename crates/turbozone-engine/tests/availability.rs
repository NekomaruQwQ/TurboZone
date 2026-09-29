//! Tests the policy boundary consumed by UI without constructing a native platform.

use std::rc::Rc;
use euclid::default::{Point2D, Rect, Size2D};
use turbozone_core::*;
use turbozone_engine::config_parser::parse_config;
use turbozone_engine::group_windows;

fn window(handle: u64, centered: bool, size: Size2D<i32>) -> WindowInfo<u64> {
    let origin = if centered {
        Point2D::new(960 - size.width / 2, 540 - size.height / 2)
    } else {
        Point2D::zero()
    };
    WindowInfo {
        handle,
        title: "Tool".into(),
        state: WindowState::Normal,
        monitor_rect: Rect::new(Point2D::zero(), Size2D::new(1920, 1080)),
        content_rect: Rect::new(origin, size),
        program: Rc::new(ProgramInfo {
            path: "C:/Tool.exe".into(), name: "Tool.exe".into(), description: "Tool".into(),
        }),
    }
}

#[test]
fn every_resize_form_emits_effective_controls_without_rewriting_the_rule() {
    let target = ResizeTarget { size: Size2D::new(1280, 720), satisfied: false };
    let unbounded = ResizeSelector::default();
    for (source, expected) in [
        ("", ResizeAvailability::Disabled),
        ("resize = false", ResizeAvailability::Disabled),
        ("resize = true", ResizeAvailability::Selector { primary: None, settings: unbounded.clone() }),
        ("resize = {}", ResizeAvailability::Selector { primary: None, settings: unbounded }),
        ("resize = [1280, 720]", ResizeAvailability::Selector {
            primary: Some(target),
            settings: ResizeSelector { default: Some(target.size), min: None, max: None },
        }),
        ("resize.exact = [1280, 720]", ResizeAvailability::Exact(target)),
        ("resize = { default = [1280, 720], min = [960, 540], max = [1920, 1080] }",
            ResizeAvailability::Selector {
                primary: Some(target),
                settings: ResizeSelector {
                    default: Some(target.size),
                    min: Some(Size2D::new(960, 540)), max: Some(Size2D::new(1920, 1080)),
                },
            }),
    ] {
        let rules = parse_config(&format!("[[rules]]\nname = 'tool'\n{source}")).unwrap().rules;
        let authored = rules[0].resize.clone();
        let windows = [window(1, false, Size2D::new(640, 480))];
        let groups = group_windows(&rules, &windows);
        assert_eq!(groups[0].actions.resize, expected, "{source}");
        assert_eq!(groups[0].windows[0].actions.resize, expected, "{source}");
        assert_eq!(rules[0].resize, authored, "availability must preserve authored values");
    }
}

#[test]
fn unavailable_actions_stay_unavailable_even_when_geometry_would_satisfy_them() {
    let rules = parse_config("[[rules]]\nname = 'tool'").unwrap().rules;
    let windows = [window(1, true, Size2D::new(1280, 720))];
    let groups = group_windows(&rules, &windows);
    let expected = ActionAvailability { center: ActionState::Unavailable, resize: ResizeAvailability::Disabled };
    assert_eq!(groups[0].actions, expected);
    assert_eq!(groups[0].windows[0].actions, expected);
}

#[test]
fn group_satisfaction_requires_every_member_without_removing_satisfied_targets() {
    for resize in ["resize.exact = [1280, 720]", "resize = [1280, 720]"] {
        let rules = parse_config(&format!("[[rules]]\nname = 'tool'\nmove = true\n{resize}")).unwrap().rules;
        for all_satisfied in [false, true] {
            let windows = [
                window(7, true, Size2D::new(1280, 720)),
                window(3, all_satisfied, if all_satisfied { Size2D::new(1280, 720) } else { Size2D::new(640, 480) }),
            ];
            let groups = group_windows(&rules, &windows);
            let group = &groups[0];
            assert_eq!(group.actions.center, if all_satisfied { ActionState::Satisfied } else { ActionState::Available });
            let expected = ResizeTarget { size: Size2D::new(1280, 720), satisfied: all_satisfied };
            match group.actions.resize {
                ResizeAvailability::Exact(target) => assert_eq!(target, expected),
                ResizeAvailability::Selector { primary, .. } => assert_eq!(primary, Some(expected)),
                ResizeAvailability::Disabled => panic!("the rule enables resizing"),
            }
            assert_eq!(group.windows[0].actions.center, ActionState::Satisfied);
            assert_eq!(group.windows[1].actions.center, group.actions.center);
            assert_eq!(group.windows.iter().map(|window| window.info.handle).collect::<Vec<_>>(), [7, 3]);
        }
    }
}

#[test]
fn unusable_defaults_leave_selectors_available_even_without_preset_choices() {
    for bounds in ["max = [100, 100]", "min = [1281, 720]", "max = [1280, 719]"] {
        let rules = parse_config(&format!(
            "[[rules]]\nname = 'tool'\nresize = {{ default = [1280, 720], {bounds} }}")).unwrap().rules;
        let windows = [window(1, false, Size2D::new(640, 480))];
        let groups = group_windows(&rules, &windows);
        let ResizeAvailability::Selector { primary, ref settings } = groups[0].actions.resize else {
            panic!("an unusable default must not disable the selector");
        };
        assert_eq!(primary, None);
        assert_eq!(settings.default, Some(Size2D::new(1280, 720)));
        assert!(!settings.allows_size(Size2D::new(1280, 720)));
    }
}

#[test]
fn exact_targets_and_restored_observations_do_not_depend_on_the_ui_preset_catalog() {
    let rules = parse_config("[[rules]]\nname = 'tool'\nmove = true\nresize.exact = [777, 555]").unwrap().rules;
    for state in [WindowState::Normal, WindowState::Minimized, WindowState::Maximized] {
        let mut candidate = window(1, true, Size2D::new(777, 555));
        candidate.state = state;
        let windows = [candidate];
        let groups = group_windows(&rules, &windows);
        assert_eq!(groups[0].actions.center, ActionState::Satisfied);
        assert_eq!(groups[0].actions.resize, ResizeAvailability::Exact(ResizeTarget {
            size: Size2D::new(777, 555), satisfied: true,
        }));
    }
}
