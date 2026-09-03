//! TOML parsing and validated rule compilation for platform-independent configuration.
//!
//! This module owns the transition from serialized [`Config`] values to compiled
//! [`RuntimeRule`] values.
//! Malformed documents fail as a whole, while invalid rules are diagnosed and excluded
//! independently. Filesystem access and report logging remain caller concerns.

use std::collections::BTreeSet;

use euclid::default::Size2D;
use serde::Deserialize;
use smol_str::{SmolStr, StrExt as _, format_smolstr};
use thiserror::Error;

use crate::{
    Config, Pattern, PatternMatcher, ProgramFilter, ResizeRule, ResizeSelector, Rule,
    RuntimeRule, WindowFilter,
};

/// Usable rules and source-ordered diagnostics for the rules that were excluded.
#[derive(Debug, Default)]
pub struct ConfigReport {
    /// Successfully compiled rules, retaining their relative declaration order.
    pub rules: Vec<RuntimeRule>,
    /// One error per rejected rule; invalid rules are never partially applied.
    pub diagnostics: Vec<RuleDiagnostic>,
}

/// A rejected rule's original position and the reason it could not be used.
#[derive(Debug)]
pub struct RuleDiagnostic {
    /// Zero-based position in the source, before invalid rules are removed.
    pub index: usize,
    /// Structural or semantic failure, without a TOML source excerpt.
    pub error: ConfigError,
}

/// Checks the document envelope without deserializing every rule as one transaction.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigDocument {
    /// Raw values allow a malformed rule to be reported independently of its neighbors.
    #[serde(default)]
    rules: Vec<toml::Value>,
}

/// Parses TOML and compiles each rule independently, without reading or writing files.
///
/// Returns an error for malformed TOML or an invalid top-level structure. Individual
/// rule errors are returned in the report. Empty documents are valid. Error chains
/// retain parser diagnostics and locations but omit potentially private source excerpts.
pub fn parse_config(source: &str) -> anyhow::Result<ConfigReport> {
    let document: ConfigDocument =
        toml::from_str(source).map_err(|mut error: toml::de::Error| {
            let location = error
                .span()
                .and_then(|span| source.get(..span.start))
                .map(|prefix| {
                    let line = prefix.bytes().filter(|&byte| byte == b'\n').count() + 1;
                    let column = prefix
                        .rsplit('\n')
                        .next()
                        .unwrap_or_default()
                        .chars()
                        .count()
                        + 1;
                    format_smolstr!("invalid configuration at line {line}, column {column}")
                })
                .unwrap_or_else(|| SmolStr::new_static("invalid configuration document"));
            // Display normally includes the offending source line, which may be private.
            error.set_input(None);
            anyhow::Error::new(error).context(location)
        })?;
    Ok(compile_rules(document.rules.into_iter().map(|value| {
        value.try_into().map_err(ConfigError::Deserialize)
    })))
}

/// Compiles already deserialized config, excluding invalid rules and later duplicate names.
/// Only successfully compiled rules reserve their name; a broken rule cannot shadow a valid one.
pub fn compile_config(config: Config) -> ConfigReport {
    compile_rules(config.rules.into_iter().map(Ok))
}

/// Shares rule recovery between typed configs and independently deserialized TOML rules.
fn compile_rules(rules: impl Iterator<Item = Result<Rule, ConfigError>>) -> ConfigReport {
    let mut names = BTreeSet::new();
    let mut report = ConfigReport::default();
    for (index, rule) in rules.enumerate() {
        let rule = rule.and_then(|rule| {
            if names.contains(&rule.name) {
                return Err(ConfigError::DuplicateRuleName { name: rule.name });
            }
            compile_rule(index, rule)
        });
        match rule {
            Ok(rule) => {
                names.insert(rule.name.clone());
                report.rules.push(rule);
            }
            Err(error) => report.diagnostics.push(RuleDiagnostic { index, error }),
        }
    }
    report
}

/// Returns whether a name is a lowercase sequence of TOML-style dotted bare keys.
pub fn is_valid_rule_name(name: &str) -> bool {
    !name.is_empty()
        && name.split('.').all(|component| {
            !component.is_empty()
                && component.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || byte == b'_'
                        || byte == b'-'
                })
        })
}

