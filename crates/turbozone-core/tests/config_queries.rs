use euclid::default::Size2D;
use smol_str::format_smolstr;
use turbozone_core::{Config, Pattern, ResizeRule, ResizeSelector, Rule, parse_config, verify_config};

/// Both successful and failed verification must leave authored values available for
/// inspection. Serialization observes every config field without adding equality derives.
#[test]
fn verification_preserves_all_authored_values_on_success_and_failure() {
    let source = "[[rules]]\nname = 'tool'\ndescription = '  My Tool  '\n\
        program.name = 'TOOL.EXE'\nprogram.path.ends_with = '/Tool.EXE'\n\
        window.title.contains = 'Ready'\nresize = [1440, 900]\n\
        [[rules]]\nname = 'fallback'";
    let mut config: Config = toml::from_str(source).unwrap();
    let authored = serde_json::to_value(&config).unwrap();
    assert!(verify_config(&config).is_some());
    assert_eq!(serde_json::to_value(&config).unwrap(), authored);
    assert_eq!(serde_json::to_value(parse_config(source).unwrap()).unwrap(), authored);

    config.rules[1].resize = ResizeRule::Exact { exact: Size2D::new(0, 900) };
    let invalid = serde_json::to_value(&config).unwrap();
    assert!(verify_config(&config).is_none());
    assert_eq!(serde_json::to_value(&config).unwrap(), invalid);
}

#[test]
fn manually_constructed_configs_receive_the_same_semantic_checks() {
    let mut config = Config { rules: vec![Rule::default()] };
    assert!(verify_config(&config).is_none());
    config.rules[0].name = "tool".into();
    assert!(verify_config(&config).is_some());
    config.rules[0].window.title = Some(Pattern::Partial {
        starts_with: "".into(), ends_with: "".into(), contains: "".into(),
    });
    assert!(verify_config(&config).is_none());
}

#[test]
fn display_names_borrow_trimmed_descriptions_or_the_rule_name() {
    for (description, expected) in [
        ("  My Tool  ", "My Tool"),
        ("", "tool"),
        (" \t\n\u{2003}", "tool"),
        ("\u{2003}Tool\u{2003}", "Tool"),
    ] {
        let rule = Rule { name: "tool".into(), description: description.into(), ..Default::default() };
        assert_eq!(rule.display_name(), expected);
        assert_eq!(rule.description, description);
        let backing = if expected == "tool" { rule.name.as_str() } else { rule.description.trim() };
        assert!(std::ptr::eq(rule.display_name(), backing));
    }
}

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
        let config = parse_config(&source).unwrap();
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
        assert_eq!(verify_config(&config), Some(()));
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
