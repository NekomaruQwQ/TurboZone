//! Private engine observations for the external-file unit suite.

use super::{Backend, Engine, Rule};

trait EngineTestExt<B: Backend> {
    fn rules(&self) -> &[Rule];
    fn into_backend(self) -> B;
}

impl<B: Backend> EngineTestExt<B> for Engine<B> {
    fn rules(&self) -> &[Rule] { &self.rules }
    fn into_backend(self) -> B { self.backend }
}

#[path = "../unit/engine.rs"]
mod cases;
