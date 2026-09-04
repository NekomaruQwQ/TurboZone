use serde::*;
use educe::Educe;
use euclid::default::Size2D;
use schemars::*;
use smol_str::{SmolStr, StrExt as _};

/// Largest physical-pixel dimension accepted from serialized configuration.
///
/// This is a product sanity bound rather than a native-platform guarantee.
/// Window operations remain fallible because compositors and applications
/// may impose lower limits dynamically.
pub const MAX_SIZE_DIMENSION: i32 = 0x4000;

/// Returns whether a value is the default value of its type.
///
/// This is useful for `skip_serializing_if` and `skip_deserializing_if`
/// attributes on serde fields.
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

/// A completely validated configuration ready for matching and UI rendering.
///
/// The parser constructs this boundary only after every serialized rule succeeds,
/// preventing callers from observing a partially usable configuration.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Rules in source order.
    pub rules: Vec<RuntimeRule>,
}

/// One serialized rule pairing window filters with the actions they enable.
#[derive(Debug, Clone)]
#[derive(Default)]
#[derive(JsonSchema)]
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    // ---- Metadata ----
    /// Unique stable identifier retained in the same compact representation
    /// at runtime.
    pub name: SmolStr,
    /// Compact user-facing section name, falling back to the rule name when
    /// empty.
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
/// This counterpart to [`Rule`] resolves display and resize settings while
/// retaining authored patterns. The engine selects each field's case policy.
#[derive(Debug, Clone)]
pub struct RuntimeRule {
    // --- Metadata ----
    /// Stable unique rule identifier.
    pub name: SmolStr,
    /// Trimmed user-facing section name, when nonempty.
    pub description: Option<SmolStr>,

    // --- Filters ----
    /// Authored patterns applied case-insensitively to program metadata.
    pub program_filters: ProgramFilter<Pattern>,
    /// Authored title pattern and client-size bounds.
    pub window_filters: WindowFilter<Pattern>,
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
        /// Supported client-area `[width, height]` in physical pixels,
        /// independent of selector limits.
        #[schemars(inner(range(min = 1, max = MAX_SIZE_DIMENSION)))]
        exact: [i32; 2],
    },
    /// Selector properties with only a default size, no minimum, and
    /// no maximum.
    SelectorDefault(
        #[schemars(inner(range(min = 1, max = MAX_SIZE_DIMENSION)))]
        [i32; 2],
    ),
    /// Selector properties, including optional default, minimum, and
    /// maximum sizes.
    Selector(ResizeSelector),
}

/// Selector defaults and inclusive bounds; omitted bounds are unrestricted.
#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(Default)]
#[derive(JsonSchema)]
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResizeSelector {
    /// Primary `[width, height]` in physical pixels, independent of selector
    /// bounds.
    #[schemars(inner(range(min = 1, max = MAX_SIZE_DIMENSION)))]
    pub default: Option<[i32; 2]>,
    /// Minimum `[width, height]` offered by the selector, in physical pixels.
    #[schemars(inner(range(min = 1, max = MAX_SIZE_DIMENSION)))]
    pub min: Option<[i32; 2]>,
    /// Maximum `[width, height]` offered by the selector, in physical pixels.
    #[schemars(inner(range(min = 1, max = MAX_SIZE_DIMENSION)))]
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

/// Program filters retaining authored patterns for runtime evaluation.
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

/// Window filters retaining authored patterns for runtime evaluation.
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
    /// Inclusive minimum client-area `[width, height]` in supported
    /// physical pixels.
    #[educe(Default = None)]
    #[schemars(inner(range(min = 1, max = MAX_SIZE_DIMENSION)))]
    pub min: Option<[i32; 2]>,
    /// Inclusive maximum client-area `[width, height]` in supported
    /// physical pixels.
    #[educe(Default = None)]
    #[schemars(inner(range(min = 1, max = MAX_SIZE_DIMENSION)))]
    pub max: Option<[i32; 2]>,
}

/// An exact string or a conjunction of nonempty literal partial patterns.
///
/// Authored literals remain unchanged through validation and matching. Callers
/// choose the case policy for their field; comparisons need no compiled predicates.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[derive(schemars::JsonSchema)]
#[derive(Deserialize, Serialize)]
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
    /// Matches literal text case-sensitively, ANDing every nonempty partial component.
    ///
    /// Empty exact strings can match empty input. Entirely empty partial patterns
    /// fail closed even when a caller constructs one without config verification.
    pub fn matches(&self, input: &str) -> bool {
        match *self {
            Self::Exact(ref pattern) => input == pattern,
            Self::Partial { ref starts_with, ref ends_with, ref contains } => {
                !(starts_with.is_empty() && ends_with.is_empty() && contains.is_empty())
                    && (starts_with.is_empty() || input.starts_with(starts_with.as_str()))
                    && (ends_with.is_empty() || input.ends_with(ends_with.as_str()))
                    && (contains.is_empty() || input.contains(contains.as_str()))
            }
        }
    }

    /// Matches using TurboZone's Unicode-aware lowercase conversion on both sides.
    ///
    /// Convert input once per call and literals only when their predicate is needed.
    /// Keeping the existing SmolStr conversion preserves matching semantics without
    /// mutating configured text or substituting ASCII-only or full case folding.
    pub fn matches_ignore_case(&self, input: &str) -> bool {
        let input = input.to_lowercase_smolstr();
        match *self {
            Self::Exact(ref pattern) => input == pattern.to_lowercase_smolstr(),
            Self::Partial { ref starts_with, ref ends_with, ref contains } => {
                !(starts_with.is_empty() && ends_with.is_empty() && contains.is_empty())
                    && (starts_with.is_empty()
                        || input.starts_with(starts_with.to_lowercase_smolstr().as_str()))
                    && (ends_with.is_empty()
                        || input.ends_with(ends_with.to_lowercase_smolstr().as_str()))
                    && (contains.is_empty()
                        || input.contains(contains.to_lowercase_smolstr().as_str()))
            }
        }
    }
}