/// A structural or semantic failure that excludes one configuration rule.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    /// A rule did not conform to the serialized config types.
    #[error(transparent)]
    Deserialize(#[from] toml::de::Error),
    /// A rule name did not satisfy the lowercase dotted bare-key grammar.
    #[error("rules[{index}].name '{name}' must match [a-z0-9_-]+(?:\\.[a-z0-9_-]+)*")]
    InvalidRuleName {
        /// Zero-based rule index.
        index: usize,
        /// Invalid configured name.
        name: SmolStr,
    },
    /// The same rule name appeared more than once.
    #[error("duplicate rule name '{name}'")]
    DuplicateRuleName {
        /// Duplicate configured name.
        name: SmolStr,
    },
    /// A partial matcher contained no predicates.
    #[error("{field} must contain starts_with, ends_with, or contains")]
    EmptyPartialMatcher {
        /// Configuration field containing the invalid matcher.
        field: SmolStr,
    },
    /// A configured program-path pattern used a backslash.
    #[error("{field} must use forward slashes; backslashes are not accepted")]
    BackslashInProgramPath {
        /// Configuration field containing the invalid path pattern.
        field: SmolStr,
    },
    /// A configured dimension was not positive.
    #[error("{field} must be positive, found {value}")]
    InvalidDimension {
        /// Configuration field containing the invalid dimension.
        field: SmolStr,
        /// Invalid configured value.
        value: i32,
    },
    /// A minimum dimension exceeded its corresponding maximum.
    #[error("{minimum_field} must not exceed {maximum_field}")]
    InvalidBounds {
        /// Configuration field containing the minimum.
        minimum_field: SmolStr,
        /// Configuration field containing the maximum.
        maximum_field: SmolStr,
    },
}

/// Resolves defaults and compiles one rule, retaining source-order diagnostics.
fn compile_rule(index: usize, rule: Rule) -> Result<RuntimeRule, ConfigError> {
    if !is_valid_rule_name(&rule.name) {
        return Err(ConfigError::InvalidRuleName {
            index,
            name: rule.name,
        });
    }
    let prefix = format_smolstr!("rules[{index}]");
    let description = rule.description.trim();
    let description = (!description.is_empty()).then(|| SmolStr::new(description));
    let (resize_exact, resize_selector) = compile_resize(rule.resize, &prefix)?;
    let program_filters = compile_program_match(rule.program, &prefix)?;
    let window_filters = compile_window_match(rule.window, &prefix)?;

    Ok(RuntimeRule {
        name: rule.name,
        description,
        relocate: rule.relocate,
        resize_exact,
        resize_selector,
        program_filters,
        window_filters,
        priority: rule.priority,
    })
}

/// Compiles serialized resize forms into mutually exclusive exact and selector actions.
/// Array shorthand remains a selector so it keeps an unbounded size menu.
fn compile_resize(
    resize: ResizeRule,
    prefix: &str,
) -> Result<(Option<Size2D<i32>>, Option<ResizeSelector>), ConfigError> {
    match resize {
        ResizeRule::Boolean(false) => Ok((None, None)),
        ResizeRule::Boolean(true) => Ok((None, Some(ResizeSelector::default()))),
        ResizeRule::Exact { exact } => {
            validate_size(exact, &format_smolstr!("{prefix}.resize.exact"))?;
            Ok((Some(Size2D::from(exact)), None))
        }
        ResizeRule::SelectorDefault(default) => {
            validate_size(default, &format_smolstr!("{prefix}.resize"))?;
            Ok((
                None,
                Some(ResizeSelector {
                    default: Some(default),
                    ..Default::default()
                }),
            ))
        }
        ResizeRule::Selector(selector) => {
            if let Some(size) = selector.default {
                validate_size(size, &format_smolstr!("{prefix}.resize.default"))?;
            }
            validate_size_bounds(selector.min, selector.max, &format_smolstr!("{prefix}.resize"))?;
            Ok((None, Some(selector)))
        }
    }
}

