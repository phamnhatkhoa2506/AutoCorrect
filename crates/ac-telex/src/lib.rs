//! Vietnamese input: syllable structure and the Telex input method.
//!
//! Everything here is pure functions over strings so it can be tested
//! exhaustively without any OS hook.

pub mod syllable;
pub mod telex;

pub use syllable::{is_valid_word, Tone};
pub use telex::{compose, Composition, Kind};
