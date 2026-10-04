//! Simulated typing, for measuring the corrector and producing training data
//! (RESEARCH.md, section 3).
//!
//! A typist model ([`typist`]) turns sentences into keystrokes, slips
//! included. The app's own [`ac_core::Engine`] handles every key, and a screen
//! model ([`screen`]) applies what the engine sends back. A second lane types
//! the same keys with corrections off, which tells what each word looked like
//! as typed; every word then ends in one [`sim::Outcome`].

pub mod screen;
pub mod sim;
pub mod typist;

/// Small deterministic generator (xorshift): the same seed gives the same run.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// A number in `0..n` (`n` > 0).
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }

    pub fn chance(&mut self, p: f64) -> bool {
        (self.next_u64() % 1_000_000) as f64 / 1_000_000.0 < p
    }
}
