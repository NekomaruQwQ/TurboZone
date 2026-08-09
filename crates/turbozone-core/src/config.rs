//! Deserialized and validated TurboRnR configuration.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::geometry::{optional_window_size_serde, WindowSize};

/// The stable identifier of a named group.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[derive(Deserialize, Serialize)]
#[serde(transparent)]
pub struct GroupId(pub String);

impl fmt::Display for GroupId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// An exact or substring string matcher as represented in TOML.
#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(Deserialize, Serialize)]
#[serde(untagged)]
pub enum StringMatcher {
    /// Matches the entire candidate string.
    Exact(String),
    /// Matches when the candidate contains the configured substring.
    Contains {
        /// The required non-empty substring.
        contains: String,
    },
}

impl StringMatcher {
    /// Matches a candidate, using Unicode lowercase comparison when case is ignored.
    pub fn matches(&self, candidate: &str, case_sensitive: bool) -> bool {
        match (self, case_sensitive) {
            (&Self::Exact(ref expected), true) => candidate == expected,
            (&Self::Contains { ref contains }, true) => candidate.contains(contains),
            (&Self::Exact(ref expected), false) => candidate.to_lowercase() == expected.to_lowercase(),
            (&Self::Contains { ref contains }, false) => {
                candidate.to_lowercase().contains(&contains.to_lowercase())
            },
        }
    }

    fn contains_value(&self) -> Option<&str> {
        match *self {
            Self::Exact(_) => None,
            Self::Contains { ref contains } => Some(contains),
        }
    }

    fn value(&self) -> &str {
        match *self {
            Self::Exact(ref value) => value,
            Self::Contains { ref contains } => contains,
        }
    }
}

/// Match constraints for executable metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[derive(Deserialize, Serialize)]
pub struct ExecutableMatcher {
    /// Optional executable filename matcher.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<StringMatcher>,
    /// Optional forward-slash executable path matcher.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<StringMatcher>,
}

impl ExecutableMatcher {
    /// Returns whether no executable constraint was configured.
    pub const fn is_unconstrained(&self) -> bool {
        self.name.is_none() && self.path.is_none()
    }

    /// Matches executable metadata, ANDing every configured field.
    pub fn matches(&self, name: Option<&str>, path: Option<&str>) -> bool {
        self.name.as_ref().is_none_or(|matcher| {
            name.is_some_and(|candidate| matcher.matches(candidate, false))
        }) && self.path.as_ref().is_none_or(|matcher| {
            path.is_some_and(|candidate| matcher.matches(candidate, false))
        })
    }
}

/// One group entry in the serialized configuration.
///
/// Validation converts this shape into either a named group or executable policy.
/// Mixing the two forms is rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(Deserialize, Serialize)]
pub struct GroupDefinition {
    /// Stable identifier for the named-group form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<GroupId>,
    /// Display label for the named-group form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Executable constraints for the executable-policy form.
    #[serde(default, skip_serializing_if = "ExecutableMatcher::is_unconstrained")]
    pub executable: ExecutableMatcher,
    /// Whether windows in the resulting group may be resized.
    pub allow_resize: bool,
    /// Optional one-click resize target.
    #[serde(default, with = "optional_window_size_serde", skip_serializing_if = "Option::is_none")]
    pub default_size: Option<WindowSize>,
}

/// One ordered rule in the serialized configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(Deserialize, Serialize)]
pub struct RuleDefinition {
    /// Human-readable rule name used in diagnostics.
    pub name: String,
    /// Stable named-group target.
    pub group: GroupId,
    /// Optional executable constraints.
    #[serde(default, skip_serializing_if = "ExecutableMatcher::is_unconstrained")]
    pub executable: ExecutableMatcher,
    /// Optional case-sensitive window-title constraint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_title: Option<StringMatcher>,
}

/// The serialized top-level TurboRnR configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[derive(Deserialize, Serialize)]
pub struct Config {
    /// Named groups and executable policies in source order.
    #[serde(default)]
    pub groups: Vec<GroupDefinition>,
    /// Named-group routing rules in first-match order.
    #[serde(default)]
    pub rules: Vec<RuleDefinition>,
}

