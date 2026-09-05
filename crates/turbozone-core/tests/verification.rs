use std::rc::Rc;

use euclid::default::{Point2D, Rect, Size2D};
use smol_str::SmolStr;
use turbozone_core::*;

/// Deserializes fixtures that exercise semantic verification separately from document parsing.
fn parse(source: &str) -> Config {
    toml::from_str(source).expect("test configuration must deserialize")
}

/// Builds the complete aggregate snapshot consumed by engine matching.
fn window(program_name: &str, program_path: &str, title: &str) -> WindowInfo<()> {
    window_with_size(program_name, program_path, title, Size2D::new(640, 480))
}

/// Varies client geometry without bypassing the snapshot boundary under test.
fn window_with_size(
    program_name: &str,
    program_path: &str,
    title: &str,
    size: Size2D<i32>) -> WindowInfo<()> {
    WindowInfo {
        handle: (),
        title: title.into(),
        state: WindowState::Normal,
        detail: Ok(WindowDetail {
            monitor_rect: Rect::new(Point2D::zero(), Size2D::new(1920, 1080)),
            content_rect: Rect::new(Point2D::zero(), size),
            process_id: 42,
            program: Rc::new(ProgramInfo {
                path: program_path.into(),
                name: program_name.into(),
                description: program_name.into(),
            }),
        }),
    }
}

#[test]
fn rule_name_accepts_lowercase_dotted_bare_keys() {
    assert!(is_valid_rule_name("vscode.main-project_2"));
}

#[test]
fn rule_name_rejects_characters_and_empty_dotted_components_outside_its_grammar() {
    for name in [
        "",
        ".app",
        "app.",
        "app..main",
        "App",
        "app name",
        "app/main",
    ] {
        assert!(!is_valid_rule_name(name), "'{name}' must be rejected");
    }
}

#[test]
fn validate_rejects_duplicate_rule_names() {
    let config = parse(
        r#"
        [[rules]]
        name = "same"

        [[rules]]
        name = "same"
    "#,
    );

    assert!(validate(&config).is_none(), "duplicate rule name must fail");
}

#[test]
fn explicit_action_fields_enable_controls() {
    let rules = validate(&parse(
        r#"
        [[rules]]
        name = "app"
        move = true
        resize = true
    "#,
    ))
    .expect("explicit action fields must validate");
    let rule = &rules[0];

    assert_eq!(
        (rule.relocate, rule.resize.selector().is_some()),
        (true, true)
    );
}

#[test]
fn omitted_action_fields_disable_controls() {
    let rules = validate(&parse(
        r#"
        [[rules]]
        name = "app"
    "#,
    ))
    .expect("omitted action fields must use defaults");
    let rule = &rules[0];

    assert_eq!(
        (
            rule.relocate,
            rule.resize.primary_size(),
            rule.resize.selector().as_ref()
        ),
        (false, None, None)
    );
}

#[test]
fn exact_resize_disables_selector() {
    let rules = validate(&parse(
        r#"
        [[rules]]
        name = "app"
        resize.exact = [1440, 900]
    "#,
    ))
    .expect("resize target must validate");

    assert_eq!(
        (
            rules[0].resize.primary_size(),
            rules[0].resize.selector().as_ref()
        ),
        (Some(Size2D::new(1440, 900)), None)
    );
}

#[test]
fn selector_default_is_independent_of_selector_bounds() {
    let rules = validate(&parse(
        r#"
        [[rules]]
        name = "app"
        resize.default = [1440, 900]
        resize.max = [1280, 800]
    "#,
    ))
    .expect("default target need not be in selector bounds");

    assert_eq!(
        rules[0]
            .resize.selector()
            .as_ref()
            .and_then(|selector| selector.default),
        Some([1440, 900])
    );
}

#[test]
fn selector_default_shorthand_enables_an_unbounded_selector() {
    let rules = validate(&parse("[[rules]]\nname = 'app'\nresize = [1440, 900]")).unwrap();

    assert_eq!(
        rules[0].resize.selector(),
        Some(ResizeSelector {
            default: Some([1440, 900]),
            min: None,
            max: None,
        })
    );
}

