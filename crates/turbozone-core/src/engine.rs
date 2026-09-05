//! Backend-independent action orchestration and snapshot lifecycle.

use std::{fmt, rc::Rc};
use std::hash::Hash;
use std::collections::BTreeMap;

use euclid::default::Size2D;
use smol_str::{SmolStr, StrExt as _, format_smolstr};

use crate::{ProgramInfo, Rule, SnapshotLogging, WindowInfo};

/// One native side effect accepted from a rendered snapshot.
///
/// Actions own exactly one handle so queues, ordering, and per-target failures remain
/// explicit. New variants may be added without allowing presentation crates to assume
/// they know the complete backend operation set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum WindowAction<H> {
    /// Sets the client area to an exact positive physical-pixel size.
    Resize(H, Size2D<i32>),
    /// Centers the live or restored client area in its current monitor work area.
    MoveToCenter(H),
}

impl<H: Copy> WindowAction<H> {
    /// Returns the native identity captured when the action was accepted.
    pub const fn handle(&self) -> H {
        match *self {
            Self::Resize(handle, _) | Self::MoveToCenter(handle) => handle,
        }
    }
}

/// Supplies snapshots and interprets native actions for one platform.
///
/// Core forwards [`WindowAction`] values without dispatching their variants. This keeps
/// platform-specific dispatch beside the native mechanisms while core retains queue
/// order, refresh policy, and non-fatal error handling.
pub trait Backend {
    /// Cheap, stable identity retained between a snapshot and its deferred action.
    ///
    /// Its [`fmt::Display`] implementation is used for logging and error messages,
    /// so it should provide a human-readable representation of the native window
    /// identity as much as possible.
    type Handle:
        fmt::Debug +
        fmt::Display +
        Copy + PartialEq + Eq + Hash + 'static;

    /// Captures the currently relevant application windows.
    fn snapshot(&mut self) -> anyhow::Result<Vec<WindowInfo<Self::Handle>>>;

    /// Performs one action against live native state.
    ///
    /// Implementations return operational failures and must panic for an unsupported
    /// future action variant rather than silently accepting an operation they did not
    /// perform.
    fn perform(&mut self, action: WindowAction<Self::Handle>) -> anyhow::Result<()>;
}

/// Owns product state and advances it only through explicit logic ticks.
///
/// Presentation code may queue work and inspect the latest sections, but only a tick
/// performs native effects or replaces the snapshot. Actions run before refresh so the
/// resulting snapshot reflects accepted user operations.
pub struct Engine<B: Backend> {
    backend: B,
    rules: Vec<Rule>,
    groups: Vec<Group<B::Handle>>,
    pending_actions: Vec<WindowAction<B::Handle>>,
    logging: SnapshotLogging<B::Handle>,
}

impl<B: Backend> Engine<B> {
    /// Creates an unticked engine taking ownership of verified rules and the backend.
    /// Callers supply rules from [`crate::parse_config`] or verify their config first.
    /// Borrowed access keeps authored values stable for matching and presentation.
    pub fn new(rules: Vec<Rule>, backend: B) -> Self {
        Self {
            backend,
            rules,
            groups: Vec::new(),
            pending_actions: Vec::new(),
            logging: SnapshotLogging::default(),
        }
    }

    /// Returns the validated rules in configuration source order.
    pub fn rules(&self) -> &[Rule] { &self.rules }

    /// Resolves a verified rule through its stable configuration identity.
    pub fn rule(&self, name: &str) -> Option<&Rule> {
        self.rules.iter().find(|rule| rule.name == name)
    }

    /// Returns the latest successfully grouped snapshot.
    pub fn groups(&self) -> &[Group<B::Handle>] { &self.groups }

    /// Defers native operations until the next logic tick in iterator order.
    pub fn queue(&mut self, actions: impl IntoIterator<Item = WindowAction<B::Handle>>) {
        self.pending_actions.extend(actions);
    }

    /// Returns whether user work should trigger a tick before the periodic deadline.
    pub const fn has_pending_actions(&self) -> bool { !self.pending_actions.is_empty() }

