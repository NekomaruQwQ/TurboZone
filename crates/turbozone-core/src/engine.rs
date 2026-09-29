//! Application-facing contracts and borrowed presentation data.
//!
//! Engines own observations and derive action availability. Presentation controls timing,
//! renders these values, and returns commands against the identities captured by the view.

use euclid::default::Size2D;
use smol_str::SmolStr;
use crate::{Platform, ProgramInfo, ResizeSelector, WindowAction, WindowInfo};

/// Ordered complete captures and per-window failures from one platform observation.
pub type WindowSnapshotCollection<H> = Vec<(H, anyhow::Result<WindowInfo<H>>)>;

/// The narrow application service consumed by presentation, generic on one platform.
///
/// Platform identities are reused directly. The service has no clock, framework, or native
/// implementation dependency; its generic batch method is intended for static dispatch.
pub trait Engine<P: Platform> {
    /// Drains actions in order, isolates failures, and replaces the snapshot on every outcome.
    /// Snapshot errors are logged only when they differ from the preceding observation.
    fn update(&mut self);

    /// Derives a borrowed view without native queries, logging, or persistent view state.
    /// Enumeration failure returns the original borrowed error instead of stale groups.
    fn view(&self) -> Result<EngineView<'_, P::Window>, &anyhow::Error>;

    /// Appends commands in iterator order without executing or reselecting their targets.
    fn queue(
        &mut self,
        actions: impl IntoIterator<Item = (P::Window, WindowAction)>);

    /// Signals that presentation should bypass its ordinary polling delay.
    fn has_pending_actions(&self) -> bool;
}

/// Distinguishes rule permission from whether an observed target already satisfies it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionState {
    /// The rule does not permit this action.
    Unavailable,
    /// The action is permitted and at least one observed target does not satisfy it.
    Available,
    /// Every observed target already satisfies the permitted action.
    Satisfied,
}

/// A usable primary size and its satisfaction for one window or an entire group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResizeTarget {
    pub size: Size2D<i32>,
    pub satisfied: bool,
}

/// Effective resize controls derived from a rule without modifying its authored values.
///
/// Exact mode excludes a selector; selector mode may omit an unusable default while
/// retaining its bounds. Presentation intersects these bounds with its preset catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResizeAvailability {
    Disabled,
    Exact(ResizeTarget),
    Selector {
        primary: Option<ResizeTarget>,
        settings: ResizeSelector,
    },
}

/// Engine-derived controls for a nonempty selection. Presentation does not resolve rules
/// or compare geometry to decide permission or primary-action satisfaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionAvailability {
    pub center: ActionState,
    pub resize: ResizeAvailability,
}

/// A successful observation paired with derived controls, borrowing original metadata.
#[derive(Debug)]
pub struct WindowView<'a, H> {
    pub info: &'a WindowInfo<H>,
    pub actions: ActionAvailability,
}

/// A nonempty rule/program selection in source-rule and lowercase-program-path order.
///
/// Members preserve enumeration order; the first member supplies representative casing.
/// Group satisfaction requires every member, while commands target all rendered members.
#[derive(Debug)]
pub struct WindowGroup<'a, H> {
    pub rule_name: &'a SmolStr,
    pub program: &'a ProgramInfo,
    pub actions: ActionAvailability,
    pub windows: Vec<WindowView<'a, H>>,
}

/// One derived view borrowing engine-owned observations, names, and original errors.
///
/// Only projection vectors and small action values are owned here; error entries preserve
/// their enumeration order independently of the UI's diagnostic presentation order.
#[derive(Debug)]
pub struct EngineView<'a, H> {
    pub groups: Vec<WindowGroup<'a, H>>,
    pub errors: Vec<(H, &'a anyhow::Error)>,
}
