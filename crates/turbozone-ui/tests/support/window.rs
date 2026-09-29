//! Prepared application views for UI tests, with no parser or policy implementation.

use std::cell::RefCell;
use std::fmt::{Debug, Display};
use std::hash::Hash;
use std::marker::PhantomData;
use std::rc::Rc;
use std::time::Duration;
use euclid::default::{Point2D, Rect, Size2D};
use smol_str::SmolStr;
use turbozone_core::*;

/// Supplies the identity type only. UI tests must never call native operations.
pub struct TestPlatform<H>(PhantomData<H>);

impl<H: Debug + Display + Copy + Eq + Hash + 'static> Platform for TestPlatform<H> {
    type Window = H;

    fn snapshot(&mut self) -> anyhow::Result<WindowSnapshotCollection<H>> {
        anyhow::bail!("UI must use its engine, not the native platform")
    }

    fn perform(&mut self, _: H, _: WindowAction) -> anyhow::Result<()> {
        anyhow::bail!("UI must queue commands through its engine")
    }
}

/// A fixture group explicitly supplies independent group and window action states.
/// The fake never matches rules, compares geometry, or derives group satisfaction.
pub struct PreparedGroup<H> {
    pub name: SmolStr,
    pub actions: ActionAvailability,
    pub windows: Vec<(WindowInfo<H>, ActionAvailability)>,
}

/// Owns the data borrowed by one prepared EngineView, including original error chains.
pub struct Scene<H> {
    pub groups: Vec<PreparedGroup<H>>,
    pub errors: Vec<(H, anyhow::Error)>,
}

impl<H> Scene<H> {
    pub const fn empty() -> Self { Self { groups: Vec::new(), errors: Vec::new() } }

    pub fn group(windows: Vec<WindowInfo<H>>, actions: ActionAvailability) -> Self {
        Self {
            groups: vec![PreparedGroup {
                name: "app".into(),
                windows: windows.into_iter().map(|window| (window, actions.clone())).collect(),
                actions,
            }],
            errors: Vec::new(),
        }
    }
}

/// Records completed effects separately from queued commands to catch native work in paints.
pub struct Calls<H> {
    pub actions: Vec<(H, WindowAction)>,
    pub updates: usize,
}

/// Advances scripted views only on update and repeats the last view for stable desktop tests.
/// A configurable delay makes completion-based UI pacing observable without private clocks.
pub struct TestEngine<H> {
    calls: Rc<RefCell<Calls<H>>>,
    delay: Duration,
    scenes: Vec<anyhow::Result<Scene<H>>>,
    current: usize,
    next: usize,
    pending: Vec<(H, WindowAction)>,
}

impl<H> TestEngine<H> {
    pub const fn calls(&self) -> &Rc<RefCell<Calls<H>>> { &self.calls }

    pub const fn set_delay(&mut self, delay: Duration) { self.delay = delay; }

    pub fn new(scenes: Vec<anyhow::Result<Scene<H>>>) -> Self {
        assert!(!scenes.is_empty(), "a scripted engine needs at least one view");
        Self {
            calls: Rc::new(RefCell::new(Calls { actions: Vec::new(), updates: 0 })),
            delay: Duration::ZERO,
            scenes,
            current: 0,
            next: 0,
            pending: Vec::new(),
        }
    }
}

impl<H: Debug + Display + Copy + Eq + Hash + 'static> Engine<TestPlatform<H>> for TestEngine<H> {
    fn update(&mut self) {
        std::thread::sleep(self.delay);
        let mut calls = self.calls.borrow_mut();
        calls.updates += 1;
        calls.actions.append(&mut self.pending);
        self.current = self.next;
        self.next = (self.next + 1).min(self.scenes.len() - 1);
    }

    fn view(&self) -> Result<EngineView<'_, H>, &anyhow::Error> {
        self.scenes[self.current].as_ref().map(|scene| EngineView {
            groups: scene.groups.iter().map(|group| WindowGroup {
                rule_name: &group.name,
                program: &group.windows.first().expect("fixture groups are nonempty").0.program,
                actions: group.actions.clone(),
                windows: group.windows.iter().map(|&(ref info, ref actions)| WindowView {
                    info,
                    actions: actions.clone(),
                }).collect(),
            }).collect(),
            errors: scene.errors.iter().map(|&(handle, ref error)| (handle, error)).collect(),
        })
    }

    fn queue(&mut self, actions: impl IntoIterator<Item = (H, WindowAction)>) {
        self.pending.extend(actions);
    }

    fn has_pending_actions(&self) -> bool { !self.pending.is_empty() }
}

/// Makes a complete framework-independent snapshot for headless UI tests.
pub fn window(path: &str, title: &str) -> WindowInfo<u64> {
    WindowInfo {
        handle: 1,
        title: title.into(),
        state: WindowState::Normal,
        monitor_rect: Rect::new(Point2D::zero(), Size2D::new(1920, 1080)),
        content_rect: Rect::new(Point2D::zero(), Size2D::new(640, 480)),
        program: Rc::new(ProgramInfo {
            path: path.into(),
            name: path.rsplit('/').next().unwrap().into(),
            description: "App Description".into(),
        }),
    }
}