/// Compiles case-insensitive program patterns without normalizing config paths.
fn compile_program_match(
    matcher: ProgramFilter<Pattern>,
    prefix: &str,
) -> Result<ProgramFilter<Vec<PatternMatcher>>, ConfigError> {
    let name = matcher
        .name
        .map(|matcher| {
            compile_string_matcher(matcher, &format_smolstr!("{prefix}.program.name"), false, false)
        })
        .transpose()?;
    let path = matcher
        .path
        .map(|matcher| {
            compile_string_matcher(matcher, &format_smolstr!("{prefix}.program.path"), false, true)
        })
        .transpose()?;
    Ok(ProgramFilter { name, path })
}

/// Compiles case-sensitive title patterns and validates inclusive size bounds.
fn compile_window_match(
    matcher: WindowFilter<Pattern>,
    prefix: &str,
) -> Result<WindowFilter<Vec<PatternMatcher>>, ConfigError> {
    let title = matcher
        .title
        .map(|matcher| {
            compile_string_matcher(matcher, &format_smolstr!("{prefix}.window.title"), true, false)
        })
        .transpose()?;
    validate_size_bounds(matcher.min, matcher.max, &format_smolstr!("{prefix}.window"))?;
    Ok(WindowFilter {
        title,
        min: matcher.min,
        max: matcher.max,
    })
}

/// Selects exact or ANDed partial predicates, rejecting empty partial patterns.
fn compile_string_matcher(
    mut matcher: Pattern,
    field: &str,
    case_sensitive: bool,
    is_path: bool,
) -> Result<Vec<PatternMatcher>, ConfigError> {
    match matcher {
        Pattern::Exact(ref mut pattern) => {
            normalize_pattern(pattern, field, case_sensitive, is_path)?;
        }
        Pattern::Partial {
            ref mut starts_with,
            ref mut ends_with,
            ref mut contains,
        } => {
            if starts_with.is_empty() && ends_with.is_empty() && contains.is_empty() {
                return Err(ConfigError::EmptyPartialMatcher {
                    field: field.into(),
                });
            }
            for (name, pattern) in [
                ("starts_with", starts_with),
                ("ends_with", ends_with),
                ("contains", contains),
            ] {
                normalize_pattern(
                    pattern,
                    &format_smolstr!("{field}.{name}"),
                    case_sensitive,
                    is_path)?;
            }
        }
    }
    Ok(matcher.to_matchers())
}

/// Validates path separators and folds only case-insensitive patterns at load time.
fn normalize_pattern(
    pattern: &mut SmolStr,
    field: &str,
    case_sensitive: bool,
    is_path: bool,
) -> Result<(), ConfigError> {
    if is_path && pattern.contains('\\') {
        return Err(ConfigError::BackslashInProgramPath {
            field: field.into(),
        });
    }
    if !case_sensitive {
        *pattern = pattern.to_lowercase_smolstr();
    }
    Ok(())
}

/// Validates both array dimensions using their configuration indices.
fn validate_size([width, height]: [i32; 2], field: &str) -> Result<(), ConfigError> {
    validate_dimension(width, &format_smolstr!("{field}[0]"))?;
    validate_dimension(height, &format_smolstr!("{field}[1]"))
}

/// Checks positive bounds and rejects inverted axes when both bounds are present.
fn validate_size_bounds(
    min: Option<[i32; 2]>,
    max: Option<[i32; 2]>,
    prefix: &str,
) -> Result<(), ConfigError> {
    if let Some(size) = min {
        validate_size(size, &format_smolstr!("{prefix}.min"))?;
    }
    if let Some(size) = max {
        validate_size(size, &format_smolstr!("{prefix}.max"))?;
    }
    if let (Some(min), Some(max)) = (min, max) {
        for (axis, (minimum, maximum)) in min.into_iter().zip(max).enumerate() {
            if minimum > maximum {
                return Err(ConfigError::InvalidBounds {
                    minimum_field: format_smolstr!("{prefix}.min[{axis}]"),
                    maximum_field: format_smolstr!("{prefix}.max[{axis}]"),
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
            field: field.into(),
            value,
        })
    } else {
        Ok(())
    }
}
