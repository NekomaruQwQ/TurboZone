use serde::*;
use educe::Educe;
use euclid::default::Size2D;
use schemars::*;
use smol_str::SmolStr;

/// Returns whether a value is the default value of its type.
///
/// This is useful for `skip_serializing_if` and `skip_deserializing_if` attributes
/// on serde fields.
fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    value == &T::default()
}

/// The serialized top-level configuration for TurboZone.
#[derive(Debug, Clone)]
#[derive(Default)]
#[derive(JsonSchema)]
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Rules in source order.
    #[serde(default)]
    pub rules: Vec<Rule>,
}

/// One serialized rule pairing window filters with the actions they enable.
#[derive(Debug, Clone)]
#[derive(Default)]
#[derive(JsonSchema)]
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    // ---- Metadata ----
    /// Unique stable identifier retained in the same compact representation at runtime.
    pub name: SmolStr,
    /// Compact user-facing section name, falling back to the rule name when empty.
    #[serde(default, skip_serializing_if = "is_default")]
    pub description: SmolStr,

    // ---- Filters ----
    /// Higher priorities take precedence; zero is the default.
    #[serde(default, skip_serializing_if = "is_default")]
    pub priority: i64,
    /// Program filters.
    #[serde(default, skip_serializing_if = "is_default")]
    pub program: ProgramFilter<Pattern>,
    /// Window filters.
    #[serde(default, skip_serializing_if = "is_default")]
    pub window: WindowFilter<Pattern>,

    // ---- Actions ----
    /// Relocation controls, currently only the "center" button.
    #[serde(default, skip_serializing_if = "is_default")]
    #[serde(rename = "move")]
    pub relocate: bool,
    /// Resize controls, target, and selector limits.
    #[serde(default, skip_serializing_if = "is_default")]
    pub resize: ResizeRule,
}

/// A validated rule ready for matching and UI rendering.
///
/// This compiled counterpart to [`Rule`] keeps runtime-only predicates beside the
/// serialized configuration contract while leaving rule selection to the engine.
#[derive(Debug)]
pub struct RuntimeRule {
    // --- Metadata ----
    /// Stable unique rule identifier.
    pub name: SmolStr,
    /// Trimmed user-facing section name, when nonempty.
    pub description: Option<SmolStr>,

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
    pub resize_selector: Option<ResizeSelector>,
}

/// Complete serialized resize behavior.
#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(Educe)]
#[derive(JsonSchema)]
#[derive(Deserialize, Serialize)]
#[educe(Default)]
#[serde(deny_unknown_fields)]
#[serde(untagged)]
pub enum ResizeRule {
    /// Resizing disabled or selector enabled without any size filters.
    #[educe(Default)]
    Boolean(bool),
    /// Exact target size. The selector is disabled in this mode.
    Exact {
        /// Positive client-area `[width, height]` in physical pixels, independent of selector limits.
        #[schemars(inner(range(min = 1, max = i32::MAX)))]
        exact: [i32; 2],
    },
    /// Selector properties with only a default size, no minimum, and no maximum.
    SelectorDefault(
        #[schemars(inner(range(min = 1, max = i32::MAX)))]
        [i32; 2],
    ),
    /// Selector properties, including optional default, minimum, and maximum sizes.
    Selector(ResizeSelector),
}

/// Selector defaults and inclusive bounds; omitted bounds are unrestricted.
#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(Default)]
#[derive(JsonSchema)]
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResizeSelector {
    /// Primary `[width, height]` in physical pixels, independent of selector bounds.
    pub default: Option<[i32; 2]>,
    /// Minimum `[width, height]` offered by the selector, in physical pixels.
    pub min: Option<[i32; 2]>,
    /// Maximum `[width, height]` offered by the selector, in physical pixels.
    pub max: Option<[i32; 2]>,
}

impl ResizeSelector {
    /// Returns whether a positive size is within all configured selector bounds.
    pub const fn allows_size(&self, size: Size2D<i32>) -> bool {
        if size.width <= 0 || size.height <= 0 {
            return false;
        }

        if let Some([min_width, min_height]) = self.min && (
            size.width < min_width ||
            size.height < min_height) {
            return false;
        }

        if let Some([max_width, max_height]) = self.max && (
            size.width > max_width ||
            size.height > max_height) {
            return false;
        }

        true
    }
}

/// Program filters using serialized patterns or compiled predicates.
#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(Educe)]
#[derive(JsonSchema)]
#[derive(Deserialize, Serialize)]
#[educe(Default)]
#[serde(deny_unknown_fields)]
pub struct ProgramFilter<S> {
    /// Optional case-insensitive filename matcher.
    #[educe(Default = None)]
    pub name: Option<S>,
    /// Optional case-insensitive path matcher.
    #[educe(Default = None)]
    pub path: Option<S>,
}

/// Window filters using serialized patterns or compiled predicates.
#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(Educe)]
#[derive(JsonSchema)]
#[derive(Deserialize, Serialize)]
#[educe(Default)]
#[serde(deny_unknown_fields)]
pub struct WindowFilter<S> {
    /// Optional case-sensitive window-title matcher.
    #[educe(Default = None)]
    pub title: Option<S>,
    /// Inclusive minimum client-area `[width, height]` in positive physical pixels.
    #[educe(Default = None)]
    pub min: Option<[i32; 2]>,
    /// Inclusive maximum client-area `[width, height]` in positive physical pixels.
    #[educe(Default = None)]
    pub max: Option<[i32; 2]>,
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
