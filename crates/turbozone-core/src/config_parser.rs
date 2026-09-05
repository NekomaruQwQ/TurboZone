//! TOML parsing and non-mutating verification for platform-independent configuration.
//!
//! This module owns structural and semantic acceptance of authored [`Config`] values.
//! Matching and presentation interpret verified rules directly without a compiled model.
//! Deserialization and semantic validation are transactional: any error rejects the
//! complete configuration and is logged here before control returns to the caller.
//! A well-formed default outside resize bounds only warns; runtime queries suppress
//! that target without modifying the authored config. Filesystem access remains a caller concern.

use std::collections::BTreeSet;

use euclid::default::Size2D;
use smol_str::*;

use crate::*;

/// Parses and validates a complete TOML configuration without performing filesystem I/O.
///
/// The whole serialized [`Config`] is deserialized before semantic validation begins.
/// Any error is logged and returns `None`; valid rules are never exposed beside invalid
/// siblings. Empty documents remain valid configurations with no rules.
pub fn parse_config(source: &str) -> Option<Config> {
    // The streaming TOML deserializer does not reject trailing elements in fixed-size
    // arrays. Converting one complete value tree preserves the exact `[width, height]`
    // contract without recovering or verifying rules independently.
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
    verify_config(&config)?;
    Some(config)
}

/// Verifies all rules without modifying authored text, resize forms, or source order.
///
/// Returns `Some(())` only when every rule is usable. The first failure is logged
/// with its field context and returns `None`; callers must reject the whole config.
/// Deserialized or manually constructed configurations use the same semantic checks.
/// Out-of-bounds defaults warn once per affected rule per verification and remain authored.
/// Matching, normalization for comparisons, and filesystem access belong elsewhere.
pub fn verify_config(config: &Config) -> Option<()> {
    let mut names = BTreeSet::new();
    for (index, rule) in config.rules.iter().enumerate() {
        if !names.insert(rule.name.as_str()) {
            log::error!("duplicate rule name '{}'", rule.name);
            return None;
        }
        verify_rule(index, rule)?;
    }
    Some(())
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

/// Checks rule identity before its action and filter constraints, preserving diagnostics.
fn verify_rule(index: usize, rule: &Rule) -> Option<()> {
    if !is_valid_rule_name(&rule.name) {
        log::error!(
            "rules[{index}].name '{}' must match [a-z0-9_-]+(?:\\.[a-z0-9_-]+)*",
            rule.name);
        return None;
    }
    let prefix = format_smolstr!("rules[{index}]");
    validate_resize(&rule.resize, &prefix, &rule.name)?;
    validate_program_match(&rule.program, &prefix)?;
    validate_window_match(&rule.window, &prefix)
}

/// Checks every configured resize dimension while preserving its serialized variant.
/// Invalid dimensions or inverted bounds reject the config. A well-formed default outside
/// those bounds is recoverable: warn here and let the primary-size query suppress it.
fn validate_resize(resize: &ResizeRule, prefix: &str, rule_name: &str) -> Option<()> {
    match *resize {
        ResizeRule::Boolean(_) => Some(()),
        ResizeRule::Exact { exact } => {
            validate_size(exact, &format_smolstr!("{prefix}.resize.exact"))
        }
        ResizeRule::SelectorDefault(default) => {
            validate_size(default, &format_smolstr!("{prefix}.resize"))
        }
        ResizeRule::Selector(ref selector) => {
            if let Some(size) = selector.default {
                validate_size(size, &format_smolstr!("{prefix}.resize.default"))?;
            }
            validate_size_bounds(selector.min, selector.max, &format_smolstr!("{prefix}.resize"))?;
            if let Some(size) = selector.default && !selector.allows_size(size) {
                log::warn!(
                    "{prefix}.resize.default {:?} for rule '{rule_name}' is outside resize bounds \
                     (min: {:?}, max: {:?}); ignoring default",
                    size.to_array(),
                    selector.min.map(Size2D::to_array),
                    selector.max.map(Size2D::to_array));
            }
            Some(())
        }
    }
}

/// Checks executable patterns without changing authored case or path separators.
fn validate_program_match(matcher: &ProgramFilter, prefix: &str) -> Option<()> {
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
fn validate_window_match(matcher: &WindowFilter, prefix: &str) -> Option<()> {
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

/// Validates geometry dimensions while retaining indices from the serialized pair
/// in diagnostics so users can locate the invalid configuration component.
fn validate_size(size: Size2D<i32>, field: &str) -> Option<()> {
    validate_dimension(size.width, &format_smolstr!("{field}[0]"))?;
    validate_dimension(size.height, &format_smolstr!("{field}[1]"))
}

/// Checks supported bounds and rejects inverted axes when both bounds are present.
fn validate_size_bounds(
    min: Option<Size2D<i32>>,
    max: Option<Size2D<i32>>,
    prefix: &str) -> Option<()> {
    if let Some(size) = min {
        validate_size(size, &format_smolstr!("{prefix}.min"))?;
    }
    if let Some(size) = max {
        validate_size(size, &format_smolstr!("{prefix}.max"))?;
    }
    if let (Some(min), Some(max)) = (min, max) {
        for (axis, (minimum, maximum)) in min.to_array().into_iter().zip(max.to_array()).enumerate() {
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
