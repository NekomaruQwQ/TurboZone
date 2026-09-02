use crate::prelude::*;
use crate::Pattern;

use schemars::*;
use schemars::generate::SchemaSettings;

const MAX_SIZE: i32 = 8192;

/// The serialized top-level configuration for TurboZone.
#[derive(Debug, Clone)]
#[derive(Default)]
#[derive(Deserialize, Serialize)]
#[derive(JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Rules in source order.
    #[serde(default)]
    pub rules: Vec<Rule>,
}

impl Config {
    /// Generates the JSON schema for [`Config`].
    ///
    /// Extra validation rules that cannot be enforced by the schema itself are
    /// validated in [`crate::parse_config`] and [`crate::compile_config`].
    pub fn schema() -> Schema {
        SchemaSettings::draft2020_12()
            .for_deserialize()
            .into_generator()
            .into_root_schema_for::<Self>()
    }
}

/// One serialized rule pairing window filters with the actions they enable.
#[derive(Debug, Clone)]
#[derive(Default)]
#[derive(Deserialize, Serialize)]
#[derive(JsonSchema)]
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
#[derive(JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(untagged)]
pub enum ResizeRule {
    /// Resizing disabled or selector enabled without any size filters.
    Boolean(bool),
    /// Exact target size. The selector is disabled in this mode.
    Exact {
        /// Positive client-area `[width, height]` in physical pixels, independent of selector limits.
        #[schemars(inner(range(min = 1, max = MAX_SIZE)))]
        exact: [i32; 2],
    },
    /// Selector properties with only a default size, no minimum, and no maximum.
    SelectorDefault(
        #[schemars(inner(range(min = 1, max = MAX_SIZE)))]
        [i32; 2],
    ),
    /// Selector properties, including optional default, minimum, and maximum sizes.
    Selector(ResizeSelector),
}

impl Default for ResizeRule {
    fn default() -> Self {
        Self::Boolean(false)
    }
}

/// Selector defaults and inclusive bounds; omitted bounds are unrestricted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[derive(Deserialize, Serialize)]
#[derive(JsonSchema)]
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
#[derive(Deserialize, Serialize)]
#[derive(JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProgramFilter<S> {
    /// Optional case-insensitive filename matcher.
    pub name: Option<S>,
    /// Optional case-insensitive path matcher.
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
#[derive(JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WindowFilter<S> {
    /// Optional case-sensitive window-title matcher.
    pub title: Option<S>,
    /// Inclusive minimum client-area `[width, height]` in positive physical pixels.
    pub min: Option<[i32; 2]>,
    /// Inclusive maximum client-area `[width, height]` in positive physical pixels.
    pub max: Option<[i32; 2]>,
}

impl<S> Default for WindowFilter<S> {
    fn default() -> Self {
        Self { title: None, min: None, max: None }
    }
}
