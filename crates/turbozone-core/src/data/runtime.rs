use crate::prelude::*;
use super::*;

/// A validated rule ready for matching and UI rendering.
#[derive(Debug)]
pub struct RuntimeRule {
    // --- Metadata ----
    /// Stable unique rule identifier.
    pub name: String,
    /// Trimmed user-facing section name, when nonempty.
    pub description: Option<String>,

    // --- Constraints ----
    /// Predicates applied to executable metadata.
    pub executable_constraints:
        ExecutableConstraint<Vec<PatternMatcher>>,
    /// Predicates applied to window metadata.
    pub window_constraints:
        WindowConstraint<Vec<PatternMatcher>>,
    /// Explicit or default matching priority.
    pub priority: i64,

    // ---- Actions ----
    /// Whether centering controls are available.
    pub relocate: bool,
    /// Optional exact target size, disabling the selector when present.
    pub resize_exact: Option<Size2D<i32>>,
    /// Selector settings, or none when resizing is disabled or exact-only.
    pub resize_limits: Option<ResizeLimits>,
}

impl RuntimeRule {
    /// Returns whether every configured constraint accepts the supplied window data.
    ///
    /// Executable names and paths must already be lowercased. Window titles remain
    /// case-sensitive and must retain their native casing.
    pub fn matches(
        &self,
        executable_name: Option<&str>,
        executable_path: &str,
        window_title: &str,
        client_size: Option<Size2D<i32>>) -> bool {
        self.matches_executable(executable_name, executable_path)
            && self.matches_window(window_title, client_size)
    }

    fn matches_executable(&self, name: Option<&str>, path: &str) -> bool {
        let matcher = &self.executable_constraints;
        let name_matches = matcher.name.as_ref().is_none_or(|predicates| {
            name.is_some_and(|name| predicates.iter().all(|predicate| predicate.matches(name)))
        });
        name_matches && matcher.path.as_ref().is_none_or(|predicates| {
            predicates.iter().all(|predicate| predicate.matches(path))
        })
    }

    fn matches_window(&self, title: &str, size: Option<Size2D<i32>>) -> bool {
        let matcher = &self.window_constraints;
        if !matcher.title.as_ref().is_none_or(|predicates| {
            predicates.iter().all(|predicate| predicate.matches(title))
        }) {
            return false;
        }
        if matcher.min.is_none() && matcher.max.is_none() {
            return true;
        }
        let Some(size) = size else {
            return false;
        };
        matcher.min.is_none_or(|minimum| {
            size.width >= minimum.width && size.height >= minimum.height
        }) && matcher.max.is_none_or(|maximum| {
            size.width <= maximum.width && size.height <= maximum.height
        })
    }
}

/// Validated rules retained in source order.
#[derive(Debug, Default)]
pub struct RuntimeConfig {
    /// Runtime rules in configuration source order.
    pub rules: Vec<RuntimeRule>,
}

impl RuntimeConfig {
    /// Returns the winning rule index, preferring higher priority and then source order.
    ///
    /// Executable names and paths must already be lowercased. Window titles remain
    /// case-sensitive and must retain their native casing.
    pub fn matching_rule_index(
        &self,
        executable_name: Option<&str>,
        executable_path: &str,
        window_title: &str,
        client_size: Option<Size2D<i32>>) -> Option<usize> {
        let mut winner = None;
        for (index, rule) in self.rules.iter().enumerate() {
            if !rule.matches(
                executable_name,
                executable_path,
                window_title,
                client_size) {
                continue;
            }
            if winner.is_none_or(|(_, priority)| rule.priority > priority) {
                winner = Some((index, rule.priority));
            }
        }
        winner.map(|(index, _)| index)
    }
}
