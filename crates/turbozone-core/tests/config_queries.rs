use euclid::default::Size2D;
use smol_str::format_smolstr;
use turbozone_core::{Config, ResizeRule, ResizeSelector};

#[test]
fn resize_queries_interpret_every_form_without_changing_the_variant() {
    for (resize, primary, selector) in [
        ("", None, None),
        ("resize = false", None, None),
        ("resize = true", None, Some(ResizeSelector::default())),
        ("resize = {}", None, Some(ResizeSelector::default())),
        ("resize.exact = [1440, 900]", Some(Size2D::new(1440, 900)), None),
        ("resize = [1440, 900]", Some(Size2D::new(1440, 900)), Some(ResizeSelector {
            default: Some(Size2D::new(1440, 900)), ..Default::default()
        })),
        ("resize.min = [960, 540]", None, Some(ResizeSelector {
            min: Some(Size2D::new(960, 540)), ..Default::default()
        })),
        ("resize = { default = [1440, 900], min = [960, 540], max = [1280, 800] }",
            None, Some(ResizeSelector {
                default: Some(Size2D::new(1440, 900)), min: Some(Size2D::new(960, 540)), max: Some(Size2D::new(1280, 800)),
            })),
    ] {
        let source = format_smolstr!("[[rules]]\nname = 'tool'\n{resize}");
        let config: Config = toml::from_str(&source).unwrap();
        let rule = &config.rules[0].resize;
        let authored = serde_json::to_value(rule).unwrap();
        assert_eq!(rule.primary_size(), primary, "{resize}");
        assert_eq!(rule.selector(), selector, "{resize}");
        assert_eq!(serde_json::to_value(rule).unwrap(), authored);
    }
}

/// Every caller gets the same bounded target without losing the authored default.
/// Exercise each axis separately, equality at both limits, and absent bounds.
#[test]
fn primary_targets_respect_inclusive_bounds_without_mutating_config() {
    for (bounds, available) in [
        ("", true),
        ("min = [1280, 720]", true),
        ("max = [1280, 720]", true),
        ("min = [1280, 720], max = [1280, 720]", true),
        ("min = [960, 540], max = [1920, 1080]", true),
        ("min = [1281, 720]", false),
        ("min = [1280, 721]", false),
        ("max = [1279, 720]", false),
        ("max = [1280, 719]", false),
        ("max = [960, 540]", false),
    ] {
        let source = format_smolstr!(
            "[[rules]]\nname = 'app'\nresize = {{ default = [1280, 720], {bounds} }}");
        let config: Config = toml::from_str(&source).unwrap();
        let authored = serde_json::to_value(&config).unwrap();
        let resize = &config.rules[0].resize;
        assert_eq!(resize.primary_size(), available.then_some(Size2D::new(1280, 720)), "{bounds}");
        assert_eq!(resize.selector().unwrap().default, Some(Size2D::new(1280, 720)));
        assert_eq!(serde_json::to_value(&config).unwrap(), authored);
    }
}

#[test]
fn selector_query_results_do_not_mutate_authored_settings() {
    let resize = ResizeRule::Selector(ResizeSelector {
        default: Some(Size2D::new(1440, 900)), min: Some(Size2D::new(960, 540)), max: None,
    });
    let mut selector = resize.selector().unwrap();
    selector.default = None;
    selector.min = None;
    assert_eq!(resize.primary_size(), Some(Size2D::new(1440, 900)));
    assert_eq!(resize.selector().unwrap().min, Some(Size2D::new(960, 540)));
    assert_ne!(resize.selector().unwrap(), selector);
}
