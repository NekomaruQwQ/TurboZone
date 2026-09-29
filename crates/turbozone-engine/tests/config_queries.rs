use turbozone_engine::config_parser::{parse_config, verify_config};
use turbozone_core::{Config, Pattern, ResizeRule, Rule};
use euclid::default::Size2D;

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
fn loading_and_matching_preserve_authored_program_patterns() {
    let config = parse_config("[[rules]]\nname = 'tool'\nprogram.name = 'TOOL.EXE'").unwrap();
    let pattern = config.rules[0].program.name.as_ref().unwrap();
    assert!(pattern.matches_ignore_case("tool.exe"));
    assert_eq!(pattern, &Pattern::Exact("TOOL.EXE".into()));
}
