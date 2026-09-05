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

/// The shared serialized and engine-owned configuration for TurboZone.
///
/// Deserialization supplies structure and defaults. Call [`crate::verify_config`]
/// before engine use; verification preserves these authored values unchanged.
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

/// One rule pairing authored window filters with the actions they enable.
///
/// The engine owns verified rules and interprets patterns on demand; presentation
/// queries display and resize helpers without maintaining another rule model.
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
    /// Authored description; [`Self::display_name`] trims and resolves blank labels on access.
    #[serde(default, skip_serializing_if = "is_default")]
    pub description: SmolStr,

    // ---- Filters ----
    /// Higher priorities take precedence; zero is the default.
    #[serde(default, skip_serializing_if = "is_default")]
    pub priority: i64,
    /// Program filters.
    #[serde(default, skip_serializing_if = "is_default")]
    pub program: ProgramFilter,
    /// Window filters.
    #[serde(default, skip_serializing_if = "is_default")]
    pub window: WindowFilter,

    // ---- Actions ----
    /// Relocation controls, currently only the "center" button.
    #[serde(default, skip_serializing_if = "is_default")]
    #[serde(rename = "move")]
    pub relocate: bool,
    /// Resize controls, target, and selector limits.
    #[serde(default, skip_serializing_if = "is_default")]
    pub resize: ResizeRule,
}

impl Rule {
    /// Borrows the trimmed display label, falling back to the stable rule name.
    /// Authored description text remains intact for inspection and serialization.
    pub fn display_name(&self) -> &str {
        let description = self.description.trim();
        if description.is_empty() { &self.name } else { description }
    }
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

impl ResizeRule {
    /// Returns the exact target or a selector default within its inclusive bounds.
    /// An out-of-bounds default remains authored but is unavailable to every consumer.
    /// Verification reports that condition; this query stays silent during rendering.
    pub fn primary_size(&self) -> Option<Size2D<i32>> {
        match *self {
            Self::Boolean(_) => None,
            Self::Exact { exact } => Some(exact.into()),
            Self::SelectorDefault(default) => Some(default.into()),
            Self::Selector(ref selector) => selector.default.map(Size2D::from)
                .filter(|&size| selector.allows_size(size)),
        }
    }

    /// Returns selector settings for enabled selector modes, synthesizing shorthand.
    /// The result contains only small optional size arrays; no heap storage or cached
    /// runtime representation is needed. Exact and disabled modes have no selector.
    pub fn selector(&self) -> Option<ResizeSelector> {
        match *self {
            Self::Boolean(false) | Self::Exact { .. } => None,
            Self::Boolean(true) => Some(ResizeSelector::default()),
            Self::SelectorDefault(default) => Some(ResizeSelector {
                default: Some(default),
                ..Default::default()
            }),
            Self::Selector(ref selector) => Some(selector.clone()),
        }
    }
}

/// Selector defaults and inclusive bounds; omitted bounds are unrestricted.
#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(Default)]
#[derive(JsonSchema)]
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResizeSelector {
    /// Primary `[width, height]` in physical pixels. A default outside the inclusive
    /// bounds is preserved in the configuration but warns during verification and
    /// is not offered as a resize target.
    #[schemars(inner(range(min = 1, max = MAX_SIZE_DIMENSION)))]
    pub default: Option<[i32; 2]>,
    /// Minimum `[width, height]` for the default and menu choices, in physical pixels.
    #[schemars(inner(range(min = 1, max = MAX_SIZE_DIMENSION)))]
    pub min: Option<[i32; 2]>,
    /// Maximum `[width, height]` for the default and menu choices, in physical pixels.
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
#[derive(Default)]
#[derive(JsonSchema)]
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramFilter {
    /// Optional case-insensitive filename matcher.
    pub name: Option<Pattern>,
    /// Optional case-insensitive path matcher.
    pub path: Option<Pattern>,
}

/// Window filters retaining authored patterns for runtime evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(Default)]
#[derive(JsonSchema)]
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WindowFilter {
    /// Optional case-sensitive window-title matcher.
    pub title: Option<Pattern>,
    /// Inclusive minimum client-area `[width, height]` in supported
    /// physical pixels.
    #[schemars(inner(range(min = 1, max = MAX_SIZE_DIMENSION)))]
    pub min: Option<[i32; 2]>,
    /// Inclusive maximum client-area `[width, height]` in supported
    /// physical pixels.
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
