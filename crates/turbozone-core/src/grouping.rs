//! Deterministic rule and executable-policy resolution.

use std::collections::BTreeMap;

use crate::{GroupId, NamedGroup, RuntimeConfig, WindowSize};

/// Executable metadata attached to a window snapshot.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecutableMetadata {
    /// Executable filename, such as msedge.exe.
    pub name: Option<String>,
    /// Native executable path already normalized by the platform layer.
    pub path: Option<String>,
    /// Friendly version-resource description used only for display.
    pub display_name: Option<String>,
}

/// A platform-neutral window plus an owned payload retained in its resolved group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowCandidate<T> {
    /// Case-sensitive window title used by rules.
    pub window_title: String,
    /// Executable metadata used by rules, policies, and automatic grouping.
    pub executable: ExecutableMetadata,
    /// Caller-owned platform payload, such as a native window snapshot.
    pub payload: T,
}

/// The deterministic identity of an automatically created executable group.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExecutableGroupId {
    /// Lowercased normalized native path.
    Path(String),
    /// Lowercased executable filename used when no path is available.
    Name(String),
    /// No executable path or filename was available.
    Unknown,
}

/// The stable key of a runtime window group.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ResolvedGroupId {
    /// A configured named group.
    Named(GroupId),
    /// An automatically created per-executable group.
    Executable(ExecutableGroupId),
}

/// A resolved group of native payloads sharing controls and display metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowGroup<T> {
    /// Stable runtime group identity.
    pub id: ResolvedGroupId,
    /// User-facing group name.
    pub name: String,
    /// Executable metadata for automatic groups; named groups may span executables.
    pub executable: Option<ExecutableMetadata>,
    /// Whether resize controls are enabled.
    pub allow_resize: bool,
    /// Optional one-click resize target.
    pub default_size: Option<WindowSize>,
    /// Platform payloads assigned to the group.
    pub windows: Vec<T>,
}

/// Resolves windows with first-match rules and first-match executable policies.
///
/// Unmatched windows form independent executable groups. Those without a policy
/// remain resize-enabled but have no invented default size.
pub fn resolve_window_groups<T>(
    config: &RuntimeConfig,
    windows: impl IntoIterator<Item = WindowCandidate<T>>,
) -> BTreeMap<ResolvedGroupId, WindowGroup<T>> {
    let mut groups = BTreeMap::new();

    for candidate in windows {
        let named_group = config.rules().iter()
            .find(|rule| rule.matches(
                candidate.executable.name.as_deref(),
                candidate.executable.path.as_deref(),
                &candidate.window_title))
            .map(|rule| {
                config.named_groups()
                    .get(&rule.group)
                    .expect("validated rules always reference named groups")
            });

        if let Some(named_group) = named_group {
            insert_named_group(&mut groups, named_group, candidate.payload);
            continue;
        }

        let policy = config.executable_policies().iter().find(|policy| {
            policy.executable.matches(
                candidate.executable.name.as_deref(),
                candidate.executable.path.as_deref())
        });
        let id = ResolvedGroupId::Executable(executable_group_id(&candidate.executable));
        let display_name = candidate.executable.display_name.clone()
            .or_else(|| candidate.executable.name.clone())
            .unwrap_or_else(|| "Unknown executable".to_owned());
        let (allow_resize, default_size) = policy
            .map_or((true, None), |policy| (policy.allow_resize, policy.default_size));
        groups.entry(id.clone())
            .or_insert_with(|| WindowGroup {
                id,
                name: display_name,
                executable: Some(candidate.executable),
                allow_resize,
                default_size,
                windows: Vec::new(),
            })
            .windows
            .push(candidate.payload);
    }

    groups
}

fn insert_named_group<T>(
    groups: &mut BTreeMap<ResolvedGroupId, WindowGroup<T>>,
    named_group: &NamedGroup,
    payload: T) {
    let id = ResolvedGroupId::Named(named_group.id.clone());
    groups.entry(id.clone())
        .or_insert_with(|| WindowGroup {
            id,
            name: named_group.name.clone(),
            executable: None,
            allow_resize: named_group.allow_resize,
            default_size: named_group.default_size,
            windows: Vec::new(),
        })
        .windows
        .push(payload);
}