    /// Applies queued operations and refreshes the complete derived view state.
    ///
    /// Individual action and snapshot failures are logged as non-fatal. A failed
    /// top-level snapshot clears sections so the UI never presents older data as live.
    pub fn tick(&mut self) {
        for action in std::mem::take(&mut self.pending_actions) {
            let handle = action.handle();
            let identity = self.window_identity(handle);
            if let Err(error) = self.backend.perform(action) {
                log::error!("window action failed for {identity}: {error:#}");
            }
        }

        let windows = match self.backend.snapshot() {
            Ok(windows) => windows,
            Err(error) => {
                self.logging.enumeration_failed(format_smolstr!("{error}"));
                self.groups.clear();
                return;
            },
        };
        self.logging.update(&windows);
        self.groups = group_windows(&self.rules, windows);
    }

    /// Returns the backend after consuming the engine, primarily for adapter tests.
    pub fn into_backend(self) -> B { self.backend }

    /// Owns cached metadata before the backend consumes the action.
    ///
    /// The identity must outlive the snapshot borrow because native execution may
    /// invalidate it; compact immutable text keeps that boundary explicit.
    fn window_identity(&self, handle: B::Handle) -> SmolStr {
        let window = self.groups.iter()
            .flat_map(|section| &section.windows)
            .find(|window| window.handle == handle);
        let Some(window) = window else {
            return format_smolstr!("{handle:?}");
        };
        if let Ok(detail) = window.detail.as_ref() {
            format_smolstr!(
                "{handle:?} title={:?} executable={:?}",
                window.title,
                detail.program.path)
        } else {
            format_smolstr!("{handle:?} title={:?}", window.title)
        }
    }
}

/// Complete windows selected by one stable rule and case-insensitive program path.
///
/// The representative program snapshot comes from the first matched window and
/// preserves its display casing. Sharing it through [`Rc`] lets presentation code
/// render group metadata without recovering it from an arbitrary window.
pub struct Group<H> {
    /// Stable rule identity; source-array position is deliberately not retained.
    pub rule_name: SmolStr,
    /// Representative immutable metadata for the grouped program.
    pub program: Rc<ProgramInfo>,
    /// Complete native snapshots belonging to this group.
    pub windows: Vec<WindowInfo<H>>,
}

/// Returns the winning rule name for a complete native window snapshot.
///
/// Higher priorities replace an existing winner while equal priorities retain
/// source order. Incomplete snapshots cannot match because their program and client
/// geometry are not trustworthy inputs to the configured filters.
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

/// Returns whether every configured filter accepts a complete window snapshot.
/// The rule must come from a verified configuration; incomplete details fail closed.
pub fn matches_rule<H>(rule: &Rule, window: &WindowInfo<H>) -> bool {
    let Ok(detail) = window.detail.as_ref() else { return false; };
    matches_program(rule, detail.program.as_ref()) && matches_window(rule, window)
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

/// Matches title and client-area filters against one complete window snapshot.
fn matches_window<H>(rule: &Rule, window: &WindowInfo<H>) -> bool {
    let Ok(detail) = window.detail.as_ref() else { return false; };
    let filters = &rule.window;
    if !filters.title.as_ref().is_none_or(|pattern| {
        pattern.matches(&window.title)
    }) {
        return false;
    }
    let size = detail.content_rect.size;
    filters.min.is_none_or(|[min_width, min_height]| {
        size.width >= min_width && size.height >= min_height
    }) && filters.max.is_none_or(|[max_width, max_height]| {
        size.width <= max_width && size.height <= max_height
    })
}

/// Groups complete matches in rule source order and then program-path order.
///
/// Rule names are the only durable rule lookup key. Lowercase paths remain internal
/// grouping and ordering keys, while each resulting [`Group`] retains the first
/// window's display-preserving program snapshot.
pub fn group_windows<H>(
    rules: &[Rule],
    windows: Vec<WindowInfo<H>>) -> Vec<Group<H>> {
    let mut matched = BTreeMap::<
        SmolStr,
        BTreeMap<SmolStr, (Rc<ProgramInfo>, Vec<WindowInfo<H>>)>>::new();
    for window in windows {
        let Ok(ref detail) = window.detail else { continue; };
        let program_path = detail.program.path.to_lowercase_smolstr();
        let program = Rc::clone(&detail.program);
        let Some(rule_name) = matching_rule_name(rules, &window) else {
            continue;
        };
        matched.entry(rule_name.clone())
            .or_default()
            .entry(program_path)
            .or_insert_with(|| (program, Vec::new()))
            .1
            .push(window);
    }

    let mut groups = Vec::new();
    for rule in rules {
        let Some(programs) = matched.remove(&rule.name) else { continue; };
        groups.extend(programs.into_values().map(|(program, windows)| Group {
            rule_name: rule.name.clone(),
            program,
            windows,
        }));
    }
    groups
}
