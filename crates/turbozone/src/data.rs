//! Concrete application state derived from native window snapshots.

use std::collections::BTreeMap;

use turbozone_core::RuntimeConfig;
use turbozone_windows::WindowInfo;

/// The page currently replacing the application body.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WindowPage {
    /// Matched rule-and-executable sections.
    #[default]
    Sections,
    /// Known unmatched and path-unavailable diagnostics.
    Unmatched,
}

/// Windows matched by one rule and sharing one executable identity.
#[derive(Debug)]
pub struct WindowSection {
    /// Source-order index into the runtime rule vector for this snapshot.
    pub rule_index: usize,
    /// Lowercased normalized path used in persistent section identity.
    pub executable_path: String,
    /// Owned native snapshots belonging to this section.
    pub windows: Vec<WindowInfo>,
}

/// A complete, disjoint classification of one native window snapshot.
#[derive(Debug, Default)]
pub struct SectionedWindows {
    /// Matched sections in rule source order and then executable-path order.
    pub sections: Vec<WindowSection>,
    /// Windows with paths which matched no rule.
    pub unmatched_windows: Vec<WindowInfo>,
    /// Windows rejected from matching because their executable path was unavailable.
    pub unknown_windows: Vec<WindowInfo>,
}

impl SectionedWindows {
    /// Consumes native snapshots and moves every window into exactly one destination.
    pub fn from_windows(config: &RuntimeConfig, windows: Vec<WindowInfo>) -> Self {
        let mut matched = BTreeMap::<(usize, String), Vec<WindowInfo>>::new();
        let mut unmatched_windows = Vec::new();
        let mut unknown_windows = Vec::new();

        for window in windows {
            let Some(executable_path) = window.executable_path.as_deref() else {
                unknown_windows.push(window);
                continue;
            };
            let executable_path = executable_path.to_lowercase();
            let executable_name = window.executable_name.as_deref().map(str::to_lowercase);
            let rule_index = config.matching_rule_index(
                executable_name.as_deref(),
                &executable_path,
                &window.window_title,
                window.client_size);
            let Some(rule_index) = rule_index else {
                unmatched_windows.push(window);
                continue;
            };
            matched.entry((rule_index, executable_path))
                .or_default()
                .push(window);
        }

        let sections = matched.into_iter()
            .map(|((rule_index, executable_path), windows)| WindowSection {
                rule_index,
                executable_path,
                windows,
            })
            .collect();
        Self {
            sections,
            unmatched_windows,
            unknown_windows,
        }
    }

    /// Returns the number of windows shown on the diagnostic replacement page.
    pub const fn diagnostic_count(&self) -> usize {
        self.unmatched_windows.len() + self.unknown_windows.len()
    }
}
