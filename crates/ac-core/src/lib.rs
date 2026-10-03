//! Platform-agnostic autocorrect engine.
//!
//! The platform layer translates OS key events into [`Key`]s, feeds them to
//! [`Engine::on_key`], and executes the returned [`Action`].

mod bigrams;
mod trigrams;
mod corrector;
mod edits;
mod engine;
mod kn;
mod lexicon;
mod personal;
mod smart;

pub use trigrams::Trigrams;
pub use bigrams::{Bigrams, HELD_OUT_SENTENCES};
pub use corrector::{Corrector, DictCorrector};
pub use engine::{Action, Decision, Engine, Key};
pub use kn::Kn;
pub use lexicon::Lexicon;
pub use personal::Personal;
pub use smart::{Ranking, SmartCorrector, Tuning};
