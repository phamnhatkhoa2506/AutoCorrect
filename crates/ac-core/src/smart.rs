//! Noisy-channel corrector over the raw keys of a word, for Vietnamese
//! (through the Telex composer) and English at once.
//!
//! score(candidate) = ln P(word) - cost(slip that turns it into the keys typed)
//!
//! A correction is made only when the best candidate clearly beats both what
//! was typed and the runner-up: a missed fix costs a keystroke, a wrong fix
//! costs trust.

use std::collections::HashMap;

use ac_telex::{compose, Kind};

use crate::corrector::{match_case, Corrector};
use crate::edits::edits1;
use crate::lexicon::Lexicon;

/// Score of a well-formed Vietnamese syllable missing from the lexicon
/// (about 20 per billion).
const UNSEEN_SYLLABLE: f64 = 3.0;
/// Words typed at least this common (ln of ~1000 per billion) are left alone:
/// telling "git" from a slip of "it" needs context (Phase 2). Misspellings
/// that leaked into the corpora ("untill", "mổi") sit below it.
const KNOWN_WORD: f64 = 6.9;
/// Discount on a typed word rarer than [`KNOWN_WORD`].
const RARE_TYPED_PENALTY: f64 = 1.5;
/// A correction must reach this score (≈ a word of ~20k per billion after a
/// typical slip), which keeps rare words from replacing unknown tokens.
const FLOOR: f64 = 5.5;
/// Required lead of the best candidate over what was typed...
const MARGIN: f64 = 1.0;
/// ...and over the runner-up.
const AMBIGUITY: f64 = 1.0;
/// Shorter tokens are too ambiguous to fix.
const MIN_KEYS: usize = 3;

pub struct SmartCorrector {
    vi: Lexicon,
    en: Lexicon,
    vietnamese: bool,
}

/// Scores behind a decision, for diagnostics.
#[derive(Debug)]
pub struct Ranking {
    /// Score of the word as typed (`-inf` if it is not a known word).
    pub typed: f64,
    /// Candidates, best first.
    pub candidates: Vec<(String, f64)>,
}

impl SmartCorrector {
    pub fn new(vi: Lexicon, en: Lexicon) -> Self {
        Self { vi, en, vietnamese: true }
    }

    /// With Vietnamese off, only English corrections are proposed.
    pub fn set_vietnamese(&mut self, on: bool) {
        self.vietnamese = on;
    }

    /// Best known reading per text of a key sequence: (text, ln frequency).
    fn readings(&self, keys: &str) -> Vec<(String, f64)> {
        let mut out = Vec::with_capacity(2);
        if let Some(f) = self.en.log_freq(keys) {
            out.push((keys.to_string(), f));
        }
        if let Some(text) = self.vietnamese_text(keys) {
            if let Some(f) = self.vi.log_freq(&text) {
                match out.iter_mut().find(|(t, _)| *t == text) {
                    Some(e) => e.1 = e.1.max(f),
                    None => out.push((text, f)),
                }
            }
        }
        out
    }

    fn vietnamese_text(&self, keys: &str) -> Option<String> {
        let c = compose(keys);
        (self.vietnamese && c.kind == Kind::Vietnamese).then_some(c.text)
    }

    /// Lowercase keys of `word` if it is worth scoring at all.
    fn keys_of(word: &str) -> Option<String> {
        let keys = word.to_lowercase();
        let plain = keys.chars().all(|c| c.is_ascii_lowercase());
        (plain && keys.chars().count() >= MIN_KEYS).then_some(keys)
    }

    /// Score of the keys as typed, and their Vietnamese reading if any.
    fn typed(&self, keys: &str) -> (f64, Option<String>) {
        let vietnamese = self.vietnamese_text(keys);
        let score = self
            .readings(keys)
            .into_iter()
            .map(|(_, f)| f)
            .chain(vietnamese.as_ref().map(|_| UNSEEN_SYLLABLE))
            .fold(f64::NEG_INFINITY, f64::max);
        (score, vietnamese)
    }

    /// Corrections of `keys`, best first.
    fn candidates(&self, keys: &str, vietnamese: Option<String>) -> Vec<(String, f64)> {
        // A well-formed syllable was typed: Vietnamese syllables are so dense
        // that changing a letter almost always lands on another one ("khoi" ->
        // "khi"), a real-word correction that needs context. Only fix marks.
        let syllable = vietnamese.is_some();
        let typed_texts: Vec<String> = std::iter::once(keys.to_string()).chain(vietnamese).collect();

        // Several slips can lead to the same word: their probabilities add up.
        let mut scores: HashMap<String, f64> = HashMap::new();
        for slip in edits1(keys).into_iter().filter(|s| s.marks_only || !syllable) {
            for (text, f) in self.readings(&slip.keys) {
                if typed_texts.contains(&text) {
                    continue;
                }
                let e = scores.entry(text).or_insert(f64::NEG_INFINITY);
                *e = log_add(*e, f - slip.cost);
            }
        }
        let mut candidates: Vec<(String, f64)> = scores.into_iter().collect();
        candidates.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        candidates
    }

