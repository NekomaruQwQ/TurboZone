use euclid::default::Size2D;
use turbozone_core::*;

/// Deserializes fixtures that exercise compilation separately from document parsing.
fn parse(source: &str) -> Config {
    toml::from_str(source).expect("test configuration must deserialize")
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

    assert_eq!(
        validate(config).expect_err("duplicate rule name must fail"),
        ConfigError::DuplicateRuleName {
            name: "same".to_owned(),
        }
    );
}

#[test]
fn explicit_action_fields_enable_controls() {
    let runtime = validate(parse(
        r#"
        [[rules]]
        name = "app"
        move = true
        resize = true
    "#,
    ))
    .expect("explicit action fields must validate");
    let rule = &runtime.rules[0];

    assert_eq!(
        (rule.relocate, rule.resize_selector.is_some()),
        (true, true)
    );
}

#[test]
fn omitted_action_fields_disable_controls() {
    let runtime = validate(parse(
        r#"
        [[rules]]
        name = "app"
    "#,
    ))
    .expect("omitted action fields must use defaults");
    let rule = &runtime.rules[0];

    assert_eq!(
        (
            rule.relocate,
            rule.resize_exact,
            rule.resize_selector.as_ref()
        ),
        (false, None, None)
    );
}

#[test]
fn exact_resize_disables_selector() {
    let runtime = validate(parse(
        r#"
        [[rules]]
        name = "app"
        resize.exact = [1440, 900]
    "#,
    ))
    .expect("resize target must validate");

    assert_eq!(
        (
            runtime.rules[0].resize_exact,
            runtime.rules[0].resize_selector.as_ref()
        ),
        (Some(Size2D::new(1440, 900)), None)
    );
}

#[test]
fn selector_default_is_independent_of_selector_bounds() {
    let runtime = validate(parse(
        r#"
        [[rules]]
        name = "app"
        resize.default = [1440, 900]
        resize.max = [1280, 800]
    "#,
    ))
    .expect("default target need not be in selector bounds");

    assert_eq!(
        runtime.rules[0]
            .resize_selector
            .as_ref()
            .and_then(|selector| selector.default),
        Some([1440, 900])
    );
}

#[test]
fn selector_default_shorthand_enables_an_unbounded_selector() {
    let runtime = validate(parse("[[rules]]\nname = 'app'\nresize = [1440, 900]")).unwrap();

    assert_eq!(
        runtime.rules[0].resize_selector,
        Some(ResizeSelector {
            default: Some([1440, 900]),
            min: None,
            max: None,
        })
    );
}

#[test]
fn empty_trimmed_description_uses_rule_name_fallback() {
    let runtime = validate(parse(
        r#"
        [[rules]]
        name = "app"
        description = "   "
    "#,
    ))
    .expect("empty trimmed description must remain valid");

    assert_eq!(runtime.rules[0].description, None);
}

/// Selector bounds apply independently to both axes and include their endpoints.
#[test]
fn selector_limits_check_both_axes_and_reject_nonpositive_sizes() {
    let runtime = validate(parse(
        r#"
        [[rules]]
        name = "app"
        resize.min = [1400, 500]
        resize.max = [3840, 1000]
    "#,
    ))
    .expect("selector limits must validate");
    let resize = runtime.rules[0]
        .resize_selector
        .as_ref()
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
    let runtime = validate(parse(
        r#"
        [[rules]]
        name = "tool"
        window.title.starts_with = "Tool"
        window.title.ends_with = "Ready"
        window.title.contains = " - "
    "#,
    ))
    .expect("partial matcher must validate");
    let rule = &runtime.rules[0];

    assert!(rule.matches(None, "c:/tool.exe", "Tool - Ready", None));
}

#[test]
fn partial_matcher_ignores_empty_components() {
    let runtime = validate(parse(
        r#"
        [[rules]]
        name = "tool"
        window.title.starts_with = ""
        window.title.ends_with = "Ready"
    "#,
    ))
    .expect("empty partial components must behave as omitted");

    assert!(runtime.rules[0].matches(None, "c:/tool.exe", "Tool Ready", None));
}

#[test]
fn string_matcher_is_exact() {
    let runtime = validate(parse(
        r#"
        [[rules]]
        name = "tool"
        window.title = "Tool"
    "#,
    ))
    .expect("bare matcher must validate");

    assert!(!runtime.rules[0].matches(None, "c:/tool.exe", "Tool Window", None));
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

    assert_eq!(
        validate(config).expect_err("empty partial matcher must fail"),
        ConfigError::EmptyPartialMatcher {
            field: "rules[0].window.title".to_owned(),
        }
    );
}

