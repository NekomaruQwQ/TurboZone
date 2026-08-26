use crate::prelude::*;
use super::*;

/// The serialized top-level configuration for TurboZone.
#[derive(Debug, Clone)]
#[derive(Default)]
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Rules in source order.
    #[serde(default)]
    pub rules: Vec<ConfigRule>,
}

/// One serialized rule pairing window constraints with the actions they enable.
#[derive(Debug, Clone)]
#[derive(Default)]
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigRule {
    // ---- Metadata ----
    /// Unique stable identifier of the rule.
    pub name: String,
    /// User-facing section name, falling back to the rule name when empty.
    #[serde(default, skip_serializing_if = "is_default")]
    pub description: String,

    // ---- Constraints ----
    /// Program constraints.
    #[serde(default, skip_serializing_if = "is_default")]
    pub program: ProgramConstraint<Pattern>,
    /// Window constraints.
    #[serde(default, skip_serializing_if = "is_default")]
    pub window: WindowConstraint<Pattern>,
    /// Higher priorities take precedence; zero is the default.
    #[serde(default, skip_serializing_if = "is_default")]
    pub priority: i64,

    // ---- Actions ----
    /// Relocation controls, currently only the "center" button.
    #[serde(default, skip_serializing_if = "is_default")]
    pub relocate: bool,
    /// Resize controls, target, and selector limits.
    #[serde(default, skip_serializing_if = "is_default")]
    pub resize: ResizeRule,
}

/// Complete serialized resize behavior.
#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[serde(untagged)]
pub enum ResizeRule {
    /// Resizing disabled or selector enabled without any size constraints.
    Boolean(bool),
    /// Exact target size. The selector is disabled in this mode.
    Exact {
        /// Positive client-area dimensions, independent of selector limits.
        exact: Size2D<i32>,
    },
    /// Selector properties, including optional default, minimum, and maximum sizes.
    Selector(ResizeLimits),
}

impl Default for ResizeRule {
    fn default() -> Self {
        Self::Boolean(false)
    }
}

/// Selector defaults and inclusive bounds; omitted bounds are unrestricted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResizeLimits {
    /// Primary resize target, independent of the selector bounds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Size2D<i32>>,
    /// The minimum size offered by the selector.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<Size2D<i32>>,
    /// The maximum size offered by the selector.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<Size2D<i32>>,
}

impl ResizeLimits {
    /// Returns whether a positive size is within all configured selector bounds.
    pub fn allows_size(&self, size: Size2D<i32>) -> bool {
        size.width > 0 && size.height > 0
            && self.min.is_none_or(|min| size.width >= min.width && size.height >= min.height)
            && self.max.is_none_or(|max| size.width <= max.width && size.height <= max.height)
    }
}

/// Program constraints using serialized patterns or compiled predicates.
#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramConstraint<S> {
    /// Optional case-insensitive program-name matcher.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<S>,
    /// Optional case-insensitive normalized-path matcher.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<S>,
}

impl<S> Default for ProgramConstraint<S> {
    fn default() -> Self {
        Self { name: None, path: None }
    }
}

/// Window constraints using serialized patterns or compiled predicates.
#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WindowConstraint<S> {
    /// Optional case-sensitive window-title matcher.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<S>,
    /// Inclusive minimum controllable client-area size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<Size2D<i32>>,
    /// Inclusive maximum controllable client-area size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<Size2D<i32>>,
}

impl<S> Default for WindowConstraint<S> {
    fn default() -> Self {
        Self { title: None, min: None, max: None }
    }
}
