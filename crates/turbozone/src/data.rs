//! Matched sections derived from native snapshots, without diagnostic presentation state.

use std::collections::BTreeMap;

use turbozone_core::{RuntimeConfig, WindowInfo};
use turbozone_windows::WindowHandle;

/// Windows matched by one rule and sharing one program identity.
pub struct WindowSection {
    /// Index into the active runtime rule vector, after rejected rules are removed.
    pub rule_index: usize,
    /// Lowercased program path used in persistent section identity.
    pub program_path: String,
    /// Complete native snapshots belonging to this section.
    pub windows: Vec<WindowInfo<WindowHandle>>,
}

/// Groups only matched, complete windows in rule order and then program-path order.
/// Callers report query failures before passing snapshots here; unmatched windows are normal.
pub fn group_windows(config: &RuntimeConfig, windows: Vec<WindowInfo<WindowHandle>>) -> Vec<WindowSection> {
    let mut matched = BTreeMap::<(usize, String), Vec<WindowInfo<WindowHandle>>>::new();
    for window in windows {
        let Ok(ref detail) = window.detail else { continue; };
        let program_path = detail.program_path.to_lowercase();
        let program_name = detail.program_name.to_lowercase();
        let Some(rule_index) = config.matching_rule_index(
            Some(&program_name), &program_path, &window.title, Some(detail.content_rect.size)) else {
            continue;
        };
        matched.entry((rule_index, program_path)).or_default().push(window);
    }
    matched.into_iter().map(|((rule_index, program_path), windows)| WindowSection {
        rule_index, program_path, windows,
    }).collect()
}
