use serde::*;
use smol_str::SmolStr;

/// Omits empty partial components without introducing an always-true predicate.
fn none_if_default<T: Default + PartialEq>(value: T) -> Option<T> {
    if value == T::default() {
        None
    } else {
        Some(value)
    }
}

/// An immutable literal pattern paired with its case-sensitive string predicate.
///
/// Compiled literals use [`SmolStr`] because they are cloned into long-lived runtime
/// rules but are usually short enough to remain inline.
#[derive(Debug, Clone)]
pub struct PatternMatcher(
    /// Literal text, already normalized by the config compiler when appropriate.
    SmolStr,
    /// Predicate chosen once during compilation.
    fn(input: &str, pattern: &str) -> bool);

impl PatternMatcher {
    /// Returns whether the candidate satisfies this predicate.
    pub fn matches(&self, input: &str) -> bool { (self.1)(input, self.0.as_str()) }
}

/// An exact string or a conjunction of nonempty literal partial patterns.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[derive(Deserialize, Serialize)]
#[derive(schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(untagged)]
pub enum Pattern {
    /// Exact match against the entire string.
    Exact(String),
    /// Several non-empty partial predicates which are ANDed.
    Partial {
        /// Required prefix, or an empty string when omitted.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        starts_with: String,
        /// Required suffix, or an empty string when omitted.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        ends_with: String,
        /// Required substring, or an empty string when omitted.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        contains: String,
    }
}

impl Pattern {
    /// Compiles literal, case-sensitive predicates without validating the pattern.
    ///
    /// Callers must normalize both patterns and candidates for case-insensitive
    /// matching. An empty partial pattern returns no predicates; config validation
    /// must reject it before using an all-predicates match.
    pub fn to_matchers(&self) -> Vec<PatternMatcher> {
        use std::iter;

        match *self {
            Self::Exact(ref t) =>
                iter::once(PatternMatcher(t.as_str().into(), |s, t| s == t))
                    .collect(),
            Self::Partial { ref starts_with, ref ends_with, ref contains } => {
                iter::empty()
                    .chain(
                        none_if_default(starts_with.as_str())
                            .map(|t| PatternMatcher(t.into(), |s, t| s.starts_with(t))))
                    .chain(
                        none_if_default(ends_with.as_str())
                            .map(|t| PatternMatcher(t.into(), |s, t| s.ends_with(t))))
                    .chain(
                        none_if_default(contains.as_str())
                            .map(|t| PatternMatcher(t.into(), |s, t| s.contains(t))))
                    .collect()
            }
        }
    }
}