fn executable_group_id(executable: &ExecutableMetadata) -> ExecutableGroupId {
    if let Some(ref path) = executable.path {
        ExecutableGroupId::Path(path.to_lowercase())
    } else if let Some(ref name) = executable.name {
        ExecutableGroupId::Name(name.to_lowercase())
    } else {
        ExecutableGroupId::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;

    fn config(source: &str) -> RuntimeConfig {
        toml::from_str::<Config>(source)
            .expect("test configuration must deserialize")
            .validate()
            .expect("test configuration must validate")
    }

    fn window(
        payload: u8,
        name: Option<&str>,
        path: Option<&str>,
        title: &str) -> WindowCandidate<u8> {
        WindowCandidate {
            window_title: title.to_owned(),
            executable: ExecutableMetadata {
                name: name.map(str::to_owned),
                path: path.map(str::to_owned),
                display_name: name.map(str::to_owned),
            },
            payload,
        }
    }

    #[test]
    fn first_matching_rule_wins() {
        let config = config(r#"
            [[groups]]
            id = "first"
            name = "First"
            allow_resize = true

            [[groups]]
            id = "second"
            name = "Second"
            allow_resize = true

            [[rules]]
            name = "First rule"
            group = "first"
            executable.name = "app.exe"

            [[rules]]
            name = "Second rule"
            group = "second"
            executable.name = "app.exe"
        "#);

        let groups = resolve_window_groups(
            &config,
            [window(1, Some("app.exe"), Some("C:/app.exe"), "App")]);

        assert!(groups.contains_key(&ResolvedGroupId::Named(GroupId("first".to_owned()))));
    }

    #[test]
    fn first_matching_executable_policy_wins() {
        let config = config(r#"
            [[groups]]
            executable.name = "app.exe"
            allow_resize = true
            default_size = [1440, 900]

            [[groups]]
            executable.name = "app.exe"
            allow_resize = true
            default_size = [1920, 1200]
        "#);

        let groups = resolve_window_groups(
            &config,
            [window(1, Some("app.exe"), Some("C:/app.exe"), "App")]);
        let group = groups.values().next().expect("one group expected");

        assert_eq!(group.default_size, Some(WindowSize::new(1440, 900)));
    }

    #[test]
    fn one_policy_produces_independent_executable_groups() {
        let config = config(r#"
            [[groups]]
            executable.path.contains = "/apps/"
            allow_resize = false
        "#);
        let groups = resolve_window_groups(&config, [
            window(1, Some("one.exe"), Some("C:/apps/one.exe"), "One"),
            window(2, Some("two.exe"), Some("C:/apps/two.exe"), "Two"),
        ]);

        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn several_executables_can_join_one_named_group() {
        let config = config(r#"
            [[groups]]
            id = "tools"
            name = "Tools"
            allow_resize = true

            [[rules]]
            name = "All tools"
            group = "tools"
            window_title.contains = "Tool"
        "#);
        let groups = resolve_window_groups(&config, [
            window(1, Some("one.exe"), Some("C:/one.exe"), "Tool One"),
            window(2, Some("two.exe"), Some("C:/two.exe"), "Tool Two"),
        ]);
        let group = groups.values().next().expect("one named group expected");

        assert_eq!(group.windows, vec![1, 2]);
    }

    #[test]
    fn unmatched_executable_uses_resize_enabled_fallback() {
        let groups = resolve_window_groups(
            &RuntimeConfig::default(),
            [window(1, Some("app.exe"), Some("C:/app.exe"), "App")]);
        let group = groups.values().next().expect("one fallback group expected");

        assert_eq!((group.allow_resize, group.default_size), (true, None));
    }

    #[test]
    fn windows_without_executable_metadata_share_unknown_group() {
        let groups = resolve_window_groups(&RuntimeConfig::default(), [
            window(1, None, None, "Unknown One"),
            window(2, None, None, "Unknown Two"),
        ]);

        assert_eq!(
            groups[&ResolvedGroupId::Executable(ExecutableGroupId::Unknown)].windows,
            vec![1, 2]);
    }
}