#[test]
fn empty_trimmed_description_uses_rule_name_fallback() {
    let rules = validate(&parse(
        r#"
        [[rules]]
        name = "app"
        description = "   "
    "#,
    ))
    .expect("empty trimmed description must remain valid");

    assert_eq!(rules[0].display_name(), "app");
    assert_eq!(rules[0].description, "   ");
}

/// Selector bounds apply independently to both axes and include their endpoints.
#[test]
fn selector_limits_check_both_axes_and_reject_nonpositive_sizes() {
    let rules = validate(&parse(
        r#"
        [[rules]]
        name = "app"
        resize.min = [1400, 500]
        resize.max = [3840, 1000]
    "#,
    ))
    .expect("selector limits must validate");
    let resize = rules[0]
        .resize.selector()
        .expect("selector must be enabled");

    for (size, allowed) in [
        ([1440, 900], true),
        ([1400, 500], true),
        ([3840, 1000], true),
        ([1399, 500], false),
        ([1400, 499], false),
        ([3841, 1000], false),
        ([3840, 1001], false),
        ([0, 900], false),
        ([1440, -1], false),
    ] {
        assert_eq!(
            resize.allows_size(Size2D::from(size)),
            allowed,
            "size {size:?}"
        );
    }
    assert!(!ResizeSelector::default().allows_size(Size2D::new(0, 900)));
    assert!(!ResizeSelector::default().allows_size(Size2D::new(1440, -1)));
}

#[test]
fn partial_matcher_ands_every_predicate() {
    let rules = validate(&parse(
        r#"
        [[rules]]
        name = "tool"
        window.title.starts_with = "Tool"
        window.title.ends_with = "Ready"
        window.title.contains = " - "
    "#,
    ))
    .expect("partial matcher must validate");
    let rule = &rules[0];

    assert!(matches_rule(rule, &window("tool.exe", "c:/tool.exe", "Tool - Ready")));
}

#[test]
fn partial_matcher_ignores_empty_components() {
    let rules = validate(&parse(
        r#"
        [[rules]]
        name = "tool"
        window.title.starts_with = ""
        window.title.ends_with = "Ready"
    "#,
    ))
    .expect("empty partial components must behave as omitted");

    assert!(matches_rule(
        &rules[0],
        &window("tool.exe", "c:/tool.exe", "Tool Ready")));
}

#[test]
fn string_matcher_is_exact() {
    let rules = validate(&parse(
        r#"
        [[rules]]
        name = "tool"
        window.title = "Tool"
    "#,
    ))
    .expect("bare matcher must validate");

    assert!(!matches_rule(
        &rules[0],
        &window("tool.exe", "c:/tool.exe", "Tool Window")));
}

#[test]
fn explicit_exact_matcher_is_rejected_by_deserialization() {
    let result = toml::from_str::<Config>(
        r#"
        [[rules]]
        name = "tool"
        window.title.exact = "Tool"
    "#,
    );

    result.expect_err("the former explicit exact matcher must not deserialize");
}

#[test]
fn empty_partial_matcher_is_rejected() {
    let config = parse(
        r#"
        [[rules]]
        name = "tool"
        window.title = {}
    "#,
    );

    assert!(validate(&config).is_none(), "empty partial matcher must fail");
}

#[test]
fn program_matchers_are_case_insensitive() {
    let rules = validate(&parse(
        r#"
        [[rules]]
        name = "tool"
        program.name = "TOOL.EXE"
        program.path.ends_with = "/TOOL.EXE"
    "#,
    ))
    .expect("program matcher must validate");

    assert!(matches_rule(
        &rules[0],
        &window("tool.exe", "c:/apps/tool.exe", "Tool")));
}

#[test]
fn program_path_matcher_rejects_backslashes() {
    let config = parse(
        r#"
        [[rules]]
        name = "tool"
        program.path = 'C:\Apps\tool.exe'
    "#,
    );

    assert!(validate(&config).is_none(), "backslash path must fail");
}

