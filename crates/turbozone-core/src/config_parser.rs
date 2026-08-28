//! Configuration validation and compilation.

use std::collections::BTreeSet;

use euclid::default::Size2D;
use thiserror::Error;

use crate::{
    Config, Rule, Pattern, PatternMatcher, ProgramFilter, ResizeLimits,
    ResizeRule, RuntimeConfig, RuntimeRule, WindowFilter,
};

impl Config {
    /// Validates serialized rules and compiles them into runtime state.
    /// Returns the first invalid name, pattern, dimension, or pair of bounds.
    pub fn validate(self) -> Result<RuntimeConfig, ConfigError> {
        let mut names = BTreeSet::new();
        let mut rules = Vec::with_capacity(self.rules.len());

        for (index, rule) in self.rules.into_iter().enumerate() {
            if !is_valid_rule_name(&rule.name) {
                return Err(ConfigError::InvalidRuleName {
                    index,
                    name: rule.name,
                });
            }
            if !names.insert(rule.name.clone()) {
                return Err(ConfigError::DuplicateRuleName { name: rule.name });
            }
            rules.push(compile_rule(index, rule)?);
        }

        Ok(RuntimeConfig { rules })
    }
}

/// Returns whether a name is a lowercase sequence of TOML-style dotted bare keys.
pub fn is_valid_rule_name(name: &str) -> bool {
    !name.is_empty() && name.split('.').all(|component| {
        !component.is_empty() && component.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-'
        })
    })
}

/// A semantic configuration validation failure.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    /// A rule name did not satisfy the lowercase dotted bare-key grammar.
    #[error("rules[{index}].name '{name}' must match [a-z0-9_-]+(?:\\.[a-z0-9_-]+)*")]
    InvalidRuleName {
        /// Zero-based rule index.
        index: usize,
        /// Invalid configured name.
        name: String,
    },
    /// The same rule name appeared more than once.
    #[error("duplicate rule name '{name}'")]
    DuplicateRuleName {
        /// Duplicate configured name.
        name: String,
    },
    /// A partial matcher contained no predicates.
    #[error("{field} must contain starts_with, ends_with, or contains")]
    EmptyPartialMatcher {
        /// Configuration field containing the invalid matcher.
        field: String,
    },
    /// A configured program-path pattern used a backslash.
    #[error("{field} must use forward slashes; backslashes are not accepted")]
    BackslashInProgramPath {
        /// Configuration field containing the invalid path pattern.
        field: String,
    },
    /// A configured dimension was not positive.
    #[error("{field} must be positive, found {value}")]
    InvalidDimension {
        /// Configuration field containing the invalid dimension.
        field: String,
        /// Invalid configured value.
        value: i32,
    },
    /// A minimum dimension exceeded its corresponding maximum.
    #[error("{minimum_field} must not exceed {maximum_field}")]
    InvalidBounds {
        /// Configuration field containing the minimum.
        minimum_field: String,
        /// Configuration field containing the maximum.
        maximum_field: String,
    },
}

/// Resolves defaults and compiles one rule, retaining source-order diagnostics.
fn compile_rule(index: usize, rule: Rule) -> Result<RuntimeRule, ConfigError> {
    let prefix = format!("rules[{index}]");
    let description = rule.description.trim().to_owned();
    let description = (!description.is_empty()).then_some(description);
    let (resize_exact, resize_limits) = compile_resize(rule.resize, &prefix)?;
    let program_filters = compile_program_match(rule.program, &prefix)?;
    let window_filters = compile_window_match(rule.window, &prefix)?;

    Ok(RuntimeRule {
        name: rule.name,
        description,
        relocate: rule.relocate,
        resize_exact,
        resize_limits,
        program_filters,
        window_filters,
        priority: rule.priority,
    })
}