impl Config {
    /// Validates and separates serialized definitions into deterministic runtime state.
    pub fn validate(self) -> Result<RuntimeConfig, ConfigError> {
        let mut named_groups = BTreeMap::new();
        let mut executable_policies = Vec::new();

        for (index, definition) in self.groups.into_iter().enumerate() {
            validate_group_settings(index, definition.allow_resize, definition.default_size)?;
            validate_executable_matcher(
                &definition.executable,
                &format!("groups[{index}].executable"))?;

            let is_named = definition.id.is_some()
                && definition.name.is_some()
                && definition.executable.is_unconstrained();
            let is_executable_policy = definition.id.is_none()
                && definition.name.is_none()
                && !definition.executable.is_unconstrained();

            if is_named {
                let id = definition.id.expect("named form checked above");
                let name = definition.name.expect("named form checked above");
                if id.0.is_empty() {
                    return Err(ConfigError::EmptyGroupId { index });
                }
                if name.is_empty() {
                    return Err(ConfigError::EmptyGroupName { index });
                }
                let group = NamedGroup {
                    id: id.clone(),
                    name,
                    allow_resize: definition.allow_resize,
                    default_size: definition.default_size,
                };
                if named_groups.insert(id.clone(), group).is_some() {
                    return Err(ConfigError::DuplicateGroupId { id });
                }
            } else if is_executable_policy {
                executable_policies.push(ExecutableGroupPolicy {
                    executable: definition.executable,
                    allow_resize: definition.allow_resize,
                    default_size: definition.default_size,
                });
            } else {
                return Err(ConfigError::InvalidGroupForm { index });
            }
        }

        let mut rules = Vec::with_capacity(self.rules.len());
        for (index, definition) in self.rules.into_iter().enumerate() {
            validate_executable_matcher(
                &definition.executable,
                &format!("rules[{index}].executable"))?;
            if let Some(ref window_title) = definition.window_title {
                validate_string_matcher(window_title, &format!("rules[{index}].window_title"))?;
            }
            if !named_groups.contains_key(&definition.group) {
                return Err(ConfigError::MissingRuleGroup {
                    rule: definition.name,
                    group: definition.group,
                });
            }
            rules.push(Rule {
                name: definition.name,
                group: definition.group,
                executable: definition.executable,
                window_title: definition.window_title,
            });
        }

        Ok(RuntimeConfig {
            named_groups,
            executable_policies,
            rules,
        })
    }
}

/// A validated named group targeted by rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedGroup {
    /// Stable configuration identifier.
    pub id: GroupId,
    /// User-facing group name.
    pub name: String,
    /// Whether resize controls are enabled.
    pub allow_resize: bool,
    /// Optional one-click resize target.
    pub default_size: Option<WindowSize>,
}

/// A validated ordered policy applied independently to each matching executable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutableGroupPolicy {
    /// Executable constraints.
    pub executable: ExecutableMatcher,
    /// Whether resize controls are enabled.
    pub allow_resize: bool,
    /// Optional one-click resize target.
    pub default_size: Option<WindowSize>,
}

/// A validated ordered rule targeting a named group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// Human-readable diagnostic name.
    pub name: String,
    /// Stable named-group target.
    pub group: GroupId,
    /// Executable constraints.
    pub executable: ExecutableMatcher,
    /// Optional case-sensitive title constraint.
    pub window_title: Option<StringMatcher>,
}

impl Rule {
    /// Matches a window, ANDing executable and title constraints.
    pub fn matches(
        &self,
        executable_name: Option<&str>,
        executable_path: Option<&str>,
        window_title: &str) -> bool {
        self.executable.matches(executable_name, executable_path)
            && self.window_title.as_ref().is_none_or(|matcher| {
                matcher.matches(window_title, true)
            })
    }
}

/// Validated configuration organized for deterministic runtime lookup.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimeConfig {
    named_groups: BTreeMap<GroupId, NamedGroup>,
    executable_policies: Vec<ExecutableGroupPolicy>,
    rules: Vec<Rule>,
}

impl RuntimeConfig {
    /// Returns the sorted named-group map.
    pub const fn named_groups(&self) -> &BTreeMap<GroupId, NamedGroup> {
        &self.named_groups
    }

    /// Returns executable policies in first-match order.
    pub fn executable_policies(&self) -> &[ExecutableGroupPolicy] {
        &self.executable_policies
    }

    /// Returns rules in first-match order.
    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }
}