#[test]
fn window_title_matchers_are_case_sensitive() {
    let rules = validate(&parse(
        r#"
        [[rules]]
        name = "tool"
        window.title = "Tool"
    "#,
    ))
    .expect("title matcher must validate");

    assert!(!matches_rule(
        &rules[0],
        &window("tool.exe", "c:/tool.exe", "tool")));
}

#[test]
fn matching_rule_prefers_higher_priority_over_source_order() {
    let rules = validate(&parse(
        r#"
        [[rules]]
        name = "first"
        priority = 0

        [[rules]]
        name = "second"
        priority = 10
    "#,
    ))
    .expect("rules must validate");

    assert_eq!(
        matching_rule_name(&rules, &window("app.exe", "c:/app.exe", "App"))
            .map(SmolStr::as_str),
        Some("second")
    );
}

#[test]
fn matching_rule_uses_source_order_for_equal_priority() {
    let rules = validate(&parse(
        r#"
        [[rules]]
        name = "first"
        priority = 10

        [[rules]]
        name = "second"
        priority = 10
    "#,
    ))
    .expect("rules must validate");

    assert_eq!(
        matching_rule_name(&rules, &window("app.exe", "c:/app.exe", "App"))
            .map(SmolStr::as_str),
        Some("first")
    );
}

#[test]
fn size_filtered_rule_rejects_incomplete_window_details() {
    let rules = validate(&parse(
        r#"
        [[rules]]
        name = "large"
        window.min = [640, 480]
    "#,
    ))
    .expect("size matcher must validate");

    let mut candidate = window("app.exe", "c:/app.exe", "App");
    candidate.detail = Err(anyhow::anyhow!("client size unavailable"));

    assert_eq!(matching_rule_name(&rules, &candidate), None);
}

/// Native geometry must satisfy both array bounds, including equal endpoints.
#[test]
fn size_bounds_are_inclusive() {
    let rules = validate(&parse(
        r#"
        [[rules]]
        name = "bounded"
        window.min = [640, 480]
        window.max = [640, 480]
    "#,
    ))
    .expect("equal inclusive bounds must validate");

    assert_eq!(
        matching_rule_name(
            &rules,
            &window_with_size("app.exe", "c:/app.exe", "App", Size2D::new(640, 480)))
            .map(SmolStr::as_str),
        Some("bounded")
    );

    for size in [[639, 480], [640, 479], [641, 480], [640, 481]] {
        assert_eq!(
            matching_rule_name(
                &rules,
                &window_with_size("app.exe", "c:/app.exe", "App", Size2D::from(size))),
            None,
            "size {size:?} must fail one of the bounds"
        );
    }
}

#[test]
fn reversed_window_size_bounds_are_rejected() {
    let config = parse(
        r#"
        [[rules]]
        name = "bounded"
        window.min = [800, 480]
        window.max = [640, 1080]
    "#,
    );

    assert!(validate(&config).is_none(), "reversed bounds must fail");
}

#[test]
fn future_regex_matcher_is_rejected_by_deserialization() {
    let result = toml::from_str::<Config>(
        r#"
        [[rules]]
        name = "future"
        window.title.regex = ".*"
    "#,
    );

    result.expect_err("future matcher form must not deserialize");
}

#[test]
fn future_glob_matcher_is_rejected_by_deserialization() {
    let result = toml::from_str::<Config>(
        r#"
        [[rules]]
        name = "future"
        program.name.glob = "*.exe"
    "#,
    );

    result.expect_err("future matcher form must not deserialize");
}

#[test]
fn unknown_rule_property_is_rejected_by_deserialization() {
    let result = toml::from_str::<Config>(
        r#"
        [[rules]]
        name = "future"
        inherited_from = "base"
    "#,
    );

    result.expect_err("unknown rule property must not deserialize");
}

#[test]
fn documented_example_validates() {
    parse_config(include_str!("../../../docs/config.example.toml"))
        .expect("documented example must deserialize and validate");
}

#[test]
fn explicit_false_disables_all_resize_controls() {
    let rules = validate(&parse("[[rules]]\nname = 'app'\nresize = false")).unwrap();
    let rule = &rules[0];
    assert_eq!(
        (rule.resize.primary_size(), rule.resize.selector().as_ref()),
        (None, None)
    );
}