/// Separates exact-only actions from selectors; a missing selector means disabled.
fn compile_resize(
    resize: ResizeRule,
    prefix: &str) -> Result<(Option<Size2D<i32>>, Option<ResizeLimits>), ConfigError> {
    match resize {
        ResizeRule::Boolean(false) => Ok((None, None)),
        ResizeRule::Boolean(true) => Ok((None, Some(ResizeLimits::default()))),
        ResizeRule::Exact { exact } => {
            validate_size(exact, &format!("{prefix}.resize.exact"))?;
            Ok((Some(Size2D::from(exact)), None))
        },
        ResizeRule::Selector(limits) => {
            if let Some(size) = limits.default {
                validate_size(size, &format!("{prefix}.resize.default"))?;
            }
            validate_size_bounds(limits.min, limits.max, &format!("{prefix}.resize"))?;
            Ok((None, Some(limits)))
        },
    }
}

/// Compiles case-insensitive program patterns without normalizing config paths.
fn compile_program_match(
    matcher: ProgramFilter<Pattern>,
    prefix: &str) -> Result<ProgramFilter<Vec<PatternMatcher>>, ConfigError> {
    let name = matcher.name
        .map(|matcher| compile_string_matcher(
            matcher,
            &format!("{prefix}.program.name"),
            false,
            false))
        .transpose()?;
    let path = matcher.path
        .map(|matcher| compile_string_matcher(
            matcher,
            &format!("{prefix}.program.path"),
            false,
            true))
        .transpose()?;
    Ok(ProgramFilter { name, path })
}

/// Compiles case-sensitive title patterns and validates inclusive size bounds.
fn compile_window_match(
    matcher: WindowFilter<Pattern>,
    prefix: &str) -> Result<WindowFilter<Vec<PatternMatcher>>, ConfigError> {
    let title = matcher.title
        .map(|matcher| compile_string_matcher(
            matcher,
            &format!("{prefix}.window.title"),
            true,
            false))
        .transpose()?;
    validate_size_bounds(matcher.min, matcher.max, &format!("{prefix}.window"))?;
    Ok(WindowFilter {
        title,
        min: matcher.min,
        max: matcher.max,
    })
}

/// Selects exact or ANDed partial predicates, rejecting empty partial patterns.
fn compile_string_matcher(
    matcher: Pattern,
    field: &str,
    case_sensitive: bool,
    is_path: bool) -> Result<Vec<PatternMatcher>, ConfigError> {
    match matcher {
        Pattern::Exact(pattern) => Ok(vec![compile_string_predicate(
            pattern,
            field,
            case_sensitive,
            is_path,
            string_equals)?]),
        Pattern::Partial {
            starts_with,
            ends_with,
            contains,
        } => compile_partial_matcher(
            starts_with,
            ends_with,
            contains,
            field,
            case_sensitive,
            is_path),
    }
}

/// Ignores empty components but requires at least one effective predicate.
fn compile_partial_matcher(
    starts_with: String,
    ends_with: String,
    contains: String,
    field: &str,
    case_sensitive: bool,
    is_path: bool) -> Result<Vec<PatternMatcher>, ConfigError> {
    if starts_with.is_empty() && ends_with.is_empty() && contains.is_empty() {
        return Err(ConfigError::EmptyPartialMatcher {
            field: field.to_owned(),
        });
    }
    let mut predicates = Vec::with_capacity(3);
    if !starts_with.is_empty() {
        predicates.push(compile_string_predicate(
            starts_with,
            &format!("{field}.starts_with"),
            case_sensitive,
            is_path,
            string_starts_with)?);
    }
    if !ends_with.is_empty() {
        predicates.push(compile_string_predicate(
            ends_with,
            &format!("{field}.ends_with"),
            case_sensitive,
            is_path,
            string_ends_with)?);
    }
    if !contains.is_empty() {
        predicates.push(compile_string_predicate(
            contains,
            &format!("{field}.contains"),
            case_sensitive,
            is_path,
            string_contains)?);
    }
    Ok(predicates)
}

/// Validates path separators and folds only case-insensitive patterns at load time.
fn compile_string_predicate(
    pattern: String,
    field: &str,
    case_sensitive: bool,
    is_path: bool,
    predicate: fn(input: &str, pattern: &str) -> bool) -> Result<PatternMatcher, ConfigError> {
    if is_path && pattern.contains('\\') {
        return Err(ConfigError::BackslashInProgramPath {
            field: field.to_owned(),
        });
    }
    let pattern = if case_sensitive { pattern } else { pattern.to_lowercase() };
    Ok(PatternMatcher::new(pattern, predicate))
}

