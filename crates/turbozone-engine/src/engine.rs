//! Platform-independent orchestration, borrowed classification, and action availability.
//!
//! Updates own raw observations and compare failures. Queries interpret verified rules
//! without native effects; the UI consumes derived controls instead of reinterpreting rules.

use turbozone_core::*;
use std::collections::{BTreeMap, HashMap};
use smol_str::*;

/// Owns native observations and advances them only through explicit updates.
///
/// Actions run before capture so the next observation reflects accepted operations.
/// Presentation decides when to update; grouping is a pure, uncached borrowed query.
pub struct EngineImpl<P: Platform> {
    platform: P,
    rules: Vec<Rule>,
    snapshot: anyhow::Result<WindowSnapshotCollection<P::Window>>,
    pending_actions: Vec<(P::Window, WindowAction)>,
}

impl<P: Platform> EngineImpl<P> {
    /// Creates an engine with no native capture yet and an empty successful snapshot.
    /// Callers provide rules from [`crate::config_parser::parse_config`] or verify them first.
    pub const fn new(rules: Vec<Rule>, platform: P) -> Self {
        Self {
            platform,
            rules,
            snapshot: Ok(Vec::new()),
            pending_actions: Vec::new(),
        }
    }

    /// Resolves a verified rule through its stable configuration identity.
    pub fn rule(&self, name: &str) -> Option<&Rule> {
        self.rules.iter().find(|rule| rule.name == name)
    }

    /// Borrows the latest raw observation, preserving enumeration failure explicitly.
    pub const fn snapshot(&self) -> &anyhow::Result<WindowSnapshotCollection<P::Window>> {
        &self.snapshot
    }
}

impl<P: Platform> Engine<P> for EngineImpl<P> {
    /// Classifies captured successes without native queries, logging, or cached state.
    /// Enumeration failure propagates instead of masquerading as an empty matched view.
    fn view(&self) -> Result<EngineView<'_, P::Window>, &anyhow::Error> {
        self.snapshot.as_ref().map(|snapshot| EngineView {
            groups: group_windows(&self.rules,
                snapshot.iter().filter_map(|&(_, ref result)| result.as_ref().ok())),
            errors: snapshot.iter().filter_map(|&(handle, ref result)| {
                result.as_ref().err().map(|error| (handle, error))
            }).collect(),
        })
    }

    /// Defers native operations until the next update in iterator order.
    fn queue(&mut self, actions: impl IntoIterator<Item = (P::Window, WindowAction)>) {
        self.pending_actions.extend(actions);
    }

    /// Returns whether user work should trigger an update before the periodic deadline.
    fn has_pending_actions(&self) -> bool { !self.pending_actions.is_empty() }

    /// Attempts the complete queued batch, then replaces the raw snapshot on any outcome.
    ///
    /// Each action failure is logged, while snapshot failures are compared only with the
    /// preceding observation. An enumeration outage discards per-window dedup history;
    /// resumed window failures are therefore reported again. No grouping runs here.
    fn update(&mut self) {
        for (handle, action) in self.pending_actions.drain(..) {
            if let Err(error) = self.platform.perform(handle, action) {
                log::error!("failed to perform action {action:?} on {handle}: {error:#}");
            }
        }

        let next = self.platform.snapshot();
        match next {
            Ok(ref snapshot) => {
                // A temporary borrowed index keeps comparison linear when enumeration
                // order changes. The owned sequence remains the only snapshot state.
                let previous: HashMap<_, _> = self.snapshot.as_ref().ok().into_iter()
                    .flatten().filter_map(|&(handle, ref result)| {
                        result.as_ref().err().map(|error| (handle, error))
                    }).collect();
                for &(handle, ref result) in snapshot {
                    let &Err(ref error) = result else { continue; };
                    let previous_error = previous.get(&handle).copied();
                    if !same_error(previous_error, error) {
                        log::warn!("failed to snapshot window {handle}: {error:#}");
                    }
                }
            },
            Err(ref error) => {
                if !same_error(self.snapshot.as_ref().err(), error) {
                    log::error!("window enumeration failed: {error:#}");
                }
            },
        }
        self.snapshot = next;
    }
}

/// Compares full contextual chains while leaving the original errors available to callers.
/// Native descriptions are excluded: a changing title does not make an error new.
fn same_error(previous: Option<&anyhow::Error>, current: &anyhow::Error) -> bool {
    previous.is_some_and(|previous| {
        format_smolstr!("{previous:#}") == format_smolstr!("{current:#}")
    })
}

