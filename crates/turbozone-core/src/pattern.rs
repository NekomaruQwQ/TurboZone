use serde::*;

fn none_if_default<T: Default + PartialEq>(value: T) -> Option<T> {
    if value == T::default() {
        None
    } else {
        Some(value)
    }
}

#[derive(Debug, Clone)]
pub struct PatternMatcher(
    String,
    fn(input: &str, pattern: &str) -> bool);

impl PatternMatcher {
    pub(crate) const fn new(
        pattern: String,
        predicate: fn(&str, &str) -> bool) -> Self {
        Self(pattern, predicate)
    }

    /// Returns whether the candidate satisfies this predicate.
    pub fn matches(&self, input: &str) -> bool {
        (self.1)(input, self.0.as_str())
    }
}

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
    pub fn to_matchers(&self) -> Vec<PatternMatcher> {
        use std::iter;

        match self {
            Self::Exact(t) =>
                iter::once(PatternMatcher(t.clone(), |s, t| s == t))
                    .collect(),
            Self::Partial { starts_with, ends_with, contains } => {
                iter::empty()
                    .chain(
                        none_if_default(starts_with.clone())
                            .map(|t| PatternMatcher(t, |s, t| s.starts_with(t))))
                    .chain(
                        none_if_default(ends_with.clone())
                            .map(|t| PatternMatcher(t, |s, t| s.ends_with(t))))
                    .chain(
                        none_if_default(contains.clone())
                            .map(|t| PatternMatcher(t, |s, t| s.contains(t))))
                    .collect()
            }
        }
    }
}
