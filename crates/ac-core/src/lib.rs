//! Platform-agnostic autocorrect engine.
//!
//! The platform layer translates OS key events into [`Key`]s, feeds them to
//! [`Engine::on_key`], and executes the returned [`Action`].

mod corrector;
mod engine;

pub use corrector::{Corrector, DictCorrector};
pub use engine::{Action, Decision, Engine, Key};
