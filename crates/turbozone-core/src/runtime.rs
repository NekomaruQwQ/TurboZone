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

    // --- Filters ----
    /// Predicates applied to program metadata.
    pub program_filters:
        ProgramFilter<Vec<PatternMatcher>>,
    /// Predicates applied to window metadata.
    pub window_filters:
        WindowFilter<Vec<PatternMatcher>>,
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
    /// Returns whether every configured filter accepts the supplied window data.
    ///
    /// Program names and paths must already be lowercased. Window titles remain
    /// case-sensitive and must retain their native casing.
    pub fn matches(
        &self,
        program_name: Option<&str>,
        program_path: &str,
        window_title: &str,
        client_size: Option<Size2D<i32>>) -> bool {
        self.matches_program(program_name, program_path)
            && self.matches_window(window_title, client_size)
    }

    fn matches_program(&self, name: Option<&str>, path: &str) -> bool {
        let matcher = &self.program_filters;
        let name_matches = matcher.name.as_ref().is_none_or(|predicates| {
            name.is_some_and(|name| predicates.iter().all(|predicate| predicate.matches(name)))
        });
        name_matches && matcher.path.as_ref().is_none_or(|predicates| {
            predicates.iter().all(|predicate| predicate.matches(path))
        })
    }

    fn matches_window(&self, title: &str, size: Option<Size2D<i32>>) -> bool {
        let matcher = &self.window_filters;
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
        matcher.min.is_none_or(|[min_width, min_height]| {
            size.width >= min_width && size.height >= min_height
        }) && matcher.max.is_none_or(|[max_width, max_height]| {
            size.width <= max_width && size.height <= max_height
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
    /// Program names and paths must already be lowercased. Window titles remain
    /// case-sensitive and must retain their native casing.
    pub fn matching_rule_index(
        &self,
        program_name: Option<&str>,
        program_path: &str,
        window_title: &str,
        client_size: Option<Size2D<i32>>) -> Option<usize> {
        let mut winner = None;
        for (index, rule) in self.rules.iter().enumerate() {
            if !rule.matches(
                program_name,
                program_path,
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
