//! Backend-independent action orchestration and snapshot lifecycle.
//!
//! Native observations remain independent of configured rules. Updates replace owned
//! snapshots and compare their failures; borrowed views classify only when requested.

use crate::*;

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::hash::Hash;

use smol_str::*;

pub type WindowSnapshotCollection<H> =
    Vec<(H, anyhow::Result<WindowInfo<H>>)>;

/// Complete windows selected by one stable rule and case-insensitive program path.
///
/// All members borrow the original observation. The first matched window supplies the
/// representative program, preserving display casing without cloning its Rc or metadata.
#[derive(Debug)]
pub struct WindowGroup<'a, H> {
    pub rule_name: &'a SmolStr,
    pub program: &'a ProgramInfo,
    pub windows: Vec<&'a WindowInfo<H>>,
}

/// Supplies snapshots and interprets native actions for one platform.
///
/// Core forwards actions without dispatching variants. Platform adapters own native
/// reads, writes, and filtering; core owns orchestration and derives views of the results.
pub trait Backend {
    /// Cheap native identity, stable independently of changing display metadata.
    ///
    /// Display provides a best-effort description even when capture failed or the
    /// window disappeared. It must not fail merely because metadata is unavailable.
    type Handle: fmt::Debug + fmt::Display + Copy + Eq + Hash + 'static;

    /// Captures relevant windows and their failures in backend enumeration order.
    ///
    /// Each included handle occurs once and equals WindowInfo.handle on success.
    /// Excluded windows are omitted; unmatched successes remain for derived views.
    /// Required-query failures retain the handle beside the original error, while an
    /// outer failure invalidates the entire enumeration. Order may change between calls.
    fn snapshot(&mut self)
     -> anyhow::Result<WindowSnapshotCollection<Self::Handle>>;

    /// Performs one action against live state, retaining contextual native failures.
    /// Implementations must panic for an unsupported future action variant.
    fn perform(&mut self, target: Self::Handle, action: WindowAction)
     -> anyhow::Result<()>;
}

/// Owns native observations and advances them only through explicit updates.
///
/// Actions run before capture so the next observation reflects accepted operations.
/// Presentation decides when to update; grouping is a pure, uncached borrowed query.
pub struct Engine<B: Backend> {
    backend: B,
    rules: Vec<Rule>,
    snapshot: anyhow::Result<WindowSnapshotCollection<B::Handle>>,
    pending_actions: Vec<(B::Handle, WindowAction)>,
}

/// A rule-dependent view borrowing one immutable engine snapshot.
///
/// Groups and error entries allocate borrowed projections of the ordered observation.
/// Windows, program metadata, rule names, and failures still belong to the engine.
/// Group members and error entries each retain their relative backend enumeration order.
#[derive(Debug)]
pub struct EngineView<'a, H> {
    pub groups: Vec<WindowGroup<'a, H>>,
    pub errors: Vec<(H, &'a anyhow::Error)>,
}

impl<B: Backend> Engine<B> {
    /// Creates an engine with no native capture yet and an empty successful snapshot.
    /// Callers provide rules from [`config_parser::parse_config`] or verify them first.
    pub const fn new(rules: Vec<Rule>, backend: B) -> Self {
        Self {
            backend,
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
    pub const fn snapshot(&self) -> &anyhow::Result<WindowSnapshotCollection<B::Handle>> {
        &self.snapshot
    }

    /// Classifies captured successes without native queries, logging, or cached state.
    /// Enumeration failure propagates instead of masquerading as an empty matched view.
    pub fn view(&self) -> Result<EngineView<'_, B::Handle>, &anyhow::Error> {
        self.snapshot.as_ref().map(|snapshot| EngineView {
            groups: group_windows(&self.rules,
                snapshot.iter().filter_map(|&(_, ref result)| result.as_ref().ok())),
            errors: snapshot.iter().filter_map(|&(handle, ref result)| {
                result.as_ref().err().map(|error| (handle, error))
            }).collect(),
        })
    }

    /// Defers native operations until the next update in iterator order.
    pub fn queue<I>(&mut self, actions: I)
    where
        I: IntoIterator<Item = (B::Handle, WindowAction)>, {
        self.pending_actions.extend(actions);
    }

    /// Returns whether user work should trigger an update before the periodic deadline.
    pub const fn has_pending_actions(&self) -> bool { !self.pending_actions.is_empty() }

    /// Attempts the complete queued batch, then replaces the raw snapshot on any outcome.
    ///
    /// Each action failure is logged, while snapshot failures are compared only with the
    /// preceding observation. An enumeration outage discards per-window dedup history;
    /// resumed window failures are therefore reported again. No grouping runs here.
    pub fn update(&mut self) {
        for (handle, action) in self.pending_actions.drain(..) {
            if let Err(error) = self.backend.perform(handle, action) {
                log::error!("failed to perform action {action:?} on {handle}: {error:#}");
            }
        }

        let next = self.backend.snapshot();
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
            windows,
        }));
    }
    groups
}
