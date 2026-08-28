use crate::prelude::*;
use super::*;

use schemars::*;
use schemars::generate::SchemaSettings;

/// The serialized top-level configuration for TurboZone.
#[derive(Debug, Clone)]
#[derive(Default)]
#[derive(Deserialize, Serialize)]
#[derive(schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Rules in source order.
    #[serde(default)]
    pub rules: Vec<Rule>,
}

impl Config {
    /// Generates the input schema for TOML editors without running application validation.
    ///
    /// Draft 7 avoids requiring newer JSON Schema keywords in editor integrations.
    /// Rule-name grammar, duplicate names, nonempty partial patterns, path separators,
    /// and comparisons between bounds remain the responsibility of `Config::validate`.
    pub fn schema() -> Schema {
        SchemaSettings::draft07()
            .for_deserialize()
            .into_generator()
            .into_root_schema_for::<Self>()
    }
}

/// One serialized rule pairing window filters with the actions they enable.
#[derive(Debug, Clone)]
#[derive(Default)]
#[derive(Deserialize, Serialize)]
#[derive(schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    // ---- Metadata ----
    /// Unique stable identifier of the rule.
    pub name: String,
    /// User-facing section name, falling back to the rule name when empty.
    #[serde(default, skip_serializing_if = "is_default")]
    pub description: String,

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

/// Complete serialized resize behavior.
#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(Deserialize, Serialize)]
#[derive(schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(untagged)]
pub enum ResizeRule {
    /// Resizing disabled or selector enabled without any size filters.
    Boolean(bool),
    /// Exact target size. The selector is disabled in this mode.
    Exact {
        /// Positive client-area `[width, height]` in physical pixels, independent of selector limits.
        #[schemars(inner(range(min = 1, max = i32::MAX)))]
        exact: [i32; 2],
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
#[derive(schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResizeLimits {
    /// Primary `[width, height]` in positive physical pixels, independent of selector bounds.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(inner(range(min = 1, max = i32::MAX)))]
    pub default: Option<[i32; 2]>,
    /// Minimum `[width, height]` offered by the selector, in positive physical pixels.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(inner(range(min = 1, max = i32::MAX)))]
    pub min: Option<[i32; 2]>,
    /// Maximum `[width, height]` offered by the selector, in positive physical pixels.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(inner(range(min = 1, max = i32::MAX)))]
    pub max: Option<[i32; 2]>,
}

impl ResizeLimits {
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
#[derive(Deserialize, Serialize)]
#[derive(schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProgramFilter<S> {
    /// Optional case-insensitive program-name matcher.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<S>,
    /// Optional case-insensitive normalized-path matcher.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<S>,
}

impl<S> Default for ProgramFilter<S> {
    fn default() -> Self {
        Self { name: None, path: None }
    }
}

/// Window filters using serialized patterns or compiled predicates.
#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(Deserialize, Serialize)]
#[derive(schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WindowFilter<S> {
    /// Optional case-sensitive window-title matcher.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<S>,
    /// Inclusive minimum client-area `[width, height]` in positive physical pixels.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(inner(range(min = 1, max = i32::MAX)))]
    pub min: Option<[i32; 2]>,
    /// Inclusive maximum client-area `[width, height]` in positive physical pixels.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(inner(range(min = 1, max = i32::MAX)))]
    pub max: Option<[i32; 2]>,
}

impl<S> Default for WindowFilter<S> {
    fn default() -> Self {
        Self { title: None, min: None, max: None }
    }
}