#[test]
fn program_matchers_are_case_insensitive() {
    let runtime = validate(parse(
        r#"
        [[rules]]
        name = "tool"
        program.name = "TOOL.EXE"
        program.path.ends_with = "/TOOL.EXE"
    "#,
    ))
    .expect("program matcher must validate");

    assert!(runtime.rules[0].matches(Some("tool.exe"), "c:/apps/tool.exe", "Tool", None));
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

    assert_eq!(
        validate(config).expect_err("backslash path must fail"),
        ConfigError::BackslashInProgramPath {
            field: "rules[0].program.path".to_owned(),
        }
    );
}

#[test]
fn window_title_matchers_are_case_sensitive() {
    let runtime = validate(parse(
        r#"
        [[rules]]
        name = "tool"
        window.title = "Tool"
    "#,
    ))
    .expect("title matcher must validate");

    assert!(!runtime.rules[0].matches(None, "c:/tool.exe", "tool", None));
}

#[test]
fn matching_rule_prefers_higher_priority_over_source_order() {
    let runtime = validate(parse(
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
        runtime.matching_rule_index(None, "c:/app.exe", "App", None),
        Some(1)
    );
}

#[test]
fn matching_rule_uses_source_order_for_equal_priority() {
    let runtime = validate(parse(
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
        runtime.matching_rule_index(None, "c:/app.exe", "App", None),
        Some(0)
    );
}

#[test]
fn size_filtered_rule_rejects_missing_client_size() {
    let runtime = validate(parse(
        r#"
        [[rules]]
        name = "large"
        window.min = [640, 480]
    "#,
    ))
    .expect("size matcher must validate");

    assert_eq!(
        runtime.matching_rule_index(None, "c:/app.exe", "App", None),
        None
    );
}

/// Native geometry must satisfy both array bounds, including equal endpoints.
#[test]
fn size_bounds_are_inclusive() {
    let runtime = validate(parse(
        r#"
        [[rules]]
        name = "bounded"
        window.min = [640, 480]
        window.max = [640, 480]
    "#,
    ))
    .expect("equal inclusive bounds must validate");

    assert_eq!(
        runtime.matching_rule_index(None, "c:/app.exe", "App", Some(Size2D::new(640, 480))),
        Some(0)
    );

    for size in [[639, 480], [640, 479], [641, 480], [640, 481]] {
        assert_eq!(
            runtime.matching_rule_index(None, "c:/app.exe", "App", Some(Size2D::from(size))),
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

    assert_eq!(
        validate(config).expect_err("reversed bounds must fail"),
        ConfigError::InvalidBounds {
            minimum_field: "rules[0].window.min[0]".to_owned(),
            maximum_field: "rules[0].window.max[0]".to_owned(),
        }
    );
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
fn documented_m1_example_validates() {
    let report = parse_config(include_str!("../../../docs/M1-Plan-config.toml"))
        .expect("documented M1 example must deserialize");
    assert_eq!(
        report.diagnostics.len(),
        0,
        "documented M1 example must compile without rejected rules"
    );
}

#[test]
fn explicit_false_disables_all_resize_controls() {
    let runtime = validate(parse("[[rules]]\nname = 'app'\nresize = false")).unwrap();
    let rule = &runtime.rules[0];
    assert_eq!(
        (rule.resize_exact, rule.resize_selector.as_ref()),
        (None, None)
    );
}

#[test]
fn empty_selector_is_enabled_and_unbounded() {
    let runtime = validate(parse("[[rules]]\nname = 'app'\nresize = {}")).unwrap();
    assert_eq!(
        runtime.rules[0].resize_selector,
        Some(ResizeSelector::default())
    );
}

/// Array validation keeps field- and axis-specific errors for every configured size.
#[test]
fn configured_sizes_reject_nonpositive_dimensions() {
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
        ] {
            let source = format!("[[rules]]\nname = 'app'\n{field} = {size:?}");
            assert_eq!(
                validate(parse(&source)).unwrap_err(),
                ConfigError::InvalidDimension {
                    field: format!("rules[0].{field}[{axis}]"),
                    value: size[axis],
                }
            );
        }
    }
}

#[test]
fn resize_rejects_reversed_selector_bounds() {
    let config =
        parse("[[rules]]\nname = 'app'\nresize.min = [640, 900]\nresize.max = [1280, 800]");
    assert_eq!(
        validate(config).unwrap_err(),
        ConfigError::InvalidBounds {
            minimum_field: "rules[0].resize.min[1]".to_owned(),
            maximum_field: "rules[0].resize.max[1]".to_owned(),
        }
    );
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
fn config_round_trip_preserves_all_resize_modes_and_generic_filters() {
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
    validate(round_trip).unwrap();
}

#[test]
fn partial_matcher_rejects_a_candidate_missing_any_predicate() {
    let runtime = validate(parse(
        r#"
        [[rules]]
        name = "tool"
        window.title = { starts_with = "Tool", ends_with = "Ready", contains = " - " }
    "#,
    ))
    .unwrap();
    assert!(!runtime.rules[0].matches(None, "c:/tool.exe", "Tool Ready", None));
}

#[test]
fn absent_filters_remain_absent_after_compilation() {
    let runtime = validate(parse("[[rules]]\nname = 'app'")).unwrap();
    let rule = &runtime.rules[0];
    assert!(
        rule.program_filters.name.is_none()
            && rule.program_filters.path.is_none()
            && rule.window_filters.title.is_none()
    );
}

#[test]
fn parser_recovers_whole_rules_in_source_order_and_only_valid_names_are_reserved() {
    let report = parse_config(
        r#"
        [[rules]]
        name = "reused"
        resize.exact = [0, 900]
        [[rules]]
        name = "malformed"
        move = "yes"
        [[rules]]
        name = "reused"
        priority = 7
        [[rules]]
        name = "reused"
        priority = 99
        [[rules]]
        name = "last"
        priority = 7
    "#,
    )
    .unwrap();
    assert_eq!(
        report
            .runtime
            .rules
            .iter()
            .map(|rule| rule.name.as_str())
            .collect::<Vec<_>>(),
        ["reused", "last"]
    );
    assert_eq!(
        report
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.index)
            .collect::<Vec<_>>(),
        [0, 1, 3]
    );
    assert!(matches!(
        report.diagnostics[0].error,
        ConfigError::InvalidDimension { .. }
    ));
    assert!(matches!(
        report.diagnostics[1].error,
        ConfigError::Deserialize(_)
    ));
    assert!(matches!(
        report.diagnostics[2].error,
        ConfigError::DuplicateRuleName { .. }
    ));
    assert_eq!(
        report
            .runtime
            .matching_rule_index(None, "c:/app.exe", "App", None),
        Some(0)
    );
}

#[test]
fn malformed_document_envelopes_are_fatal_but_bad_rule_values_are_recoverable() {
    for source in [
        "[[rules",
        "rules = 'bad'",
        "[rules]\nname = 'app'",
        "unknown = []",
    ] {
        assert!(parse_config(source).is_err(), "{source}");
    }
    let report =
        parse_config("rules = ['bad', { name = 'good' }, { name = 'bad', unknown = true }]")
            .unwrap();
    assert_eq!(report.runtime.rules.len(), 1);
    assert_eq!(report.runtime.rules[0].name, "good");
    assert_eq!(
        report
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.index)
            .collect::<Vec<_>>(),
        [0, 2]
    );
}

#[test]
fn empty_documents_and_documents_with_only_rejected_rules_remain_usable() {
    for source in ["", "# Just comments\n", "rules = []"] {
        let report = parse_config(source).unwrap();
        assert!(report.runtime.rules.is_empty() && report.diagnostics.is_empty());
    }
    let report = parse_config("[[rules]]\nname = 'INVALID'").unwrap();
    assert!(report.runtime.rules.is_empty());
    assert_eq!(report.diagnostics.len(), 1);
}

#[test]
fn parser_checks_each_partial_program_component_without_folding_titles() {
    let report = parse_config(
        r#"
        [[rules]]
        name = "app"
        program.name = { starts_with = "TO", ends_with = ".EXE", contains = "OOL" }
        program.path = { starts_with = "C:/", ends_with = "/TOOL.EXE", contains = "/APPS/" }
        window.title = { starts_with = "Tool", ends_with = "Ready", contains = " - " }
    "#,
    )
    .unwrap();
    assert!(report.diagnostics.is_empty());
    let rule = &report.runtime.rules[0];
    assert!(rule.matches(Some("tool.exe"), "c:/apps/tool.exe", "Tool - Ready", None));
    assert!(!rule.matches(Some("tool.exe"), "c:/other/tool.exe", "Tool - Ready", None));
    assert!(!rule.matches(Some("tool.exe"), "c:/apps/tool.exe", "tool - Ready", None));
}

#[test]
fn parser_rejects_oversized_arrays_before_compilation() {
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
        let report = parse_config(&source).unwrap();
        assert_eq!(report.diagnostics.len(), 1, "{field}");
        assert_eq!(report.runtime.rules[0].name, "good");
    }
}

#[test]
fn parser_errors_retain_locations_without_private_source_excerpts() {
    let error = parse_config("# Header\nrules = [ # PRIVATE_SOURCE_SENTINEL").unwrap_err();
    let message = format!("{error:#}");
    assert!(message.contains("line 2"));
    assert!(!message.contains("PRIVATE_SOURCE_SENTINEL"));
}

/// Adapts a compilation report to the strict assertions used by these fixtures.
fn validate(config: Config) -> Result<RuntimeConfig, ConfigError> {
    let report = compile_config(config);
    match report.diagnostics.into_iter().next() {
        Some(diagnostic) => Err(diagnostic.error),
        None => Ok(report.runtime),
    }
}