#[test]
fn empty_selector_is_enabled_and_unbounded() {
    let rules = validate(&parse("[[rules]]\nname = 'app'\nresize = {}")).unwrap();
    assert_eq!(
        rules[0].resize.selector(),
        Some(ResizeSelector::default())
    );
}

/// Array validation keeps field- and axis-specific errors outside the supported range.
#[test]
fn configured_sizes_reject_dimensions_outside_supported_range() {
    for field in [
        "resize",
        "resize.exact",
        "resize.default",
        "resize.min",
        "resize.max",
        "window.min",
        "window.max",
    ] {
        for (size, axis) in [
            ([0, 900], 0),
            ([1440, 0], 1),
            ([-1, 900], 0),
            ([1440, -1], 1),
            ([MAX_SIZE_DIMENSION + 1, 900], 0),
            ([1440, MAX_SIZE_DIMENSION + 1], 1),
        ] {
            let source = format!("[[rules]]\nname = 'app'\n{field} = {size:?}");
            assert!(
                validate(&parse(&source)).is_none(),
                "rules[0].{field}[{axis}] must reject {}",
                size[axis]);
        }
    }
}

#[test]
fn resize_rejects_reversed_selector_bounds() {
    let config =
        parse("[[rules]]\nname = 'app'\nresize.min = [640, 900]\nresize.max = [1280, 800]");
    assert!(validate(&config).is_none(), "reversed resize bounds must fail");
}

/// Exact actions and selectors remain mutually exclusive after the size type change.
#[test]
fn resize_rejects_mixed_variants_unknown_fields_and_oversized_arrays() {
    for resize in [
        "{ exact = [640, 480], min = [320, 240] }",
        "{ exact = [640, 480], default = [800, 600] }",
        "{ enabled = true }",
        "{ min = [640, 480, 1] }",
    ] {
        let source = format!("[[rules]]\nname = 'app'\nresize = {resize}");
        assert!(
            toml::from_str::<Config>(&source).is_err(),
            "must reject {resize}"
        );
    }
}

/// Serde rejects missing or mistyped dimensions before semantic validation runs.
#[test]
fn configured_size_arrays_reject_missing_or_invalid_dimensions() {
    for field in [
        "resize",
        "resize.exact",
        "resize.default",
        "resize.min",
        "resize.max",
        "window.min",
        "window.max",
    ] {
        for size in [
            "[]",
            "[640]",
            "[640.5, 480]",
            "[640, 480.5]",
            "['640', 480]",
            "[640, '480']",
            "[2147483648, 480]",
            "[640, 2147483648]",
            "[-2147483649, 480]",
            "[640, -2147483649]",
            "{ width = 640, height = 480 }",
        ] {
            let source = format!("[[rules]]\nname = 'app'\n{field} = {size}");
            assert!(
                toml::from_str::<Config>(&source).is_err(),
                "must reject {field} = {size}"
            );
        }
    }
}

/// All size fields retain axis order and optionality across a TOML round trip.
#[test]
fn config_round_trip_preserves_all_resize_modes_and_filters() {
    let config = parse(
        r#"
        [[rules]]
        name = "disabled"
        [[rules]]
        name = "unbounded"
        resize = true
        program.name = "APP.EXE"
        [[rules]]
        name = "default-shorthand"
        resize = [1440, 900]
        [[rules]]
        name = "exact"
        resize.exact = [640, 480]
        [[rules]]
        name = "selector"
        resize.default = [1440, 900]
        resize.min = [640, 480]
        resize.max = [1920, 1200]
        window.title.contains = "App"
        window.min = [640, 480]
        window.max = [1920, 1080]
    "#,
    );
    let serialized = toml::to_string(&config).unwrap();
    let round_trip = parse(&serialized);
    assert_eq!(config.rules.len(), round_trip.rules.len());
    for (original, restored) in config.rules.iter().zip(&round_trip.rules) {
        assert_eq!(original.resize, restored.resize);
        assert_eq!(original.program, restored.program);
        assert_eq!(original.window, restored.window);
    }
    validate(&round_trip).unwrap();
}

