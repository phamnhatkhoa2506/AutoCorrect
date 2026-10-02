//! Vietnamese input: syllable structure and the Telex input method.
//!
//! Everything here is pure functions over strings so it can be tested
//! exhaustively without any OS hook.

pub mod syllable;
pub mod telex;
pub mod vni;

pub use syllable::{canonical, is_valid_word, Tone};
pub use telex::{compose, to_keys, Composition, Kind};

/// How Vietnamese is typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Method {
    #[default]
    Telex,
    Vni,
}

impl Method {
    /// Composes the raw keys of one word.
    pub fn compose(self, raw: &str) -> Composition {
        match self {
            Method::Telex => compose(raw),
            Method::Vni => vni::compose_vni(raw),
        }
    }

    /// The keys that type `text`.
    pub fn keys_for(self, text: &str) -> String {
        match self {
            Method::Telex => to_keys(text),
            Method::Vni => vni::to_keys(text),
        }
    }

    /// The same keys as Telex keys, which the corrector reads. Words that are
    /// not Vietnamese are left as typed.
    pub fn telex_keys(self, raw: &str) -> String {
        match self {
            Method::Telex => raw.to_string(),
            Method::Vni => {
                let telex = vni::to_telex(raw);
                if compose(&telex).kind == Kind::Literal {
                    raw.to_string()
                } else {
                    telex
                }
            }
        }
    }
}
