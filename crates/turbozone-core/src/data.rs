//! Serialized configuration data and validated runtime rules.

use euclid::default::Size2D;
use serde::{Deserialize, Serialize};

/// A client-area size represented in configuration as `[width, height]`.
pub type ConfigSize = (i32, i32);

/// The serialized top-level TurboZone configuration.
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Rules in source order.
    #[serde(default)]
    pub rules: Vec<Rule>,
}

/// One serialized rule.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    /// Unique stable identifier used in persistent UI section identity.
    pub name: String,
    /// Optional user-facing section name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional move behavior.
    #[serde(default, rename = "move", skip_serializing_if = "Option::is_none")]
    pub r#move: Option<MoveConfig>,
    /// Optional resize behavior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resize: Option<ResizeConfig>,
    /// Optional window-selection constraints.
    #[serde(default, rename = "match", skip_serializing_if = "Option::is_none")]
    pub r#match: Option<RuleMatch>,
}

/// Serialized move shorthand.
#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum MoveConfig {
    /// A Boolean toggle; `true` selects centering.
    Boolean(bool),
    /// An explicit move target.
    Target(MoveTarget),
}

/// A configured move target.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MoveTarget {
    /// Center the live or restored window rectangle.
    Center,
}

/// Serialized resize shorthand or complete settings.
#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ResizeConfig {
    /// A Boolean toggle; `true` enables selector-only resizing.
    Boolean(bool),
    /// A fixed target width and height.
    Size(ConfigSize),
    /// Complete resize behavior.
    Settings(ResizeSettings),
}

/// Complete serialized resize behavior.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResizeSettings {
    /// Whether resize controls are available.
    pub enabled: bool,
    /// Optional fixed target width.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_width: Option<i32>,
    /// Optional fixed target height.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_height: Option<i32>,
    /// Optional minimum width offered by the selector.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_width: Option<i32>,
    /// Optional minimum height offered by the selector.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_height: Option<i32>,
    /// Optional maximum width offered by the selector.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_width: Option<i32>,
    /// Optional maximum height offered by the selector.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_height: Option<i32>,
}

/// Serialized rule-selection settings.
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuleMatch {
    /// Higher priorities take precedence; zero is the default.
    #[serde(default)]
    pub priority: i64,
    /// Optional executable constraints.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable: Option<ExecutableMatch>,
    /// Optional window constraints.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<WindowMatch>,
}

/// Serialized executable constraints.
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutableMatch {
    /// Optional case-insensitive executable-name matcher.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<StringMatcher>,
    /// Optional case-insensitive normalized-path matcher.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<StringMatcher>,
}

/// Serialized window constraints.
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WindowMatch {
    /// Optional case-sensitive window-title matcher.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<StringMatcher>,
    /// Inclusive minimum controllable client-area size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_size: Option<ConfigSize>,
    /// Inclusive maximum controllable client-area size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_size: Option<ConfigSize>,
}

/// A bare exact matcher, explicit exact matcher, or component matcher.
#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum StringMatcher {
    /// A bare exact string.
    Bare(String),
    /// An explicit exact string.
    Exact(ExactStringMatcher),
    /// Several component predicates which are ANDed.
    Components(ComponentStringMatcher),
}

/// The explicit exact string-matcher form.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExactStringMatcher {
    /// The complete candidate string required for a match.
    pub exact: String,
}

/// Component string predicates which are ANDed when present.
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentStringMatcher {
    /// Optional required prefix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starts_with: Option<String>,
    /// Optional required suffix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ends_with: Option<String>,
    /// Optional required substring.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contains: Option<String>,
}

/// A string pattern paired with its compiled predicate.
#[derive(Debug)]
pub struct RuntimeStringMatcher {
    pattern: String,
    predicate: fn(input: &str, pattern: &str) -> bool,
}

impl RuntimeStringMatcher {
    /// Creates a compiled matcher from an owned pattern and predicate.
    pub(crate) const fn new(
        pattern: String,
        predicate: fn(input: &str, pattern: &str) -> bool) -> Self {
        Self { pattern, predicate }
    }

    /// Returns whether the candidate satisfies this predicate.
    pub fn matches(&self, input: &str) -> bool {
        (self.predicate)(input, &self.pattern)
    }
}

/// Fully resolved move behavior.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RuntimeMove {
    /// Moving is unavailable.
    #[default]
    Disabled,
    /// Centering is available.
    Center,
}

/// Fully resolved resize behavior.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RuntimeResize {
    /// Whether resize controls are available.
    pub enabled: bool,
    /// Optional fixed target width.
    pub target_width: Option<i32>,
    /// Optional fixed target height.
    pub target_height: Option<i32>,
    /// Optional minimum selector width.
    pub min_width: Option<i32>,
    /// Optional minimum selector height.
    pub min_height: Option<i32>,
    /// Optional maximum selector width.
    pub max_width: Option<i32>,
    /// Optional maximum selector height.
    pub max_height: Option<i32>,
}

