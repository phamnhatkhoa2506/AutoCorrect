//! Words typed without the spaces between them ("quanheej" for "quan hệ"): the
//! keys are cut into two or more Vietnamese syllables, and the cut is taken only
//! if it clearly stands out.
//!
//! A syllable may start at any key, so there are many cuts; a beam over the cut
//! positions keeps the best few. Every syllable but the first must be likelier
//! after the one before it than on its own: two syllables that merely happen to
//! be valid next to each other ("xa" "vi" for "xavi") say nothing.

use super::*;

/// The longest syllable in keys ("nghiengs" with its tone key).
const MAX_SYLLABLE_KEYS: usize = 9;
/// Every syllable is at least this many keys ("an"): shorter ones would let
/// nearly any string be cut.
const MIN_SYLLABLE_KEYS: usize = 2;
/// Paths kept per cut position.
const BEAM: usize = 6;

#[derive(Clone)]
struct Path {
    score: f64,
    ids: Vec<u32>,
    texts: Vec<String>,
    /// The least a syllable after the first gains from the one before it.
    min_lift: f64,
}

impl SmartCorrector {
    /// `keys` cut into Vietnamese syllables, joined by spaces, if one cut clearly
    /// stands out; `None` otherwise. Meant for keys that are no word.
    pub(super) fn split_run(&self, keys: &str, history: &[&str]) -> Option<String> {
        if !self.vietnamese || !keys.is_ascii() || keys.len() < 2 * MIN_SYLLABLE_KEYS + 1 {
            return None;
        }
        let t = &self.tuning;
        let base = self.context_of(history);
        let n = keys.len();
        let mut paths: Vec<Vec<Path>> = vec![Vec::new(); n + 1];
        paths[0].push(Path { score: 0.0, ids: Vec::new(), texts: Vec::new(), min_lift: f64::INFINITY });
        for i in 0..n {
            let from = std::mem::take(&mut paths[i]);
            for path in &from {
                for j in i + MIN_SYLLABLE_KEYS..=n.min(i + MAX_SYLLABLE_KEYS) {
                    let c = compose(&keys[i..j]);
                    if c.kind != Kind::Vietnamese {
                        continue;
                    }
                    let (Some(id), Some(f)) = (self.vi.id(&c.text), self.vi.log_freq(&c.text)) else { continue };
                    if f < t.split_floor {
                        continue;
                    }
                    let ctx = match path.ids.as_slice() {
                        [] => base,
                        [only] => Context { vi: Some(*only), vi2: base.vi, vote: base.vote, ..Context::default() },
                        [.., before, last] => Context { vi: Some(*last), vi2: Some(*before), vote: base.vote, ..Context::default() },
                    };
                    let score = self.with_context(f, self.ngram_vi(ctx, id));
                    let mut next = path.clone();
                    // ln P, not ln count: a cut into more syllables must not score higher for that alone.
                    next.score += score - LN_BILLION;
                    if !path.ids.is_empty() {
                        next.min_lift = next.min_lift.min(score - f);
                    }
                    next.ids.push(id);
                    next.texts.push(c.text);
                    let at = &mut paths[j];
                    at.push(next);
                    at.sort_by(|a, b| b.score.total_cmp(&a.score));
                    at.truncate(BEAM);
                }
            }
        }
        let done = &paths[n];
        let best = done.first().filter(|p| p.ids.len() >= 2)?;
        let runner_up = done.get(1).map_or(f64::NEG_INFINITY, |p| p.score);
        (best.min_lift >= t.split_lift && best.score - runner_up >= t.split_margin).then(|| best.texts.join(" "))
    }
}