/// A semantic configuration validation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// A group mixed named and executable-policy fields or completed neither form.
    InvalidGroupForm {
        /// Zero-based group index.
        index: usize,
    },
    /// A named group used an empty stable ID.
    EmptyGroupId {
        /// Zero-based group index.
        index: usize,
    },
    /// A named group used an empty display name.
    EmptyGroupName {
        /// Zero-based group index.
        index: usize,
    },
    /// A named group ID occurred more than once.
    DuplicateGroupId {
        /// Duplicate stable ID.
        id: GroupId,
    },
    /// A rule referenced no defined named group.
    MissingRuleGroup {
        /// Human-readable rule name.
        rule: String,
        /// Missing stable group ID.
        group: GroupId,
    },
    /// A contains matcher used an empty substring.
    EmptyContains {
        /// Configuration field containing the invalid matcher.
        field: String,
    },
    /// A configured executable path used a backslash.
    BackslashInExecutablePath {
        /// Configuration field containing the invalid path.
        field: String,
    },
    /// A configured default size contained a non-positive dimension.
    InvalidDefaultSize {
        /// Zero-based group index.
        index: usize,
        /// Invalid width.
        width: i32,
        /// Invalid height.
        height: i32,
    },
    /// A resize-disabled group also configured a default size.
    DefaultSizeWhenResizeDisabled {
        /// Zero-based group index.
        index: usize,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::InvalidGroupForm { index } => write!(
                f,
                "groups[{index}] must be either id + name or executable.*"),
            Self::EmptyGroupId { index } => write!(f, "groups[{index}].id must not be empty"),
            Self::EmptyGroupName { index } => {
                write!(f, "groups[{index}].name must not be empty")
            },
            Self::DuplicateGroupId { ref id } => write!(f, "duplicate named group id '{id}'"),
            Self::MissingRuleGroup { ref rule, ref group } => {
                write!(f, "rule '{rule}' references missing named group '{group}'")
            },
            Self::EmptyContains { ref field } => write!(f, "{field}.contains must not be empty"),
            Self::BackslashInExecutablePath { ref field } => write!(
                f,
                "{field} must use forward slashes; backslashes are not accepted"),
            Self::InvalidDefaultSize { index, width, height } => write!(
                f,
                "groups[{index}].default_size must be positive, found [{width}, {height}]"),
            Self::DefaultSizeWhenResizeDisabled { index } => write!(
                f,
                "groups[{index}].default_size requires allow_resize = true"),
        }
    }
}

impl Error for ConfigError {}

const fn validate_group_settings(
    index: usize,
    allow_resize: bool,
    default_size: Option<WindowSize>) -> Result<(), ConfigError> {
    let Some(default_size) = default_size else {
        return Ok(());
    };
    if default_size.width <= 0 || default_size.height <= 0 {
        return Err(ConfigError::InvalidDefaultSize {
            index,
            width: default_size.width,
            height: default_size.height,
        });
    }
    if !allow_resize {
        return Err(ConfigError::DefaultSizeWhenResizeDisabled { index });
    }
    Ok(())
}

fn validate_executable_matcher(
    matcher: &ExecutableMatcher,
    field: &str) -> Result<(), ConfigError> {
    if let Some(ref name) = matcher.name {
        validate_string_matcher(name, &format!("{field}.name"))?;
    }
    if let Some(ref path) = matcher.path {
        validate_string_matcher(path, &format!("{field}.path"))?;
        if path.value().contains('\\') {
            return Err(ConfigError::BackslashInExecutablePath {
                field: format!("{field}.path"),
            });
        }
    }
    Ok(())
}