/// Validates both array dimensions using their configuration indices.
fn validate_size([width, height]: [i32; 2], field: &str) -> Result<(), ConfigError> {
    validate_dimension(width, &format!("{field}[0]"))?;
    validate_dimension(height, &format!("{field}[1]"))
}

/// Checks positive bounds and rejects inverted axes when both bounds are present.
fn validate_size_bounds(
    min: Option<[i32; 2]>,
    max: Option<[i32; 2]>,
    prefix: &str) -> Result<(), ConfigError> {
    if let Some(size) = min {
        validate_size(size, &format!("{prefix}.min"))?;
    }
    if let Some(size) = max {
        validate_size(size, &format!("{prefix}.max"))?;
    }
    if let (Some(min), Some(max)) = (min, max) {
        for (axis, (minimum, maximum)) in min.into_iter().zip(max).enumerate() {
            if minimum > maximum {
                return Err(ConfigError::InvalidBounds {
                    minimum_field: format!("{prefix}.min[{axis}]"),
                    maximum_field: format!("{prefix}.max[{axis}]"),
                });
            }
        }
    }
    Ok(())
}

/// Rejects zero and negative physical-pixel dimensions.
fn validate_dimension(value: i32, field: &str) -> Result<(), ConfigError> {
    if value <= 0 {
        Err(ConfigError::InvalidDimension {
            field: field.to_owned(),
            value,
        })
    } else {
        Ok(())
    }
}

/// Matches the full string.
fn string_equals(input: &str, pattern: &str) -> bool { input == pattern }

/// Matches a literal prefix.
fn string_starts_with(input: &str, pattern: &str) -> bool { input.starts_with(pattern) }

/// Matches a literal suffix.
fn string_ends_with(input: &str, pattern: &str) -> bool { input.ends_with(pattern) }

