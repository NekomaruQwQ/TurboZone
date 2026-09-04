//! TOML parsing and validated rule compilation for platform-independent configuration.
//!
//! This module owns the transition from serialized [`Config`] values to compiled
//! [`RuntimeRule`] values.
//! Deserialization and semantic validation are transactional: any error rejects the
//! complete configuration and is logged here before control returns to the caller.
//! Filesystem access remains a caller concern.

use std::collections::BTreeSet;

use euclid::default::Size2D;
use smol_str::*;

use crate::*;

/// Parses and validates a complete TOML configuration without performing filesystem I/O.
///
/// The whole serialized [`Config`] is deserialized before semantic validation begins.
/// Any error is logged and returns `None`; valid rules are never exposed beside invalid
/// siblings. Empty documents remain valid configurations with no runtime rules.
pub fn parse_config(source: &str) -> Option<RuntimeConfig> {
    // The streaming TOML deserializer does not reject trailing elements in fixed-size
    // arrays. Converting one complete value tree preserves the exact `[width, height]`
    // contract without recovering or compiling rules independently.
    let config = toml::from_str::<toml::Value>(source)
        .and_then(toml::Value::try_into)
        .map_err(|mut error: toml::de::Error| {
            // TOML normally includes the offending source line in its display text.
            // Configuration contents can be private, so remove that source before logging
            // the parser's explanation.
            error.set_input(None);
            log::error!("failed to deserialize configuration: {error}");
        })
        .ok()?;
    compile_config(config)
}

/// Compiles every rule as one transaction, preserving source order on success.
fn compile_config(config: Config) -> Option<RuntimeConfig> {
    let mut names = BTreeSet::new();
    let mut rules = Vec::with_capacity(config.rules.len());
    for (index, rule) in config.rules.into_iter().enumerate() {
        if !names.insert(rule.name.clone()) {
            log::error!("duplicate rule name '{}'", rule.name);
            return None;
        }
        rules.push(compile_rule(index, rule)?);
    }
    Some(RuntimeConfig { rules })
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

/// Resolves defaults and compiles one rule without exposing partially validated state.
fn compile_rule(index: usize, rule: Rule) -> Option<RuntimeRule> {
    if !is_valid_rule_name(&rule.name) {
        log::error!(
            "rules[{index}].name '{}' must match [a-z0-9_-]+(?:\\.[a-z0-9_-]+)*",
            rule.name);
        return None;
    }
    let prefix = format_smolstr!("rules[{index}]");
    let description = rule.description.trim();
    let description = (!description.is_empty()).then(|| SmolStr::new(description));
    let (resize_exact, resize_selector) = compile_resize(rule.resize, &prefix)?;
    validate_program_match(&rule.program, &prefix)?;
    validate_window_match(&rule.window, &prefix)?;

    Some(RuntimeRule {
        name: rule.name,
        description,
        relocate: rule.relocate,
        resize_exact,
        resize_selector,
        program_filters: rule.program,
        window_filters: rule.window,
        priority: rule.priority,
    })
}

/// Compiles serialized resize forms into mutually exclusive exact and selector actions.
/// Array shorthand remains a selector so it keeps an unbounded size menu.
fn compile_resize(
    resize: ResizeRule,
    prefix: &str,
) -> Option<(Option<Size2D<i32>>, Option<ResizeSelector>)> {
    match resize {
        ResizeRule::Boolean(false) => Some((None, None)),
        ResizeRule::Boolean(true) => Some((None, Some(ResizeSelector::default()))),
        ResizeRule::Exact { exact } => {
            validate_size(exact, &format_smolstr!("{prefix}.resize.exact"))?;
            Some((Some(Size2D::from(exact)), None))
        }
        ResizeRule::SelectorDefault(default) => {
            validate_size(default, &format_smolstr!("{prefix}.resize"))?;
            Some((
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
            Some((None, Some(selector)))
        }
    }
}

/// Checks executable patterns without changing authored case or path separators.
fn validate_program_match(matcher: &ProgramFilter<Pattern>, prefix: &str) -> Option<()> {
    if let Some(ref pattern) = matcher.name {
        validate_pattern(
            pattern,
            &format_smolstr!("{prefix}.program.name"),
            false)?;
    }
    if let Some(ref pattern) = matcher.path {
        validate_pattern(
            pattern,
            &format_smolstr!("{prefix}.program.path"),
            true)?;
    }
    Some(())
}

/// Checks title patterns and inclusive size bounds without interpreting case policy.
fn validate_window_match(matcher: &WindowFilter<Pattern>, prefix: &str) -> Option<()> {
    if let Some(ref pattern) = matcher.title {
        validate_pattern(
            pattern,
            &format_smolstr!("{prefix}.window.title"),
            false)?;
    }
    validate_size_bounds(matcher.min, matcher.max, &format_smolstr!("{prefix}.window"))
}

/// Rejects ineffective partial filters and invalid path literals before engine use.
fn validate_pattern(matcher: &Pattern, field: &str, is_path: bool) -> Option<()> {
    match *matcher {
        Pattern::Exact(ref pattern) => {
            validate_literal(pattern, field, is_path)?;
        }
        Pattern::Partial {
            ref starts_with,
            ref ends_with,
            ref contains,
        } => {
            if starts_with.is_empty() && ends_with.is_empty() && contains.is_empty() {
                log::error!(
                    "{field} must contain starts_with, ends_with, or contains");
                return None;
            }
            for (name, pattern) in [
                ("starts_with", starts_with),
                ("ends_with", ends_with),
                ("contains", contains),
            ] {
                validate_literal(
                    pattern,
                    &format_smolstr!("{field}.{name}"),
                    is_path)?;
            }
        }
    }
    Some(())
}

/// Configured path literals must already use the backend's forward-slash convention.
fn validate_literal(pattern: &str, field: &str, is_path: bool) -> Option<()> {
    if is_path && pattern.contains('\\') {
        log::error!("{field} must use forward slashes; backslashes are not accepted");
        return None;
    }
    Some(())
}

/// Validates both array dimensions using their configuration indices.
fn validate_size([width, height]: [i32; 2], field: &str) -> Option<()> {
    validate_dimension(width, &format_smolstr!("{field}[0]"))?;
    validate_dimension(height, &format_smolstr!("{field}[1]"))
}

/// Checks supported bounds and rejects inverted axes when both bounds are present.
fn validate_size_bounds(
    min: Option<[i32; 2]>,
    max: Option<[i32; 2]>,
    prefix: &str,
) -> Option<()> {
    if let Some(size) = min {
        validate_size(size, &format_smolstr!("{prefix}.min"))?;
    }
    if let Some(size) = max {
        validate_size(size, &format_smolstr!("{prefix}.max"))?;
    }
    if let (Some(min), Some(max)) = (min, max) {
        for (axis, (minimum, maximum)) in min.into_iter().zip(max).enumerate() {
            if minimum > maximum {
                log::error!(
                    "{prefix}.min[{axis}] must not exceed {prefix}.max[{axis}]");
                return None;
            }
        }
    }
    Some(())
}

/// Rejects physical-pixel dimensions outside TurboZone's configuration contract.
fn validate_dimension(value: i32, field: &str) -> Option<()> {
    if !(1..=MAX_SIZE_DIMENSION).contains(&value) {
        log::error!(
            "{field} must be between 1 and {MAX_SIZE_DIMENSION} inclusive, found {value}");
        None
    } else {
        Some(())
    }
}