impl RuntimeResize {
    /// Returns the validated primary target when both dimensions are present.
    pub fn primary_size(self) -> Option<Size2D<i32>> {
        self.target_width.zip(self.target_height)
            .map(|(width, height)| Size2D::new(width, height))
    }

    /// Returns whether a built-in selector size satisfies every configured limit.
    pub fn allows_selector_size(self, size: Size2D<i32>) -> bool {
        self.min_width.is_none_or(|minimum| size.width >= minimum)
            && self.min_height.is_none_or(|minimum| size.height >= minimum)
            && self.max_width.is_none_or(|maximum| size.width <= maximum)
            && self.max_height.is_none_or(|maximum| size.height <= maximum)
    }
}

/// Validated executable predicates.
#[derive(Debug, Default)]
pub struct RuntimeExecutableMatch {
    /// Predicates ANDed against the lowercased executable filename.
    pub name: Vec<RuntimeStringMatcher>,
    /// Predicates ANDed against the lowercased normalized executable path.
    pub path: Vec<RuntimeStringMatcher>,
}

/// Validated window predicates.
#[derive(Debug, Default)]
pub struct RuntimeWindowMatch {
    /// Predicates ANDed against the case-sensitive window title.
    pub title: Vec<RuntimeStringMatcher>,
    /// Inclusive minimum controllable client-area size.
    pub min_size: Option<Size2D<i32>>,
    /// Inclusive maximum controllable client-area size.
    pub max_size: Option<Size2D<i32>>,
}

/// Fully resolved matching behavior.
#[derive(Debug, Default)]
pub struct RuntimeRuleMatch {
    /// Explicit or default matching priority.
    pub priority: i64,
    /// Optional executable predicates.
    pub executable: Option<RuntimeExecutableMatch>,
    /// Optional window predicates.
    pub window: Option<RuntimeWindowMatch>,
}

/// A validated rule ready for matching and UI rendering.
#[derive(Debug)]
pub struct RuntimeRule {
    /// Stable unique rule identifier.
    pub name: String,
    /// Optional trimmed user-facing section name.
    pub description: Option<String>,
    /// Fully resolved move behavior.
    pub r#move: RuntimeMove,
    /// Fully resolved resize behavior.
    pub resize: RuntimeResize,
    /// Fully resolved matching behavior.
    pub r#match: RuntimeRuleMatch,
}

impl RuntimeRule {
    /// Returns whether every configured constraint accepts the supplied window data.
    ///
    /// Executable names and paths must already be lowercased. Window titles remain
    /// case-sensitive and must retain their native casing.
    pub fn matches(
        &self,
        executable_name: Option<&str>,
        executable_path: &str,
        window_title: &str,
        client_size: Option<Size2D<i32>>) -> bool {
        self.matches_executable(executable_name, executable_path)
            && self.matches_window(window_title, client_size)
    }

    fn matches_executable(&self, name: Option<&str>, path: &str) -> bool {
        let Some(ref matcher) = self.r#match.executable else {
            return true;
        };
        let name_matches = matcher.name.is_empty() || name.is_some_and(|name| {
            matcher.name.iter().all(|predicate| predicate.matches(name))
        });
        name_matches && matcher.path.iter().all(|predicate| predicate.matches(path))
    }

    fn matches_window(&self, title: &str, size: Option<Size2D<i32>>) -> bool {
        let Some(ref matcher) = self.r#match.window else {
            return true;
        };
        if !matcher.title.iter().all(|predicate| predicate.matches(title)) {
            return false;
        }
        if matcher.min_size.is_none() && matcher.max_size.is_none() {
            return true;
        }
        let Some(size) = size else {
            return false;
        };
        matcher.min_size.is_none_or(|minimum| {
            size.width >= minimum.width && size.height >= minimum.height
        }) && matcher.max_size.is_none_or(|maximum| {
            size.width <= maximum.width && size.height <= maximum.height
        })
    }
}

/// Validated rules retained in source order.
#[derive(Debug, Default)]
pub struct RuntimeConfig {
    /// Runtime rules in configuration source order.
    pub rules: Vec<RuntimeRule>,
}

impl RuntimeConfig {
    /// Returns the winning rule index, preferring higher priority and then source order.
    ///
    /// Executable names and paths must already be lowercased. Window titles remain
    /// case-sensitive and must retain their native casing.
    pub fn matching_rule_index(
        &self,
        executable_name: Option<&str>,
        executable_path: &str,
        window_title: &str,
        client_size: Option<Size2D<i32>>) -> Option<usize> {
        let mut winner = None;
        for (index, rule) in self.rules.iter().enumerate() {
            if !rule.matches(
                executable_name,
                executable_path,
                window_title,
                client_size) {
                continue;
            }
            if winner.is_none_or(|(_, priority)| rule.r#match.priority > priority) {
                winner = Some((index, rule.r#match.priority));
            }
        }
        winner.map(|(index, _)| index)
    }
}
