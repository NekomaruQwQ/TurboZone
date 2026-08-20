//! Configuration validation and compilation.

use std::collections::BTreeSet;

use euclid::default::Size2D;
use thiserror::Error;

use crate::{
    ComponentStringMatcher, Config, ConfigSize, ExecutableMatch, MoveConfig, MoveTarget,
    ResizeConfig, ResizeSettings, Rule, RuleMatch, RuntimeConfig, RuntimeExecutableMatch,
    RuntimeMove, RuntimeResize, RuntimeRule, RuntimeRuleMatch, RuntimeStringMatcher,
    RuntimeWindowMatch, StringMatcher, WindowMatch,
};

impl Config {
    /// Validates serialized rules and compiles them into runtime state.
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
    /// A component matcher contained no predicates.
    #[error("{field} must contain starts_with, ends_with, or contains")]
    EmptyComponentMatcher {
        /// Configuration field containing the invalid matcher.
        field: String,
    },
    /// A component matcher contained an empty predicate value.
    #[error("{field}.{component} must not be empty")]
    EmptyComponentValue {
        /// Configuration field containing the invalid matcher.
        field: String,
        /// Empty component property.
        component: &'static str,
    },
    /// A configured executable-path pattern used a backslash.
    #[error("{field} must use forward slashes; backslashes are not accepted")]
    BackslashInExecutablePath {
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

fn compile_rule(index: usize, rule: Rule) -> Result<RuntimeRule, ConfigError> {
    let prefix = format!("rules[{index}]");
    let description = rule.description.map(|description| description.trim().to_owned());
    let move_action = match rule.r#move {
        None | Some(MoveConfig::Boolean(false)) => RuntimeMove::Disabled,
        Some(MoveConfig::Boolean(true) | MoveConfig::Target(MoveTarget::Center)) => {
            RuntimeMove::Center
        },
    };
    let resize = compile_resize(rule.resize, &rule.name, &prefix)?;
    let matcher = compile_rule_match(rule.r#match.unwrap_or_default(), &prefix)?;

    Ok(RuntimeRule {
        name: rule.name,
        description,
        r#move: move_action,
        resize,
        r#match: matcher,
    })
}

fn compile_resize(
    resize: Option<ResizeConfig>,
    rule_name: &str,
    prefix: &str) -> Result<RuntimeResize, ConfigError> {
    match resize {
        None | Some(ResizeConfig::Boolean(false)) => Ok(RuntimeResize::default()),
        Some(ResizeConfig::Boolean(true)) => Ok(RuntimeResize {
            enabled: true,
            ..RuntimeResize::default()
        }),
        Some(ResizeConfig::Size(size)) => {
            let size = validate_size(size, &format!("{prefix}.resize"))?;
            Ok(RuntimeResize {
                enabled: true,
                target_width: Some(size.width),
                target_height: Some(size.height),
                ..RuntimeResize::default()
            })
        },
        Some(ResizeConfig::Settings(settings)) => {
            compile_resize_settings(&settings, rule_name, prefix)
        },
    }
}

fn compile_resize_settings(
    settings: &ResizeSettings,
    rule_name: &str,
    prefix: &str) -> Result<RuntimeResize, ConfigError> {
    validate_optional_dimension(settings.target_width, &format!("{prefix}.resize.target_width"))?;
    validate_optional_dimension(settings.target_height, &format!("{prefix}.resize.target_height"))?;
    validate_optional_dimension(settings.min_width, &format!("{prefix}.resize.min_width"))?;
    validate_optional_dimension(settings.min_height, &format!("{prefix}.resize.min_height"))?;
    validate_optional_dimension(settings.max_width, &format!("{prefix}.resize.max_width"))?;
    validate_optional_dimension(settings.max_height, &format!("{prefix}.resize.max_height"))?;
    validate_optional_bounds(
        settings.min_width,
        settings.max_width,
        &format!("{prefix}.resize.min_width"),
        &format!("{prefix}.resize.max_width"))?;
    validate_optional_bounds(
        settings.min_height,
        settings.max_height,
        &format!("{prefix}.resize.min_height"),
        &format!("{prefix}.resize.max_height"))?;

    let (target_width, target_height) = match (settings.target_width, settings.target_height) {
        (Some(width), Some(height)) => (Some(width), Some(height)),
        (None, None) => (None, None),
        (Some(_), None) | (None, Some(_)) => {
            log::warn!(
                "rule '{rule_name}' has an incomplete resize target; ignoring both target dimensions");
            (None, None)
        },
    };

    Ok(RuntimeResize {
        enabled: settings.enabled,
        target_width,
        target_height,
        min_width: settings.min_width,
        min_height: settings.min_height,
        max_width: settings.max_width,
        max_height: settings.max_height,
    })
}

fn compile_rule_match(
    matcher: RuleMatch,
    prefix: &str) -> Result<RuntimeRuleMatch, ConfigError> {
    Ok(RuntimeRuleMatch {
        priority: matcher.priority,
        executable: matcher.executable
            .map(|matcher| compile_executable_match(matcher, prefix))
            .transpose()?
            .filter(|matcher| !matcher.name.is_empty() || !matcher.path.is_empty()),
        window: matcher.window
            .map(|matcher| compile_window_match(matcher, prefix))
            .transpose()?
            .filter(|matcher| {
                !matcher.title.is_empty()
                    || matcher.min_size.is_some()
                    || matcher.max_size.is_some()
            }),
    })
}

fn compile_executable_match(
    matcher: ExecutableMatch,
    prefix: &str) -> Result<RuntimeExecutableMatch, ConfigError> {
    let name = matcher.name
        .map(|matcher| compile_string_matcher(
            matcher,
            &format!("{prefix}.match.executable.name"),
            false,
            false))
        .transpose()?
        .unwrap_or_default();
    let path = matcher.path
        .map(|matcher| compile_string_matcher(
            matcher,
            &format!("{prefix}.match.executable.path"),
            false,
            true))
        .transpose()?
        .unwrap_or_default();
    Ok(RuntimeExecutableMatch { name, path })
}

fn compile_window_match(
    matcher: WindowMatch,
    prefix: &str) -> Result<RuntimeWindowMatch, ConfigError> {
    let title = matcher.title
        .map(|matcher| compile_string_matcher(
            matcher,
            &format!("{prefix}.match.window.title"),
            true,
            false))
        .transpose()?
        .unwrap_or_default();
    let min_size = matcher.min_size
        .map(|size| validate_size(size, &format!("{prefix}.match.window.min_size")))
        .transpose()?;
    let max_size = matcher.max_size
        .map(|size| validate_size(size, &format!("{prefix}.match.window.max_size")))
        .transpose()?;
    if let (Some(minimum), Some(maximum)) = (min_size, max_size) {
        if minimum.width > maximum.width {
            return Err(ConfigError::InvalidBounds {
                minimum_field: format!("{prefix}.match.window.min_size[0]"),
                maximum_field: format!("{prefix}.match.window.max_size[0]"),
            });
        }
        if minimum.height > maximum.height {
            return Err(ConfigError::InvalidBounds {
                minimum_field: format!("{prefix}.match.window.min_size[1]"),
                maximum_field: format!("{prefix}.match.window.max_size[1]"),
            });
        }
    }
    Ok(RuntimeWindowMatch {
        title,
        min_size,
        max_size,
    })
}

fn compile_string_matcher(
    matcher: StringMatcher,
    field: &str,
    case_sensitive: bool,
    is_path: bool) -> Result<Vec<RuntimeStringMatcher>, ConfigError> {
    match matcher {
        StringMatcher::Bare(pattern) => Ok(vec![compile_string_predicate(
            pattern,
            field,
            case_sensitive,
            is_path,
            string_equals)?]),
        StringMatcher::Exact(matcher) => Ok(vec![compile_string_predicate(
            matcher.exact,
            field,
            case_sensitive,
            is_path,
            string_equals)?]),
        StringMatcher::Components(matcher) => {
            compile_component_matcher(matcher, field, case_sensitive, is_path)
        },
    }
}

fn compile_component_matcher(
    matcher: ComponentStringMatcher,
    field: &str,
    case_sensitive: bool,
    is_path: bool) -> Result<Vec<RuntimeStringMatcher>, ConfigError> {
    if matcher.starts_with.is_none() && matcher.ends_with.is_none() && matcher.contains.is_none() {
        return Err(ConfigError::EmptyComponentMatcher {
            field: field.to_owned(),
        });
    }
    let mut predicates = Vec::with_capacity(3);
    if let Some(pattern) = matcher.starts_with {
        validate_component_value(&pattern, field, "starts_with")?;
        predicates.push(compile_string_predicate(
            pattern,
            &format!("{field}.starts_with"),
            case_sensitive,
            is_path,
            string_starts_with)?);
    }
    if let Some(pattern) = matcher.ends_with {
        validate_component_value(&pattern, field, "ends_with")?;
        predicates.push(compile_string_predicate(
            pattern,
            &format!("{field}.ends_with"),
            case_sensitive,
            is_path,
            string_ends_with)?);
    }
    if let Some(pattern) = matcher.contains {
        validate_component_value(&pattern, field, "contains")?;
        predicates.push(compile_string_predicate(
            pattern,
            &format!("{field}.contains"),
            case_sensitive,
            is_path,
            string_contains)?);
    }
    Ok(predicates)
}

fn compile_string_predicate(
    pattern: String,
    field: &str,
    case_sensitive: bool,
    is_path: bool,
    predicate: fn(input: &str, pattern: &str) -> bool) -> Result<RuntimeStringMatcher, ConfigError> {
    if is_path && pattern.contains('\\') {
        return Err(ConfigError::BackslashInExecutablePath {
            field: field.to_owned(),
        });
    }
    let pattern = if case_sensitive { pattern } else { pattern.to_lowercase() };
    Ok(RuntimeStringMatcher::new(pattern, predicate))
}

fn validate_component_value(
    value: &str,
    field: &str,
    component: &'static str) -> Result<(), ConfigError> {
    if value.is_empty() {
        Err(ConfigError::EmptyComponentValue {
            field: field.to_owned(),
            component,
        })
    } else {
        Ok(())
    }
}

fn validate_size(size: ConfigSize, field: &str) -> Result<Size2D<i32>, ConfigError> {
    validate_dimension(size.0, &format!("{field}[0]"))?;
    validate_dimension(size.1, &format!("{field}[1]"))?;
    Ok(Size2D::new(size.0, size.1))
}

fn validate_optional_dimension(value: Option<i32>, field: &str) -> Result<(), ConfigError> {
    if let Some(value) = value {
        validate_dimension(value, field)?;
    }
    Ok(())
}

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

fn validate_optional_bounds(
    minimum: Option<i32>,
    maximum: Option<i32>,
    minimum_field: &str,
    maximum_field: &str) -> Result<(), ConfigError> {
    if minimum.zip(maximum).is_some_and(|(minimum, maximum)| minimum > maximum) {
        Err(ConfigError::InvalidBounds {
            minimum_field: minimum_field.to_owned(),
            maximum_field: maximum_field.to_owned(),
        })
    } else {
        Ok(())
    }
}

fn string_equals(input: &str, pattern: &str) -> bool {
    input == pattern
}

fn string_starts_with(input: &str, pattern: &str) -> bool {
    input.starts_with(pattern)
}

fn string_ends_with(input: &str, pattern: &str) -> bool {
    input.ends_with(pattern)
}

fn string_contains(input: &str, pattern: &str) -> bool {
    input.contains(pattern)
}

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
    fn boolean_true_enables_default_actions() {
        let runtime = parse(r#"
            [[rules]]
            name = "app"
            move = true
            resize = true
        "#).validate().expect("Boolean action shorthands must validate");
        let rule = &runtime.rules[0];

        assert_eq!((rule.r#move, rule.resize.enabled), (RuntimeMove::Center, true));
    }

    #[test]
    fn resize_size_compiles_to_primary_target() {
        let runtime = parse(r#"
            [[rules]]
            name = "app"
            resize = [1440, 900]
        "#).validate().expect("resize tuple must validate");

        assert_eq!(
            runtime.rules[0].resize.primary_size(),
            Some(Size2D::new(1440, 900)));
    }

    #[test]
    fn incomplete_resize_target_is_ignored() {
        let runtime = parse(r#"
            [[rules]]
            name = "app"
            resize.enabled = true
            resize.target_width = 1440
        "#).validate().expect("incomplete target is a warning, not an error");

        assert_eq!(runtime.rules[0].resize.primary_size(), None);
    }

    #[test]
    fn description_is_trimmed_without_empty_validation() {
        let runtime = parse(r#"
            [[rules]]
            name = "app"
            description = "   "
        "#).validate().expect("empty trimmed description remains valid");

        assert_eq!(runtime.rules[0].description.as_deref(), Some(""));
    }

    #[test]
    fn selector_limits_filter_manifest_sizes() {
        let runtime = parse(r#"
            [[rules]]
            name = "app"
            resize.enabled = true
            resize.min_width = 1400
            resize.max_height = 1000
        "#).validate().expect("selector limits must validate");
        let resize = runtime.rules[0].resize;

        assert_eq!(
            (
                resize.allows_selector_size(Size2D::new(1440, 900)),
                resize.allows_selector_size(Size2D::new(1280, 800))),
            (true, false));
    }

    #[test]
    fn component_matcher_ands_every_predicate() {
        let runtime = parse(r#"
            [[rules]]
            name = "tool"
            match.window.title.starts_with = "Tool"
            match.window.title.ends_with = "Ready"
            match.window.title.contains = " - "
        "#).validate().expect("component matcher must validate");
        let rule = &runtime.rules[0];

        assert!(rule.matches(None, "c:/tool.exe", "Tool - Ready", None));
    }

    #[test]
    fn bare_string_matcher_is_exact() {
        let runtime = parse(r#"
            [[rules]]
            name = "tool"
            match.window.title = "Tool"
        "#).validate().expect("bare matcher must validate");

        assert!(!runtime.rules[0].matches(None, "c:/tool.exe", "Tool Window", None));
    }

    #[test]
    fn empty_component_matcher_is_rejected() {
        let config = parse(r#"
            [[rules]]
            name = "tool"
            match.window.title = {}
        "#);

        assert_eq!(
            config.validate().expect_err("empty component matcher must fail"),
            ConfigError::EmptyComponentMatcher {
                field: "rules[0].match.window.title".to_owned(),
            });
    }

    #[test]
    fn executable_matchers_are_case_insensitive() {
        let runtime = parse(r#"
            [[rules]]
            name = "tool"
            match.executable.name = "TOOL.EXE"
            match.executable.path.ends_with = "/TOOL.EXE"
        "#).validate().expect("executable matcher must validate");

        assert!(runtime.rules[0].matches(
            Some("tool.exe"),
            "c:/apps/tool.exe",
            "Tool",
            None));
    }

    #[test]
    fn executable_path_matcher_rejects_backslashes() {
        let config = parse(r#"
            [[rules]]
            name = "tool"
            match.executable.path = 'C:\Apps\tool.exe'
        "#);

        assert_eq!(
            config.validate().expect_err("backslash path must fail"),
            ConfigError::BackslashInExecutablePath {
                field: "rules[0].match.executable.path".to_owned(),
            });
    }

    #[test]
    fn window_title_matchers_are_case_sensitive() {
        let runtime = parse(r#"
            [[rules]]
            name = "tool"
            match.window.title = "Tool"
        "#).validate().expect("title matcher must validate");

        assert!(!runtime.rules[0].matches(None, "c:/tool.exe", "tool", None));
    }

    #[test]
    fn matching_rule_prefers_higher_priority_over_source_order() {
        let runtime = parse(r#"
            [[rules]]
            name = "first"
            match.priority = 0

            [[rules]]
            name = "second"
            match.priority = 10
        "#).validate().expect("rules must validate");

        assert_eq!(runtime.matching_rule_index(None, "c:/app.exe", "App", None), Some(1));
    }

    #[test]
    fn matching_rule_uses_source_order_for_equal_priority() {
        let runtime = parse(r#"
            [[rules]]
            name = "first"
            match.priority = 10

            [[rules]]
            name = "second"
            match.priority = 10
        "#).validate().expect("rules must validate");

        assert_eq!(runtime.matching_rule_index(None, "c:/app.exe", "App", None), Some(0));
    }

    #[test]
    fn size_constrained_rule_rejects_missing_client_size() {
        let runtime = parse(r#"
            [[rules]]
            name = "large"
            match.window.min_size = [640, 480]
        "#).validate().expect("size matcher must validate");

        assert_eq!(runtime.matching_rule_index(None, "c:/app.exe", "App", None), None);
    }

    #[test]
    fn size_bounds_are_inclusive() {
        let runtime = parse(r#"
            [[rules]]
            name = "bounded"
            match.window.min_size = [640, 480]
            match.window.max_size = [640, 480]
        "#).validate().expect("equal inclusive bounds must validate");

        assert_eq!(
            runtime.matching_rule_index(
                None,
                "c:/app.exe",
                "App",
                Some(Size2D::new(640, 480))),
            Some(0));
    }

    #[test]
    fn reversed_window_size_bounds_are_rejected() {
        let config = parse(r#"
            [[rules]]
            name = "bounded"
            match.window.min_size = [800, 480]
            match.window.max_size = [640, 1080]
        "#);

        assert_eq!(
            config.validate().expect_err("reversed bounds must fail"),
            ConfigError::InvalidBounds {
                minimum_field: "rules[0].match.window.min_size[0]".to_owned(),
                maximum_field: "rules[0].match.window.max_size[0]".to_owned(),
            });
    }

    #[test]
    fn future_regex_matcher_is_rejected_by_deserialization() {
        let result = toml::from_str::<Config>(r#"
            [[rules]]
            name = "future"
            match.window.title.regex = ".*"
        "#);

        result.expect_err("future matcher form must not deserialize");
    }

    #[test]
    fn future_glob_matcher_is_rejected_by_deserialization() {
        let result = toml::from_str::<Config>(r#"
            [[rules]]
            name = "future"
            match.executable.name.glob = "*.exe"
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
}