/// Returns the winning rule name for a complete native window snapshot.
/// Higher priorities replace the winner while equal priorities retain source order.
pub fn matching_rule_name<'a, H>(
    rules: &'a [Rule],
    window: &WindowInfo<H>) -> Option<&'a SmolStr> {
    let mut winner = None;
    for rule in rules {
        if !matches_rule(rule, window) {
            continue;
        }
        if winner.is_none_or(|(_, priority)| rule.priority > priority) {
            winner = Some((&rule.name, rule.priority));
        }
    }
    winner.map(|(name, _)| name)
}

/// Returns whether every configured filter accepts a complete snapshot.
/// Failed captures cannot enter this boundary; the rule must be verified first.
pub fn matches_rule<H>(rule: &Rule, window: &WindowInfo<H>) -> bool {
    matches_program(rule, &window.program) && matches_window(rule, window)
}

/// Selects case-insensitive evaluation for authored executable patterns.
/// Absent filters do not require converting their candidate fields.
fn matches_program(rule: &Rule, program: &ProgramInfo) -> bool {
    let filters = &rule.program;
    filters.name.as_ref().is_none_or(|pattern| {
        pattern.matches_ignore_case(&program.name)
    }) && filters.path.as_ref().is_none_or(|pattern| {
        pattern.matches_ignore_case(&program.path)
    })
}

/// Matches case-sensitive titles and inclusive client-area bounds on both axes.
fn matches_window<H>(rule: &Rule, window: &WindowInfo<H>) -> bool {
    let filters = &rule.window;
    if !filters.title.as_ref().is_none_or(|pattern| {
        pattern.matches(&window.title)
    }) {
        return false;
    }
    let size = window.content_rect.size;
    filters.min.is_none_or(|min| {
        size.width >= min.width && size.height >= min.height
    }) && filters.max.is_none_or(|max| {
        size.width <= max.width && size.height <= max.height
    })
}

/// Borrows complete matches in rule source order and then lowercase program-path order.
///
/// Unmatched windows stay in the caller's snapshot. Each emitted group is nonempty,
/// retains input order, and uses its first member's program metadata. Borrowed iterators
/// allow callers to select successes without collecting an intermediate window list.
pub fn group_windows<'a, H: 'a>(
    rules: &'a [Rule],
    windows: impl IntoIterator<Item = &'a WindowInfo<H>>) -> Vec<WindowGroup<'a, H>> {
    let mut matched = BTreeMap::<
        &SmolStr,
        BTreeMap<SmolStr, (&ProgramInfo, Vec<&WindowInfo<H>>)>>::new();
    for window in windows {
        let Some(rule_name) = matching_rule_name(rules, window) else { continue; };
        let program_path = window.program.path.to_lowercase_smolstr();
        matched.entry(rule_name)
            .or_default()
            .entry(program_path)
            .or_insert_with(|| (&window.program, Vec::new()))
            .1
            .push(window);
    }

    let mut groups = Vec::new();
    for rule in rules {
        let Some(programs) = matched.remove(&rule.name) else { continue; };
        groups.extend(programs.into_values().map(|(program, windows)| WindowGroup {
            rule_name: &rule.name,
            program,
            actions: action_availability(rule, &windows),
            windows: windows.into_iter().map(|info| WindowView {
                info,
                actions: action_availability(rule, &[info]),
            }).collect(),
        }));
    }
    groups
}

/// Derives controls for a nonempty selection from value semantics and observed geometry.
///
/// Group satisfaction requires every member; exact targets never expose a selector and
/// invalid selector defaults are omitted by ResizeRule without changing authored settings.
fn action_availability<H>(rule: &Rule, windows: &[&WindowInfo<H>]) -> ActionAvailability {
    let center = if !rule.relocate {
        ActionState::Unavailable
    } else if windows.iter().all(|window| window.is_centered()) {
        ActionState::Satisfied
    } else {
        ActionState::Available
    };
    let primary = rule.resize.primary_size().map(|size| ResizeTarget {
        size,
        satisfied: windows.iter().all(|window| window.content_rect.size == size),
    });
    let resize = if let Some(settings) = rule.resize.selector() {
        ResizeAvailability::Selector { primary, settings }
    } else if let Some(target) = primary {
        ResizeAvailability::Exact(target)
    } else {
        ResizeAvailability::Disabled
    };
    ActionAvailability { center, resize }
}
