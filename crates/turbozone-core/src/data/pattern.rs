use crate::prelude::*;

/// A string pattern paired with its compiled predicate.
#[derive(Debug, Clone)]
pub struct PatternMatcher(
    String,
    fn(input: &str, pattern: &str) -> bool);

impl PatternMatcher {
    /// Pairs a validated pattern with the predicate chosen by the compiler.
    pub(crate) const fn new(pattern: String, predicate: fn(&str, &str) -> bool) -> Self {
        Self(pattern, predicate)
    }

    /// Returns whether the candidate satisfies this predicate.
    pub fn matches(&self, input: &str) -> bool {
        let &Self(ref pattern, predicate) = self;
        (predicate)(input, pattern)
    }
}

/// An exact string matcher or a collection of partial predicates.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[derive(Deserialize, Serialize)]
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