fn validate_string_matcher(matcher: &StringMatcher, field: &str) -> Result<(), ConfigError> {
    if matcher.contains_value() == Some("") {
        Err(ConfigError::EmptyContains {
            field: field.to_owned(),
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> Config {
        toml::from_str(source).expect("test configuration must deserialize")
    }

    #[test]
    fn validate_accepts_rule_referencing_named_group() {
        let config = parse(r#"
            [[groups]]
            id = "edge"
            name = "Edge"
            allow_resize = true

            [[rules]]
            name = "Edge rule"
            group = "edge"
        "#);

        config.validate().expect("valid group reference must validate");
    }

    #[test]
    fn validate_rejects_rule_referencing_missing_named_group() {
        let config = parse(r#"
            [[rules]]
            name = "Missing rule"
            group = "missing"
        "#);

        assert_eq!(
            config.validate(),
            Err(ConfigError::MissingRuleGroup {
                rule: "Missing rule".to_owned(),
                group: GroupId("missing".to_owned()),
            }));
    }

    #[test]
    fn validate_rejects_duplicate_named_group_ids() {
        let config = parse(r#"
            [[groups]]
            id = "edge"
            name = "First"
            allow_resize = true

            [[groups]]
            id = "edge"
            name = "Second"
            allow_resize = true
        "#);

        assert_eq!(
            config.validate(),
            Err(ConfigError::DuplicateGroupId {
                id: GroupId("edge".to_owned()),
            }));
    }

    #[test]
    fn deserialize_supports_exact_and_contains_matchers() {
        let config = parse(r#"
            [[groups]]
            executable.name = "msedge.exe"
            executable.path.contains = "/Microsoft/Edge/"
            allow_resize = true
        "#);
        let executable = &config.groups[0].executable;

        assert_eq!(
            executable,
            &ExecutableMatcher {
                name: Some(StringMatcher::Exact("msedge.exe".to_owned())),
                path: Some(StringMatcher::Contains {
                    contains: "/Microsoft/Edge/".to_owned(),
                }),
            });
    }

    #[test]
    fn executable_matcher_is_case_insensitive_for_name_and_path() {
        let matcher = ExecutableMatcher {
            name: Some(StringMatcher::Exact("MSEDGE.EXE".to_owned())),
            path: Some(StringMatcher::Contains {
                contains: "/MICROSOFT/EDGE/".to_owned(),
            }),
        };

        assert!(matcher.matches(
            Some("msedge.exe"),
            Some("C:/Program Files/Microsoft/Edge/msedge.exe")));
    }

    #[test]
    fn rule_title_matcher_is_case_sensitive() {
        let rule = Rule {
            name: "PWA".to_owned(),
            group: GroupId("pwa".to_owned()),
            executable: ExecutableMatcher::default(),
            window_title: Some(StringMatcher::Contains {
                contains: "My PWA".to_owned(),
            }),
        };

        assert!(!rule.matches(None, None, "my pwa"));
    }

    #[test]
    fn rule_fields_are_anded() {
        let rule = Rule {
            name: "Ready tool".to_owned(),
            group: GroupId("tools".to_owned()),
            executable: ExecutableMatcher {
                name: Some(StringMatcher::Exact("tool.exe".to_owned())),
                path: Some(StringMatcher::Contains {
                    contains: "/tools/".to_owned(),
                }),
            },
            window_title: Some(StringMatcher::Contains {
                contains: "Ready".to_owned(),
            }),
        };

        assert!(!rule.matches(
            Some("tool.exe"),
            Some("C:/other/tool.exe"),
            "Ready"));
    }

    #[test]
    fn validate_rejects_backslash_in_configured_executable_path() {
        let config = parse(r"
            [[groups]]
            executable.path = 'C:\Program Files\App\app.exe'
            allow_resize = true
        ");

        assert_eq!(
            config.validate(),
            Err(ConfigError::BackslashInExecutablePath {
                field: "groups[0].executable.path".to_owned(),
            }));
    }

    #[test]
    fn validate_rejects_empty_contains_value() {
        let config = parse(r#"
            [[groups]]
            executable.path.contains = ""
            allow_resize = true
        "#);

        assert_eq!(
            config.validate(),
            Err(ConfigError::EmptyContains {
                field: "groups[0].executable.path".to_owned(),
            }));
    }

    #[test]
    fn validate_rejects_non_positive_default_size() {
        let config = parse(r#"
            [[groups]]
            id = "bad-size"
            name = "Bad size"
            allow_resize = true
            default_size = [0, 900]
        "#);

        assert_eq!(
            config.validate(),
            Err(ConfigError::InvalidDefaultSize {
                index: 0,
                width: 0,
                height: 900,
            }));
    }

    #[test]
    fn validate_accepts_positive_euclid_default_size() {
        let config = parse(r#"
            [[groups]]
            id = "good-size"
            name = "Good size"
            allow_resize = true
            default_size = [1440, 900]
        "#);
        let runtime = config.validate().expect("positive size must validate");

        assert_eq!(
            runtime.named_groups()[&GroupId("good-size".to_owned())].default_size,
            Some(WindowSize::new(1440, 900)));
    }

    #[test]
    fn validate_rejects_default_size_when_resize_is_disabled() {
        let config = parse(r#"
            [[groups]]
            id = "disabled"
            name = "Disabled"
            allow_resize = false
            default_size = [1440, 900]
        "#);

        assert_eq!(
            config.validate(),
            Err(ConfigError::DefaultSizeWhenResizeDisabled { index: 0 }));
    }
}