    /// Scores every correction of `word` (raw keys), for diagnostics.
    pub fn rank(&self, word: &str) -> Option<Ranking> {
        let keys = Self::keys_of(word)?;
        let (typed, vietnamese) = self.typed(&keys);
        Some(Ranking { typed, candidates: self.candidates(&keys, vietnamese) })
    }
}

impl Corrector for SmartCorrector {
    fn correct(&self, word: &str) -> Option<String> {
        let keys = Self::keys_of(word)?;
        let (typed, vietnamese) = self.typed(&keys);
        // Fast path for nearly every word: it is a common, correct one.
        if typed >= KNOWN_WORD {
            return None;
        }
        // A capitalised known word is most likely a name ("Tuan").
        let capitalised = word.chars().next().is_some_and(char::is_uppercase);
        if capitalised && typed > f64::NEG_INFINITY {
            return None;
        }
        // Rare entries are often misspellings that leaked into the corpora.
        let typed = typed - RARE_TYPED_PENALTY;

        let candidates = self.candidates(&keys, vietnamese);
        let (best, score) = candidates.first()?;
        let runner_up = candidates.get(1).map_or(f64::NEG_INFINITY, |c| c.1);
        let confident = *score >= FLOOR && score - typed >= MARGIN && score - runner_up >= AMBIGUITY;
        confident.then(|| match_case(word, best))
    }
}

/// ln(e^a + e^b) without overflow.
fn log_add(a: f64, b: f64) -> f64 {
    let (hi, lo) = if a > b { (a, b) } else { (b, a) };
    if lo == f64::NEG_INFINITY {
        hi
    } else {
        hi + (lo - hi).exp().ln_1p()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corrector() -> SmartCorrector {
        SmartCorrector::new(
            Lexicon::parse(include_str!("../../../data/vi_syllables.tsv")),
            Lexicon::parse(include_str!("../../../data/en_words.tsv")),
        )
    }

    #[test]
    fn fixes_typos() {
        let c = corrector();
        for (typed, want) in [
            ("teh", "the"),
            ("Recieve", "Receive"),
            ("TEH", "THE"),
            ("dunhf", "dùng"),
            ("untill", "until"),
            ("occured", "occurred"),
            ("definately", "definitely"),
            ("mooir", "mỗi"), // hỏi/ngã mixed up
            ("gruwi", "gửi"),
            ("nhnah", "nhanh"),
        ] {
            assert_eq!(c.correct(typed).as_deref(), Some(want), "{typed}: {:?}", c.rank(typed));
        }
    }

    #[test]
    fn leaves_good_words() {
        let c = corrector();
        for typed in [
            "hello", "the", "terminal", "dungf", "tieengs", "dduowcj", "khoong", "npm", "git",
            "kubectl", "cargo", "Tuan", "nhanh", "toi",
        ] {
            assert_eq!(c.correct(typed), None, "{typed}: {:?}", c.rank(typed));
        }
    }

    /// Known misses, kept visible so tuning can be measured against them:
    /// "seperate" (separate is rare in the corpora and the vowel slip costly),
    /// "thier" (their vs Vietnamese "thể" too close without context),
    /// "khogn" (không is two slips away).
    #[test]
    fn known_misses() {
        let c = corrector();
        for typed in ["seperate", "thier", "khogn"] {
            assert_eq!(c.correct(typed), None, "{typed} now corrected: update this test");
        }
    }

    /// Prints decisions for a word list: `cargo test -p ac-core explore -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn explore() {
        let c = corrector();
        for w in [
            "teh", "recieve", "dunhf", "mooir", "gruwi", "toi", "khoi", "lops", "ws", "npm", "kubectl",
            "cargo", "Tuan", "nhanh", "nhnah", "vieejt", "ddungs", "thuowng", "hte", "adn", "waht",
            "becuase", "definately", "occured", "thier", "jsut", "taht", "wiht", "khogn", "duowcj",
        ] {
            let r = c.rank(w);
            let top: Vec<_> = r.iter().flat_map(|r| r.candidates.iter().take(3)).collect();
            println!("{w:>10} -> {:<12} typed {:>6.2} top {top:?}", format!("{:?}", c.correct(w)),
                r.as_ref().map_or(f64::NAN, |r| r.typed));
        }
    }

    /// `cargo test -p ac-core --release speed -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn speed() {
        let c = corrector();
        for w in ["teh", "dunhf", "definately", "nguyeenx", "kubectl"] {
            let start = std::time::Instant::now();
            for _ in 0..100 {
                std::hint::black_box(c.correct(w));
            }
            println!("{w:>10}: {:>7.1} us/word", start.elapsed().as_secs_f64() * 1e6 / 100.0);
        }
    }

    #[test]
    fn english_only_mode_skips_vietnamese() {
        let mut c = corrector();
        c.set_vietnamese(false);
        assert_eq!(c.correct("dunhf"), None);
        assert_eq!(c.correct("teh").as_deref(), Some("the"));
    }

    #[test]
    fn log_add_is_stable() {
        assert!((log_add(0.0, 0.0) - 2f64.ln()).abs() < 1e-12);
        assert_eq!(log_add(f64::NEG_INFINITY, 5.0), 5.0);
    }
}