/// Matches a literal substring.
fn string_contains(input: &str, pattern: &str) -> bool { input.contains(pattern) }

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> Config {
        toml::from_str(source).expect("test configuration must deserialize")
    }

    #[test]
    fn rule_name_accepts_lowercase_dotted_bare_keys() {
        assert!(is_valid_rule_name("vscode.main-project_2"));
    }

    #[test]
    fn rule_name_rejects_characters_and_empty_dotted_components_outside_its_grammar() {
        for name in ["", ".app", "app.", "app..main", "App", "app name", "app/main"] {
            assert!(!is_valid_rule_name(name), "'{name}' must be rejected");
        }
    }

    #[test]
    fn validate_rejects_duplicate_rule_names() {
        let config = parse(r#"
            [[rules]]
            name = "same"

            [[rules]]
            name = "same"
        "#);

        assert_eq!(
            config.validate().expect_err("duplicate rule name must fail"),
            ConfigError::DuplicateRuleName {
                name: "same".to_owned(),
            });
    }

    #[test]
    fn explicit_action_fields_enable_controls() {
        let runtime = parse(r#"
            [[rules]]
            name = "app"
            move = true
            resize = true
        "#).validate().expect("explicit action fields must validate");
        let rule = &runtime.rules[0];

        assert_eq!((rule.relocate, rule.resize_limits.is_some()), (true, true));
    }

    #[test]
    fn omitted_action_fields_disable_controls() {
        let runtime = parse(r#"
            [[rules]]
            name = "app"
        "#).validate().expect("omitted action fields must use defaults");
        let rule = &runtime.rules[0];

        assert_eq!((rule.relocate, rule.resize_exact, rule.resize_limits.as_ref()), (false, None, None));
    }

    #[test]
    fn exact_resize_disables_selector() {
        let runtime = parse(r#"
            [[rules]]
            name = "app"
            resize.exact = [1440, 900]
        "#).validate().expect("resize target must validate");

        assert_eq!(
            (runtime.rules[0].resize_exact, runtime.rules[0].resize_limits.as_ref()),
            (Some(Size2D::new(1440, 900)), None));
    }

    #[test]
    fn selector_default_is_independent_of_selector_bounds() {
        let runtime = parse(r#"
            [[rules]]
            name = "app"
            resize.default = [1440, 900]
            resize.max = [1280, 800]
        "#).validate().expect("default target need not be in selector bounds");

        assert_eq!(
            runtime.rules[0].resize_limits.as_ref().and_then(|limits| limits.default),
            Some([1440, 900]));
    }

    #[test]
    fn empty_trimmed_description_uses_rule_name_fallback() {
        let runtime = parse(r#"
            [[rules]]
            name = "app"
            description = "   "
        "#).validate().expect("empty trimmed description must remain valid");

        assert_eq!(runtime.rules[0].description, None);
    }

    /// Selector bounds apply independently to both axes and include their endpoints.
    #[test]
    fn selector_limits_check_both_axes_and_reject_nonpositive_sizes() {
        let runtime = parse(r#"
            [[rules]]
            name = "app"
            resize.min = [1400, 500]
            resize.max = [3840, 1000]
        "#).validate().expect("selector limits must validate");
        let resize = runtime.rules[0].resize_limits.as_ref().expect("selector must be enabled");

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
            assert_eq!(resize.allows_size(Size2D::from(size)), allowed, "size {size:?}");
        }
        assert!(!ResizeLimits::default().allows_size(Size2D::new(0, 900)));
        assert!(!ResizeLimits::default().allows_size(Size2D::new(1440, -1)));
    }

    #[test]
    fn partial_matcher_ands_every_predicate() {
        let runtime = parse(r#"
            [[rules]]
            name = "tool"
            window.title.starts_with = "Tool"
            window.title.ends_with = "Ready"
            window.title.contains = " - "
        "#).validate().expect("partial matcher must validate");
        let rule = &runtime.rules[0];

        assert!(rule.matches(None, "c:/tool.exe", "Tool - Ready", None));
    }

    #[test]
    fn partial_matcher_ignores_empty_components() {
        let runtime = parse(r#"
            [[rules]]
            name = "tool"
            window.title.starts_with = ""
            window.title.ends_with = "Ready"
        "#).validate().expect("empty partial components must behave as omitted");

        assert!(runtime.rules[0].matches(None, "c:/tool.exe", "Tool Ready", None));
    }

    #[test]
    fn string_matcher_is_exact() {
        let runtime = parse(r#"
            [[rules]]
            name = "tool"
            window.title = "Tool"
        "#).validate().expect("bare matcher must validate");

        assert!(!runtime.rules[0].matches(None, "c:/tool.exe", "Tool Window", None));
    }

    #[test]
    fn explicit_exact_matcher_is_rejected_by_deserialization() {
        let result = toml::from_str::<Config>(r#"
            [[rules]]
            name = "tool"
            window.title.exact = "Tool"
        "#);

        result.expect_err("the former explicit exact matcher must not deserialize");
    }

    #[test]
    fn empty_partial_matcher_is_rejected() {
        let config = parse(r#"
            [[rules]]
            name = "tool"
            window.title = {}
        "#);

        assert_eq!(
            config.validate().expect_err("empty partial matcher must fail"),
            ConfigError::EmptyPartialMatcher {
                field: "rules[0].window.title".to_owned(),
            });
    }

    #[test]
    fn program_matchers_are_case_insensitive() {
        let runtime = parse(r#"
            [[rules]]
            name = "tool"
            program.name = "TOOL.EXE"
            program.path.ends_with = "/TOOL.EXE"
        "#).validate().expect("program matcher must validate");

        assert!(runtime.rules[0].matches(
            Some("tool.exe"),
            "c:/apps/tool.exe",
            "Tool",
            None));
    }

    #[test]
    fn program_path_matcher_rejects_backslashes() {
        let config = parse(r#"
            [[rules]]
            name = "tool"
            program.path = 'C:\Apps\tool.exe'
        "#);

        assert_eq!(
            config.validate().expect_err("backslash path must fail"),
            ConfigError::BackslashInProgramPath {
                field: "rules[0].program.path".to_owned(),
            });
    }

    #[test]
    fn window_title_matchers_are_case_sensitive() {
        let runtime = parse(r#"
            [[rules]]
            name = "tool"
            window.title = "Tool"
        "#).validate().expect("title matcher must validate");

        assert!(!runtime.rules[0].matches(None, "c:/tool.exe", "tool", None));
    }

    #[test]
    fn matching_rule_prefers_higher_priority_over_source_order() {
        let runtime = parse(r#"
            [[rules]]
            name = "first"
            priority = 0

            [[rules]]
            name = "second"
            priority = 10
        "#).validate().expect("rules must validate");

        assert_eq!(runtime.matching_rule_index(None, "c:/app.exe", "App", None), Some(1));
    }

    #[test]
    fn matching_rule_uses_source_order_for_equal_priority() {
        let runtime = parse(r#"
            [[rules]]
            name = "first"
            priority = 10

            [[rules]]
            name = "second"
            priority = 10
        "#).validate().expect("rules must validate");

        assert_eq!(runtime.matching_rule_index(None, "c:/app.exe", "App", None), Some(0));
    }

    #[test]
    fn size_filtered_rule_rejects_missing_client_size() {
        let runtime = parse(r#"
            [[rules]]
            name = "large"
            window.min = [640, 480]
        "#).validate().expect("size matcher must validate");

        assert_eq!(runtime.matching_rule_index(None, "c:/app.exe", "App", None), None);
    }

    /// Native geometry must satisfy both array bounds, including equal endpoints.
    #[test]
    fn size_bounds_are_inclusive() {
        let runtime = parse(r#"
            [[rules]]
            name = "bounded"
            window.min = [640, 480]
            window.max = [640, 480]
        "#).validate().expect("equal inclusive bounds must validate");

        assert_eq!(
            runtime.matching_rule_index(
                None,
                "c:/app.exe",
                "App",
                Some(Size2D::new(640, 480))),
            Some(0));

        for size in [[639, 480], [640, 479], [641, 480], [640, 481]] {
            assert_eq!(
                runtime.matching_rule_index(None, "c:/app.exe", "App", Some(Size2D::from(size))),
                None,
                "size {size:?} must fail one of the bounds");
        }
    }

    #[test]
    fn reversed_window_size_bounds_are_rejected() {
        let config = parse(r#"
            [[rules]]
            name = "bounded"
            window.min = [800, 480]
            window.max = [640, 1080]
        "#);

        assert_eq!(
            config.validate().expect_err("reversed bounds must fail"),
            ConfigError::InvalidBounds {
                minimum_field: "rules[0].window.min[0]".to_owned(),
                maximum_field: "rules[0].window.max[0]".to_owned(),
            });
    }

    #[test]
    fn future_regex_matcher_is_rejected_by_deserialization() {
        let result = toml::from_str::<Config>(r#"
            [[rules]]
            name = "future"
            window.title.regex = ".*"
        "#);

        result.expect_err("future matcher form must not deserialize");
    }

    #[test]
    fn future_glob_matcher_is_rejected_by_deserialization() {
        let result = toml::from_str::<Config>(r#"
            [[rules]]
            name = "future"
            program.name.glob = "*.exe"
        "#);

        result.expect_err("future matcher form must not deserialize");
    }

    #[test]
    fn unknown_rule_property_is_rejected_by_deserialization() {
        let result = toml::from_str::<Config>(r#"
            [[rules]]
            name = "future"
            inherited_from = "base"
        "#);

        result.expect_err("unknown rule property must not deserialize");
    }

    #[test]
    fn documented_m1_example_validates() {
        let config = toml::from_str::<Config>(include_str!("../../../docs/M1-Plan-config.toml"))
            .expect("documented M1 example must deserialize");

        config.validate().expect("documented M1 example must validate");
    }

    #[test]
    fn explicit_false_disables_all_resize_controls() {
        let runtime = parse("[[rules]]\nname = 'app'\nresize = false").validate().unwrap();
        let rule = &runtime.rules[0];
        assert_eq!((rule.resize_exact, rule.resize_limits.as_ref()), (None, None));
    }

    #[test]
    fn empty_selector_is_enabled_and_unbounded() {
        let runtime = parse("[[rules]]\nname = 'app'\nresize = {}").validate().unwrap();
        assert_eq!(runtime.rules[0].resize_limits, Some(ResizeLimits::default()));
    }

    /// Array validation keeps field- and axis-specific errors for every configured size.
    #[test]
    fn configured_sizes_reject_nonpositive_dimensions() {
        for field in ["resize.exact", "resize.default", "resize.min", "resize.max", "window.min", "window.max"] {
            for (size, axis) in [([0, 900], 0), ([1440, 0], 1), ([-1, 900], 0), ([1440, -1], 1)] {
                let source = format!("[[rules]]\nname = 'app'\n{field} = {size:?}");
                assert_eq!(parse(&source).validate().unwrap_err(), ConfigError::InvalidDimension {
                    field: format!("rules[0].{field}[{axis}]"),
                    value: size[axis],
                });
            }
        }
    }

    #[test]
    fn resize_rejects_reversed_selector_bounds() {
        let config = parse("[[rules]]\nname = 'app'\nresize.min = [640, 900]\nresize.max = [1280, 800]");
        assert_eq!(config.validate().unwrap_err(), ConfigError::InvalidBounds {
            minimum_field: "rules[0].resize.min[1]".to_owned(),
            maximum_field: "rules[0].resize.max[1]".to_owned(),
        });
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
            assert!(toml::from_str::<Config>(&source).is_err(), "must reject {resize}");
        }
    }

    /// Serde rejects missing or mistyped dimensions before semantic validation runs.
    #[test]
    fn configured_size_arrays_reject_missing_or_invalid_dimensions() {
        for field in ["resize.exact", "resize.default", "resize.min", "resize.max", "window.min", "window.max"] {
            for size in [
                "[]", "[640]",
                "[640.5, 480]", "[640, 480.5]",
                "['640', 480]", "[640, '480']",
                "[2147483648, 480]", "[640, 2147483648]",
                "[-2147483649, 480]", "[640, -2147483649]",
                "{ width = 640, height = 480 }",
            ] {
                let source = format!("[[rules]]\nname = 'app'\n{field} = {size}");
                assert!(toml::from_str::<Config>(&source).is_err(), "must reject {field} = {size}");
            }
        }
    }

    /// All size fields retain axis order and optionality across a TOML round trip.
    #[test]
    fn config_round_trip_preserves_all_resize_modes_and_generic_filters() {
        let config = parse(r#"
            [[rules]]
            name = "disabled"
            [[rules]]
            name = "unbounded"
            resize = true
            program.name = "APP.EXE"
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
        "#);
        let serialized = toml::to_string(&config).unwrap();
        let round_trip = parse(&serialized);
        assert_eq!(config.rules.len(), round_trip.rules.len());
        for (original, restored) in config.rules.iter().zip(&round_trip.rules) {
            assert_eq!(original.resize, restored.resize);
            assert_eq!(original.program, restored.program);
            assert_eq!(original.window, restored.window);
        }
        round_trip.validate().unwrap();
    }

    #[test]
    fn partial_matcher_rejects_a_candidate_missing_any_predicate() {
        let runtime = parse(r#"
            [[rules]]
            name = "tool"
            window.title = { starts_with = "Tool", ends_with = "Ready", contains = " - " }
        "#).validate().unwrap();
        assert!(!runtime.rules[0].matches(None, "c:/tool.exe", "Tool Ready", None));
    }

    #[test]
    fn absent_filters_remain_absent_after_compilation() {
        let runtime = parse("[[rules]]\nname = 'app'").validate().unwrap();
        let rule = &runtime.rules[0];
        assert!(rule.program_filters.name.is_none()
            && rule.program_filters.path.is_none()
            && rule.window_filters.title.is_none());
    }
}