#[test]
fn partial_matcher_rejects_a_candidate_missing_any_predicate() {
    let rules = validate(&parse(
        r#"
        [[rules]]
        name = "tool"
        window.title = { starts_with = "Tool", ends_with = "Ready", contains = " - " }
    "#,
    ))
    .unwrap();
    assert!(!matches_rule(
        &rules[0],
        &window("tool.exe", "c:/tool.exe", "Tool Ready")));
}

#[test]
fn absent_filters_remain_absent_after_verification() {
    let rules = validate(&parse("[[rules]]\nname = 'app'")).unwrap();
    let rule = &rules[0];
    assert!(
        rule.program.name.is_none()
            && rule.program.path.is_none()
            && rule.window.title.is_none()
    );
}

#[test]
fn parser_rejects_every_rule_when_one_rule_fails_semantic_validation() {
    assert!(parse_config(
        r#"
        [[rules]]
        name = "first"
        [[rules]]
        name = "broken"
        resize.exact = [0, 900]
        [[rules]]
        name = "last"
    "#).is_none());
}

#[test]
fn parser_deserializes_every_rule_before_semantic_validation() {
    for source in [
        "[[rules",
        "rules = 'bad'",
        "[rules]\nname = 'app'",
        "unknown = []",
        "rules = [{ name = 'good' }, 'bad']",
        "rules = ['bad', { name = 'good' }]",
        "rules = [{ name = 'good' }, { name = 'bad', unknown = true }]",
    ] {
        assert!(parse_config(source).is_none(), "{source}");
    }
}

#[test]
fn empty_documents_remain_valid_but_invalid_only_documents_do_not() {
    for source in ["", "# Just comments\n", "rules = []"] {
        assert!(parse_config(source).is_some_and(|config| config.rules.is_empty()));
    }
    assert!(parse_config("[[rules]]\nname = 'INVALID'").is_none());
}

#[test]
fn parser_preserves_every_rule_in_source_order_after_complete_validation() {
    let config = parse_config(
        r#"
        [[rules]]
        name = "first"
        [[rules]]
        name = "second"
        [[rules]]
        name = "last"
    "#).unwrap();
    assert_eq!(
        config.rules.iter().map(|rule| rule.name.as_str()).collect::<Vec<_>>(),
        ["first", "second", "last"]);
}

#[test]
fn parser_checks_each_partial_program_component_without_folding_titles() {
    let config = parse_config(
        r#"
        [[rules]]
        name = "app"
        program.name = { starts_with = "TO", ends_with = ".EXE", contains = "OOL" }
        program.path = { starts_with = "C:/", ends_with = "/TOOL.EXE", contains = "/APPS/" }
        window.title = { starts_with = "Tool", ends_with = "Ready", contains = " - " }
    "#).unwrap();
    let rule = &config.rules[0];
    assert!(matches_rule(
        rule,
        &window("tool.exe", "c:/apps/tool.exe", "Tool - Ready")));
    assert!(!matches_rule(
        rule,
        &window("tool.exe", "c:/other/tool.exe", "Tool - Ready")));
    assert!(!matches_rule(
        rule,
        &window("tool.exe", "c:/apps/tool.exe", "tool - Ready")));
}

#[test]
fn parser_rejects_oversized_arrays_with_the_complete_configuration() {
    for field in [
        "window.min",
        "window.max",
        "resize",
        "resize.exact",
        "resize.min",
        "resize.max",
        "resize.default",
    ] {
        let source =
            format!("[[rules]]\nname = 'bad'\n{field} = [640, 480, 1]\n[[rules]]\nname = 'good'");
        assert!(parse_config(&source).is_none(), "{field}");
    }
}

/// Exercises semantic validation through the same transactional public parser as startup.
fn validate(config: &Config) -> Option<Vec<Rule>> {
    let source = toml::to_string(&config).expect("typed test config must serialize");
    parse_config(&source).map(|config| config.rules)
}
