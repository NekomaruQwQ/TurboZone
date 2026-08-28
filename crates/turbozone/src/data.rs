//! Concrete application state derived from native window snapshots.

use std::collections::BTreeMap;

use turbozone_core::{RuntimeConfig, WindowInfo};
use turbozone_windows::WindowHandle;

/// The page currently replacing the application body.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WindowPage {
    /// Matched rule-and-program sections.
    #[default]
    Sections,
    /// Unmatched windows and snapshots with unavailable details.
    Diagnostics,
}

/// Windows matched by one rule and sharing one program identity.
#[derive(Debug)]
pub struct WindowSection {
    /// Source-order index into the runtime rule vector for this snapshot.
    pub rule_index: usize,
    /// Lowercased normalized path used in persistent section identity.
    pub program_path: String,
    /// Owned native snapshots belonging to this section.
    pub windows: Vec<WindowInfo<WindowHandle>>,
}

/// A complete, disjoint classification of one native window snapshot.
#[derive(Debug, Default)]
pub struct SectionedWindows {
    /// Matched sections in rule source order and then program-path order.
    pub sections: Vec<WindowSection>,
    /// Windows with complete details which matched no rule.
    pub unmatched_windows: Vec<WindowInfo<WindowHandle>>,
    /// Windows excluded from matching because one or more detail queries failed.
    pub failed_windows: Vec<WindowInfo<WindowHandle>>,
}

impl SectionedWindows {
    /// Consumes native snapshots and moves every window into exactly one destination.
    pub fn from_windows(config: &RuntimeConfig, windows: Vec<WindowInfo<WindowHandle>>) -> Self {
        let mut matched = BTreeMap::<(usize, String), Vec<WindowInfo<WindowHandle>>>::new();
        let mut unmatched_windows = Vec::new();
        let mut failed_windows = Vec::new();

        for window in windows {
            let Ok(ref detail) = window.detail else {
                failed_windows.push(window);
                continue;
            };
            let program_path = detail.program_path.to_lowercase();
            let program_name = detail.program_name.to_lowercase();
            let rule_index = config.matching_rule_index(
                Some(&program_name),
                &program_path,
                &window.title,
                Some(detail.content_rect.size));
            let Some(rule_index) = rule_index else {
                unmatched_windows.push(window);
                continue;
            };
            matched.entry((rule_index, program_path))
                .or_default()
                .push(window);
        }

        let sections = matched.into_iter()
            .map(|((rule_index, program_path), windows)| WindowSection {
                rule_index,
                program_path,
                windows,
            })
            .collect();
        Self {
            sections,
            unmatched_windows,
            failed_windows,
        }
    }

    /// Returns the number of windows shown on the diagnostic replacement page.
    pub const fn diagnostic_count(&self) -> usize {
        self.unmatched_windows.len() + self.failed_windows.len()
    }
}

#[cfg(test)]
mod tests {
    use euclid::default::{Point2D, Rect, Size2D};
    use turbozone_core::{Config, Rule, Pattern, WindowDetail, WindowFilter, WindowState};

    use super::*;

    /// Builds a complete snapshot without touching native windows.
    fn window(title: &str) -> WindowInfo<WindowHandle> {
        WindowInfo {
            handle: WindowHandle::default(),
            title: title.to_owned(),
            state: WindowState::Normal,
            detail: Ok(WindowDetail {
                monitor_rect: Rect::new(Point2D::zero(), Size2D::new(1920, 1080)),
                content_rect: Rect::new(Point2D::zero(), Size2D::new(640, 480)),
                process_id: 1,
                program_path: "C:/Apps/App.exe".to_owned(),
                program_name: "App.exe".to_owned(),
            }),
        }
    }

    #[test]
    fn classification_keeps_matched_unmatched_and_failed_windows_disjoint() {
        let config = Config { rules: vec![Rule {
            name: "app".to_owned(),
            window: WindowFilter { title: Some(Pattern::Exact("Matched".to_owned())), ..Default::default() },
            ..Default::default()
        }] }.validate().unwrap();
        let mut failed = window("Failed");
        failed.detail = Err(vec!["Client geometry unavailable".to_owned()]);
        let windows = SectionedWindows::from_windows(
            &config, vec![window("Matched"), window("Unmatched"), failed]);
        assert_eq!(
            (
                windows.sections[0].windows[0].title.as_str(),
                windows.unmatched_windows[0].title.as_str(),
                windows.failed_windows[0].title.as_str(),
                windows.diagnostic_count()),
            ("Matched", "Unmatched", "Failed", 2));
    }

    #[test]
    fn failed_details_never_match_even_an_unfiltered_rule() {
        let config = Config { rules: vec![Rule { name: "all".to_owned(), ..Default::default() }] }
            .validate().unwrap();
        let mut failed = window("App");
        failed.detail = Err(vec!["Monitor geometry unavailable".to_owned()]);
        let windows = SectionedWindows::from_windows(&config, vec![failed]);
        assert!(windows.sections.is_empty() && windows.failed_windows.len() == 1);
    }

    #[test]
    fn recovered_details_reenter_matching_on_the_next_snapshot() {
        let config = Config { rules: vec![Rule { name: "all".to_owned(), ..Default::default() }] }
            .validate().unwrap();
        let mut failed = window("App");
        failed.detail = Err(vec!["Monitor geometry unavailable".to_owned()]);
        let first = SectionedWindows::from_windows(&config, vec![failed]);
        let next = SectionedWindows::from_windows(&config, vec![window("App")]);
        assert_eq!((first.diagnostic_count(), next.diagnostic_count(), next.sections.len()), (1, 0, 1));
    }

    #[test]
    fn program_section_identity_is_case_insensitive() {
        let config = Config { rules: vec![Rule { name: "all".to_owned(), ..Default::default() }] }
            .validate().unwrap();
        let mut other = window("Other");
        other.detail.as_mut().unwrap().program_path = "c:/apps/app.EXE".to_owned();
        let windows = SectionedWindows::from_windows(&config, vec![window("App"), other]);
        assert_eq!((windows.sections.len(), windows.sections[0].windows.len()), (1, 2));
    }
}
