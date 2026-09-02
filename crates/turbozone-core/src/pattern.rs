use serde::*;
use smol_str::SmolStr;

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
///
/// Serialized and compiled literals share [`SmolStr`] so parsing, validation, and
/// runtime matching keep one owned representation without changing their wire format.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[derive(Deserialize, Serialize)]
#[derive(schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(untagged)]
pub enum Pattern {
    /// Exact match against the entire string.
    Exact(SmolStr),
    /// Several non-empty partial predicates which are ANDed.
    Partial {
        /// Required prefix, or an empty string when omitted.
        #[serde(default, skip_serializing_if = "SmolStr::is_empty")]
        starts_with: SmolStr,
        /// Required suffix, or an empty string when omitted.
        #[serde(default, skip_serializing_if = "SmolStr::is_empty")]
        ends_with: SmolStr,
        /// Required substring, or an empty string when omitted.
        #[serde(default, skip_serializing_if = "SmolStr::is_empty")]
        contains: SmolStr,
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
                iter::once(PatternMatcher(t.clone(), |s, t| s == t))
                    .collect(),
            Self::Partial { ref starts_with, ref ends_with, ref contains } => {
                iter::empty()
                    .chain(
                        (!starts_with.is_empty())
                            .then(|| PatternMatcher(starts_with.clone(), |s, t| s.starts_with(t))))
                    .chain(
                        (!ends_with.is_empty())
                            .then(|| PatternMatcher(ends_with.clone(), |s, t| s.ends_with(t))))
                    .chain(
                        (!contains.is_empty())
                            .then(|| PatternMatcher(contains.clone(), |s, t| s.contains(t))))
                    .collect()
            }
        }
    }
}
