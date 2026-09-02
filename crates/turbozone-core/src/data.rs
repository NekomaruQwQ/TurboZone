//! Stable, backend-generic sections derived from immutable window snapshots.

use std::collections::BTreeMap;

use smol_str::{SmolStr, StrExt as _};

use crate::{RuntimeConfig, WindowInfo};

/// Windows selected by one stable rule name and sharing one program identity.
pub struct WindowSection<H> {
    /// Stable rule identity; source-array position is deliberately not retained.
    pub rule_name: SmolStr,
    /// Lowercased program path used in persistent section identity.
    pub program_path: SmolStr,
    /// Complete native snapshots belonging to this section.
    pub windows: Vec<WindowInfo<H>>,
}

/// Groups complete matches in rule source order and then program-path order.
///
/// Rule names are the only durable lookup key. The nested map isolates program
/// grouping from source order, allowing future config edits to reorder rules without
/// redirecting an existing section through an unstable array position.
pub fn group_windows<H>(
    config: &RuntimeConfig,
    windows: Vec<WindowInfo<H>>) -> Vec<WindowSection<H>> {
    let mut matched = BTreeMap::<SmolStr, BTreeMap<SmolStr, Vec<WindowInfo<H>>>>::new();
    for window in windows {
        let Ok(ref detail) = window.detail else { continue; };
        let program_path = detail.program_path.to_lowercase_smolstr();
        let program_name = detail.program_name.to_lowercase_smolstr();
        let Some(rule_name) = config.matching_rule_name(
            Some(&program_name), &program_path, &window.title, Some(detail.content_rect.size)) else {
            continue;
        };
        matched.entry(rule_name.clone())
            .or_default()
            .entry(program_path)
            .or_default()
            .push(window);
    }

    let mut sections = Vec::new();
    for rule in &config.rules {
        let Some(programs) = matched.remove(&rule.name) else { continue; };
        sections.extend(programs.into_iter().map(|(program_path, windows)| WindowSection {
            rule_name: rule.name.clone(),
            program_path,
            windows,
        }));
    }
    sections
}
