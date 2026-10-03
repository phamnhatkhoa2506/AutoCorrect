//! Noisy-channel corrector over the raw keys of a word, for Vietnamese
//! (through the Telex composer) and English at once.
//!
//! score(candidate) = ln P(word | previous word) - cost(slips that turn it
//! into the keys typed)
//!
//! A correction is made only when the best candidate clearly beats both what
//! was typed and the runner-up: a missed fix costs a keystroke, a wrong fix
//! costs trust.

use std::collections::HashMap;

use ac_telex::syllable::split_tone;
use ac_telex::{compose, to_keys, Kind, Tone};

use crate::bigrams::Bigrams;
use crate::kn::Kn;
use crate::trigrams::Trigrams;
use crate::corrector::{match_case, Corrector};
use crate::edits::{bag_distance, edits1, letter_counts, slip_cost, FAR_MAX_COST};
use crate::lexicon::Lexicon;
use crate::personal::Personal;

/// Score of a well-formed Vietnamese syllable missing from the lexicon
/// (about 20 per billion).
const UNSEEN_SYLLABLE: f64 = 3.0;
/// Shorter tokens are too ambiguous to fix.
const MIN_KEYS: usize = 3;
/// Two-slip corrections are only tried for unknown words at least this long.
const FAR_MIN_KEYS: usize = 4;
/// ln(1e9): lexicon frequencies are per billion tokens.
const LN_BILLION: f64 = 20.723_265_836_946_41;

/// The knobs of the decision, public so `ac-bench` can sweep them.
#[derive(Debug, Clone, Copy)]
pub struct Tuning {
    /// Words typed at least this common (ln per billion; 6.9 is ~1000) are
    /// left alone: telling "git" from a slip of "it" would need a real-word
    /// model. Misspellings that leaked into the corpora ("untill", "mổi") sit
    /// below it.
    pub known_word: f64,
    /// The same for a word that is also a well-formed Vietnamese syllable:
    /// higher, since leaked misspellings there are tone slips that deserve a
    /// fix ("mổi" for "mỗi") while the syllable space is too dense to protect
    /// less.
    pub known_syllable: f64,
    /// Discount on a typed word rarer than `known_word`.
    pub rare_typed_penalty: f64,
    /// A correction must reach this score (5.5 is ~ a word of 20k per
    /// billion after a typical slip), which keeps rare words from replacing
    /// unknown tokens.
    pub floor: f64,
    /// Required lead of the best candidate over what was typed...
    pub margin: f64,
    /// ...and over the runner-up.
    pub ambiguity: f64,
    /// `floor` and `ambiguity` for two-slip corrections, which are far more
    /// likely to be wrong (a very common short word two slips away is not
    /// better evidence than the typo being a rare word).
    pub far_floor: f64,
    pub far_ambiguity: f64,
    /// Share of the probability mass the previous word gets: a pair seen
    /// after "in" is worth more than the word's overall frequency, but a word
    /// never seen after it is not ruled out.
    pub bigram_weight: f64,
    /// Share of the probability mass the two previous words get, where the
    /// triple was seen: the rest is the pair-and-frequency estimate. 0 means
    /// pairs only.
    pub trigram_weight: f64,
    /// Use the Kneser-Ney tables instead of mixing pairs and triples with
    /// fixed weights (when the tables are loaded).
    pub kn: bool,
    /// Penalty for a candidate of the other language than the previous word
    /// (an English word after an English word is more likely English).
    pub language_penalty: f64,
    /// How much the words before the previous one count towards the language
    /// of the phrase: each step back counts this fraction of the one after
    /// it. 0 looks at the previous word only.
    pub phrase_decay: f64,
    /// A word typed without marks gets them only if the best accented form
    /// beats the bare word by this much (ln units: 5 is about 150 times as
    /// likely)...
    pub restore_margin: f64,
    /// ...and the second best accented form by this much.
    pub restore_ambiguity: f64,
    /// Bare words that are also English words at least this common ("the",
    /// "do", "can") are left alone.
    pub restore_english: f64,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            known_word: 5.5,
            known_syllable: 6.9,
            rare_typed_penalty: 0.5,
            floor: 5.5,
            margin: 2.0,
            ambiguity: 1.0,
            far_floor: 5.5,
            far_ambiguity: 1.0,
            bigram_weight: 0.5,
            trigram_weight: 0.7,
            kn: true,
            language_penalty: 3.0,
            phrase_decay: 0.7,
            restore_margin: 6.5,
            restore_ambiguity: 1.5,
            restore_english: 8.0,
        }
    }
}

impl Tuning {
    /// The presets of the settings window: 0 careful (fewer corrections,
    /// almost never a wrong one), 1 balanced (the default), 2 bold.
    pub fn preset(level: u8) -> Self {
        let balanced = Self::default();
        match level {
            0 => Self {
                known_word: 4.5,
                known_syllable: 6.0,
                margin: 3.0,
                floor: 6.5,
                far_floor: 7.5,
                restore_margin: 8.0,
                ..balanced
            },
            2 => Self {
                known_word: 6.9,
                known_syllable: 7.5,
                margin: 1.0,
                floor: 4.5,
                far_floor: 5.0,
                restore_margin: 5.0,
                ..balanced
            },
            _ => balanced,
        }
    }
}

pub struct SmartCorrector {
    vi: Lexicon,
    en: Lexicon,
    vi_bigrams: Bigrams,
    en_bigrams: Bigrams,
    vi_trigrams: Trigrams,
    en_trigrams: Trigrams,
    vi_kn: Kn,
    en_kn: Kn,
    /// Telex keys of every Vietnamese syllable, by lexicon id (for the
    /// two-slip search).
    vi_keys: Vec<String>,
    /// Read keys as Telex and propose Vietnamese corrections.
    vietnamese: bool,
    /// Propose English corrections. English words still count as known
    /// when off, so "git" is never "fixed" into Vietnamese.
    english: bool,
    /// Use the previous word to rank candidates.
    context: bool,
    /// Give words typed without marks their marks.
    restore: bool,
    /// Vietnamese words by their letters without marks ("khong" -> không,
    /// khống, khổng...), as lexicon ids.
    bare_index: HashMap<String, Vec<u32>>,
    tuning: Tuning,
    /// Known misspellings and their fixes (English), checked first.
    misspellings: HashMap<String, String>,
    /// The user's own words to ignore and replacements, checked before all.
    personal: Personal,
}

/// Scores behind a decision, for diagnostics.
#[derive(Debug)]
pub struct Ranking {
    /// Score of the word as typed (`-inf` if it is not a known word).
    pub typed: f64,
    /// Candidates, best first.
    pub candidates: Vec<(String, f64)>,
}

/// What the words before say about a candidate: ln P given the previous
/// word, and given the two previous ones (where those were frequent enough).
#[derive(Clone, Copy, Default)]
struct Ngram {
    bi: Option<f64>,
    tri: Option<f64>,
    /// ln P from the Kneser-Ney tables, when in use.
    kn: Option<f64>,
}

/// The previous word, as ids in each lexicon.
#[derive(Clone, Copy, Default)]
struct Context {
    vi: Option<u32>,
    en: Option<u32>,
    /// The word before the previous one.
    vi2: Option<u32>,
    en2: Option<u32>,
    /// Which language the phrase leans to: -1 English only ... 1 Vietnamese
    /// only, 0 undecided.
    vote: f64,
}

impl SmartCorrector {
    pub fn new(vi: Lexicon, en: Lexicon) -> Self {
        let vi_keys = vi.words().iter().map(|(w, _)| to_keys(w)).collect();
        let mut bare_index: HashMap<String, Vec<u32>> = HashMap::new();
        for (id, (word, _)) in vi.words().iter().enumerate() {
            bare_index.entry(skeleton(word).0).or_default().push(id as u32);
        }
        Self {
            vi,
            en,
            vi_bigrams: Bigrams::EMPTY,
            en_bigrams: Bigrams::EMPTY,
            vi_trigrams: Trigrams::EMPTY,
            en_trigrams: Trigrams::EMPTY,
            vi_kn: Kn::EMPTY,
            en_kn: Kn::EMPTY,
            vi_keys,
            vietnamese: true,
            english: true,
            context: true,
            restore: true,
            bare_index,
            tuning: Tuning::default(),
            misspellings: HashMap::new(),
            personal: Personal::default(),
        }
    }

    pub fn set_personal(&mut self, personal: Personal) {
        self.personal = personal;
    }

    /// Never correct `word` again (learned from the user undoing it).
    pub fn add_ignore(&mut self, word: &str) {
        self.personal.add_ignore(word);
    }

    /// Adds a `typo<TAB>fix` list of common misspellings. These are fixed
    /// whatever their frequency in the corpora (which contain many of them),
    /// except where the typo is also well-formed Telex.
    pub fn with_misspellings(mut self, tsv: &str) -> Self {
        for line in tsv.lines().filter(|l| !l.starts_with("#")) {
            let Some((typo, fix)) = line.split_once("\t") else { continue };
            let (typo, fix) = (typo.trim().to_lowercase(), fix.trim().to_string());
            if typo.len() >= MIN_KEYS && compose(&typo).kind != Kind::Vietnamese {
                self.misspellings.insert(typo, fix);
            }
        }
        self
    }

    pub fn set_tuning(&mut self, tuning: Tuning) {
        self.tuning = tuning;
    }

    /// Adds word-pair statistics (see [`Bigrams::from_bytes`]).
    pub fn with_kn(mut self, vi: Kn, en: Kn) -> Self {
        self.vi_kn = vi;
        self.en_kn = en;
        self
    }

    pub fn with_trigrams(mut self, vi: Trigrams, en: Trigrams) -> Self {
        self.vi_trigrams = vi;
        self.en_trigrams = en;
        self
    }

    pub fn with_bigrams(mut self, vi: Bigrams, en: Bigrams) -> Self {
        self.vi_bigrams = vi;
        self.en_bigrams = en;
        self
    }

    /// Turns the previous-word context on or off (for A/B measurements).
    pub fn set_context(&mut self, on: bool) {
        self.context = on;
    }

    fn context_of(&self, history: &[&str]) -> Context {
        let Some(prev) = history.last().filter(|_| self.context) else {
            return Context::default();
        };
        let prev = prev.to_lowercase();
        let mut ctx = Context { vi: self.vi.id(&prev), en: self.en.id(&prev), ..Context::default() };
        if let Some(before) = history.len().checked_sub(2).map(|i| history[i].to_lowercase()) {
            ctx.vi2 = self.vi.id(&before);
            ctx.en2 = self.en.id(&before);
        }
        // The phrase's language: the previous word counts fully, each word
        // before it a fraction less. Words in both languages say nothing.
        let (mut weight, mut vote) = (1.0, 0.0);
        for word in history.iter().rev().take(4) {
            let word = word.to_lowercase();
            match (self.vi.id(&word).is_some(), self.en.id(&word).is_some()) {
                (true, false) => vote += weight,
                (false, true) => vote -= weight,
                _ => {}
            }
            weight *= self.tuning.phrase_decay;
        }
        ctx.vote = vote.clamp(-1.0, 1.0);
        ctx
    }

    /// ln frequency, raised when the previous word makes it likely.
    fn with_context(&self, f: f64, ngram: Ngram) -> f64 {
        if let Some(ln_p) = ngram.kn {
            return ln_p + LN_BILLION;
        }
        if ngram.bi.is_none() && ngram.tri.is_none() {
            return f;
        }
        let uni = (f - LN_BILLION).exp();
        let w = self.tuning.bigram_weight;
        let pair = ngram.bi.map_or(uni, |ln_p| w * ln_p.exp() + (1.0 - w) * uni);
        let t = self.tuning.trigram_weight;
        let p = ngram.tri.map_or(pair, |ln_p| t * ln_p.exp() + (1.0 - t) * pair);
        p.ln() + LN_BILLION
    }

    fn ngram_vi(&self, ctx: Context, id: u32) -> Ngram {
        let kn = ctx.vi.filter(|_| self.tuning.kn && !self.vi_kn.is_empty()).map(|b| self.vi_kn.ln_prob(ctx.vi2, b, id));
        Ngram {
            kn,
            bi: ctx.vi.and_then(|p| self.vi_bigrams.ln_prob(p, id)),
            tri: ctx.vi2.zip(ctx.vi).and_then(|(a, b)| self.vi_trigrams.ln_prob(a, b, id)),
        }
    }

    fn ngram_en(&self, ctx: Context, id: u32) -> Ngram {
        let kn = ctx.en.filter(|_| self.tuning.kn && !self.en_kn.is_empty()).map(|b| self.en_kn.ln_prob(ctx.en2, b, id));
        Ngram {
            kn,
            bi: ctx.en.and_then(|p| self.en_bigrams.ln_prob(p, id)),
            tri: ctx.en2.zip(ctx.en).and_then(|(a, b)| self.en_trigrams.ln_prob(a, b, id)),
        }
    }

    /// Score change for a candidate that is only a word of the language the
    /// previous word is not in.
    fn language_bias(&self, ctx: Context, in_en: bool, in_vi: bool) -> f64 {
        let against = (ctx.vote < 0.0 && in_vi && !in_en) || (ctx.vote > 0.0 && in_en && !in_vi);
        if against {
            -self.tuning.language_penalty * ctx.vote.abs()
        } else {
            0.0
        }
    }

    /// Best known reading per text of a key sequence: (text, score).
    fn readings(&self, keys: &str, english: bool, ctx: Context) -> Vec<(String, f64)> {
        let mut out = Vec::with_capacity(2);
        if let Some(f) = self.en.log_freq(keys).filter(|_| english) {
            let ngram = self.en.id(keys).map_or(Ngram::default(), |id| self.ngram_en(ctx, id));
            let bias = self.language_bias(ctx, true, self.vi.id(keys).is_some());
            out.push((keys.to_string(), self.with_context(f, ngram) + bias));
        }
        if let Some(text) = self.vietnamese_text(keys) {
            if let (Some(f), Some(id)) = (self.vi.log_freq(&text), self.vi.id(&text)) {
                let f = self.with_context(f, self.ngram_vi(ctx, id)) + self.language_bias(ctx, self.en.id(&text).is_some(), true);
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

    /// A Vietnamese word typed without any marks ("khong" for "không"). The
    /// bare syllable is a real word in the corpora (people do write without
    /// accents), so its own frequency cannot tell it from a typo. Instead
    /// the accented forms with the same letters compete with it, the
    /// previous word as evidence, and one must win by a wide margin.
    fn restore_marks(&self, word: &str, keys: &str, history: &[&str]) -> Option<String> {
        // Typed without marks: the Telex keys changed nothing. (Not required
        // to be a valid syllable: "duoc" is not one, yet it is "được".)
        if compose(keys).text != keys {
            return None;
        }
        let text = keys.to_string();
        let t = &self.tuning;
        // English words that are also bare syllables ("the", "do", "can").
        if self.en.log_freq(keys).is_some_and(|f| f >= t.restore_english) {
            return None;
        }
        // A capitalised word may well be a name written without accents
        // ("Tuan", "Hung"); missing "Khong" at the start of a sentence is the
        // smaller loss.
        if word.chars().next().is_some_and(char::is_uppercase) {
            return None;
        }
        let ctx = self.context_of(history);
        let score = |id: u32, f: f64| {
            self.with_context(f, self.ngram_vi(ctx, id))
        };
        let typed = match (self.vi.id(&text), self.vi.log_freq(&text)) {
            (Some(id), Some(f)) => score(id, f),
            _ => UNSEEN_SYLLABLE,
        };
        let mut candidates: Vec<(&str, f64)> = self
            .bare_index
            .get(&text)?
            .iter()
            .filter_map(|&id| {
                let (w, f) = &self.vi.words()[id as usize];
                (*w != text).then(|| (w.as_str(), score(id, *f)))
            })
            .collect();
        candidates.sort_by(|a, b| b.1.total_cmp(&a.1));
        let (best, best_score) = *candidates.first()?;
        let runner_up = candidates.get(1).map_or(f64::NEG_INFINITY, |c| c.1);
        (best_score - typed >= t.restore_margin && best_score - runner_up >= t.restore_ambiguity)
            .then(|| match_case(word, best))
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
            .readings(keys, true, Context::default())
            .into_iter()
            .map(|(_, f)| f)
            .chain(vietnamese.as_ref().map(|_| UNSEEN_SYLLABLE))
            .fold(f64::NEG_INFINITY, f64::max);
        (score, vietnamese)
    }

    /// Corrections of `keys` one slip away, best first.
    fn candidates(&self, keys: &str, vietnamese: Option<String>, ctx: Context) -> Vec<(String, f64)> {
        // A well-formed syllable was typed: Vietnamese syllables are so dense
        // that changing a letter almost always lands on another one ("khoi" ->
        // "khi", "iết" -> "siết"), a real-word correction that needs context.
        // Only the kind of mark may change (hỏi <-> ngã, ô <-> ơ): same letters,
        // and a tone stays a tone ("ạn" was typed with "j" on purpose).
        let required = vietnamese.as_deref().map(skeleton);
        let typed_texts: Vec<String> = std::iter::once(keys.to_string()).chain(vietnamese).collect();

        // Several slips can lead to the same word: their probabilities add up.
        let mut scores: HashMap<String, f64> = HashMap::new();
        for slip in edits1(keys).into_iter().filter(|s| s.marks_only || required.is_none()) {
            for (text, f) in self.readings(&slip.keys, self.english, ctx) {
                if typed_texts.contains(&text) || required.as_ref().is_some_and(|r| !same_word(r, &skeleton(&text))) {
                    continue;
                }
                // Without English corrections (terminals, IDEs) an unaccented
                // Vietnamese fix is just another English-looking word: "teh"
                // must not become "the" (a Vietnamese word too) in code.
                if !self.english && text.is_ascii() {
                    continue;
                }
                let e = scores.entry(text).or_insert(f64::NEG_INFINITY);
                *e = log_add(*e, f - slip.cost);
            }
        }
        sorted(scores)
    }

    /// Corrections two slips away ("khogn" -> "không"), found by comparing
    /// the keys with every word in the lexicons. Only for unknown words that
    /// nothing one slip away explains.
    fn far_candidates(&self, keys: &str, ctx: Context) -> Vec<(String, f64)> {
        let typed = keys.as_bytes();
        let typed_bag = letter_counts(typed);
        let mut scores: HashMap<String, f64> = HashMap::new();
        // The cheap tests run for every word of both lexicons; the context
        // lookup and the alignment only for the few that survive them.
        let mut consider = |target: &str, text: &str, f: f64, bigram: &dyn Fn() -> Ngram, bias: &dyn Fn() -> f64| {
            if target.len().abs_diff(typed.len()) > 2 || bag_distance(&typed_bag, target.as_bytes()) > 4 {
                return;
            }
            if let Some(cost) = slip_cost(typed, target.as_bytes(), FAR_MAX_COST) {
                let e = scores.entry(text.to_string()).or_insert(f64::NEG_INFINITY);
                *e = log_add(*e, self.with_context(f, bigram()) + bias() - cost);
            }
        };
        if self.english {
            for (id, (word, f)) in self.en.words().iter().enumerate() {
                let bigram = || self.ngram_en(ctx, id as u32);
                consider(word, word, *f, &bigram, &|| self.language_bias(ctx, true, self.vi.id(word).is_some()));
            }
        }
        if self.vietnamese {
            for (id, (word, f)) in self.vi.words().iter().enumerate() {
                let bigram = || self.ngram_vi(ctx, id as u32);
                consider(&self.vi_keys[id], word, *f, &bigram, &|| self.language_bias(ctx, self.en.id(word).is_some(), true));
            }
        }
        if !self.english {
            scores.retain(|text, _| !text.is_ascii());
        }
        sorted(scores)
    }

    /// Scores every correction of `word` (raw keys), for diagnostics.
    pub fn rank(&self, word: &str, prev: Option<&str>) -> Option<Ranking> {
        self.rank_in(word, prev.as_slice())
    }

    /// For the hard-case journal: a word left alone although it had a close
    /// alternative. Returns a short note ("best score | next score | typed
    /// score"), or `None` when the word is plainly fine or has no plausible
    /// alternative.
    pub fn near_miss(&self, word: &str, history: &[&str]) -> Option<String> {
        let keys = Self::keys_of(word)?;
        if self.personal.ignores(&word.to_lowercase()) {
            return None;
        }
        let capital = word.chars().next().is_some_and(char::is_uppercase);
        let bare = self.vietnamese && self.restore && !capital && compose(&keys).text == keys;
        let ranking = if bare {
            // Common English words that are also bare syllables are left alone on purpose.
            if self.en.log_freq(&keys).is_some_and(|f| f >= self.tuning.restore_english) {
                return None;
            }
            self.rank_bare(&keys, history)?
        } else {
            let (typed, vietnamese) = self.typed(&keys);
            let known = if vietnamese.is_some() { self.tuning.known_syllable } else { self.tuning.known_word };
            if typed >= known {
                return None;
            }
            self.rank_in(word, history)?
        };
        let top = ranking.candidates.first()?;
        let second = ranking.candidates.get(1);
        let gap = second.map_or(f64::INFINITY, |c| top.1 - c.1);
        // Plainly fine as typed, or a clear winner over everything: not a hard case.
        if ranking.typed - top.1 > 3.0 || (gap >= 2.0 && top.1 - ranking.typed >= 3.0) {
            return None;
        }
        let show = |s: f64| if s.is_finite() { format!("{s:.1}") } else { "-".to_string() };
        Some(format!(
            "{} {} | {} {} | typed {}",
            top.0,
            show(top.1),
            second.map_or("-", |c| c.0.as_str()),
            show(second.map_or(f64::NEG_INFINITY, |c| c.1)),
            show(ranking.typed)
        ))
    }

    /// Diagnostics for a word typed without marks: the score of the bare word
    /// and of its accented readings, best first (what `restore_marks` weighs).
    pub fn rank_bare(&self, keys: &str, history: &[&str]) -> Option<Ranking> {
        let ctx = self.context_of(history);
        let score = |id: u32, f: f64| self.with_context(f, self.ngram_vi(ctx, id));
        let typed = match (self.vi.id(keys), self.vi.log_freq(keys)) {
            (Some(id), Some(f)) => score(id, f),
            _ => UNSEEN_SYLLABLE,
        };
        let mut candidates: Vec<(String, f64)> = self
            .bare_index
            .get(keys)?
            .iter()
            .filter_map(|&id| {
                let (w, f) = &self.vi.words()[id as usize];
                (w.as_str() != keys).then(|| (w.clone(), score(id, *f)))
            })
            .collect();
        sorted_desc(&mut candidates);
        Some(Ranking { typed, candidates })
    }

    /// Diagnostics: what the word after `candidate` says for it, as the
    /// ln likelihood ratio of that pair against the word on its own. This is
    /// context the app never has when it decides (the word is not typed yet).
    pub fn right_context_bonus(&self, candidate: &str, next: &str) -> f64 {
        let (Some(a), Some(b), Some(f)) = (self.vi.id(&candidate.to_lowercase()), self.vi.id(next), self.vi.log_freq(next)) else {
            return 0.0;
        };
        let w = self.tuning.bigram_weight;
        let uni = (f - LN_BILLION).exp();
        match self.vi_bigrams.ln_prob(a, b) {
            Some(lp) => (w * lp.exp() + (1.0 - w) * uni).ln() - uni.ln(),
            None => (1.0 - w).ln(),
        }
    }

    pub fn rank_in(&self, word: &str, history: &[&str]) -> Option<Ranking> {
        let keys = Self::keys_of(word)?;
        let (typed, vietnamese) = self.typed(&keys);
        let ctx = self.context_of(history);
        let mut candidates = self.candidates(&keys, vietnamese, ctx);
        let t = &self.tuning;
        if confident(&candidates, typed, t.margin, t.floor, t.ambiguity).is_none() && typed == f64::NEG_INFINITY && keys.len() >= FAR_MIN_KEYS {
            candidates = self.far_candidates(&keys, ctx);
        }
        Some(Ranking { typed, candidates })
    }
}

fn sorted_desc(candidates: &mut [(String, f64)]) {
    candidates.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
}

/// Candidates by descending score (ties alphabetically, for determinism).
fn sorted(scores: HashMap<String, f64>) -> Vec<(String, f64)> {
    let mut candidates: Vec<(String, f64)> = scores.into_iter().collect();
    candidates.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    candidates
}

/// The best candidate, if it clearly beats what was typed and the runner-up.
fn confident(candidates: &[(String, f64)], typed: f64, margin: f64, floor: f64, ambiguity: f64) -> Option<&str> {
    let (best, score) = candidates.first()?;
    let runner_up = candidates.get(1).map_or(f64::NEG_INFINITY, |c| c.1);
    (*score >= floor && score - typed >= margin && score - runner_up >= ambiguity).then_some(best.as_str())
}

impl Corrector for SmartCorrector {
    fn set_languages(&mut self, vietnamese: bool, english: bool) {
        self.vietnamese = vietnamese;
        self.english = english;
    }

    fn set_restore_marks(&mut self, on: bool) {
        self.restore = on;
    }

    fn correct(&self, word: &str) -> Option<String> {
        self.correct_after(word, None)
    }

    fn correct_after(&self, word: &str, prev: Option<&str>) -> Option<String> {
        self.correct_in(word, prev.as_slice())
    }

    fn correct_in(&self, word: &str, history: &[&str]) -> Option<String> {
        // The user's own dictionary comes first, for words of any length.
        let lower = word.to_lowercase();
        if let Some(instead) = self.personal.fix(&lower) {
            return Some(match_case(word, instead));
        }
        if self.personal.ignores(&lower) {
            return None;
        }
        let keys = Self::keys_of(word)?;
        if self.english {
            if let Some(fix) = self.misspellings.get(&keys) {
                return Some(match_case(word, fix));
            }
        }
        if self.vietnamese && self.restore {
            if let Some(fix) = self.restore_marks(word, &keys, history) {
                return Some(fix);
            }
        }
        let (typed, vietnamese) = self.typed(&keys);
        // Fast path for nearly every word: it is a common, correct one.
        let known = if vietnamese.is_some() { self.tuning.known_syllable } else { self.tuning.known_word };
        if typed >= known {
            return None;
        }
        // A capitalised known word is most likely a name ("Tuan"); so is an
        // unknown one in the middle of a sentence ("Wolff", after a word).
        // Only a capitalised unknown word that starts a phrase can be a typo.
        let capitalised = word.chars().next().is_some_and(char::is_uppercase);
        if capitalised && (typed > f64::NEG_INFINITY || !history.is_empty()) {
            return None;
        }
        let ctx = self.context_of(history);
        // Rare entries are often misspellings that leaked into the corpora.
        let typed_penalised = typed - self.tuning.rare_typed_penalty;

        let near = self.candidates(&keys, vietnamese, ctx);
        let t = &self.tuning;
        if let Some(best) = confident(&near, typed_penalised, t.margin, t.floor, t.ambiguity) {
            return Some(match_case(word, best));
        }
        // Two slips: only for unknown words, never names.
        if typed == f64::NEG_INFINITY && !capitalised && keys.len() >= FAR_MIN_KEYS {
            let far = self.far_candidates(&keys, ctx);
            return confident(&far, typed, t.margin, t.far_floor, t.far_ambiguity).map(|best| match_case(word, best));
        }
        None
    }
}

/// Whether `b` is `a` with at most a plausibly mistaken mark: same base
/// letters, and the same tone, hỏi/ngã swapped, or the tone of a
/// neighbouring key (s/x, r/f). Adding or dropping a tone is never a slip.
fn same_word(a: &(String, Option<Tone>), b: &(String, Option<Tone>)) -> bool {
    use Tone::{Hoi, Huyen, Nga, Sac};
    let tones_ok = match (a.1, b.1) {
        (x, y) if x == y => true,
        (Some(x), Some(y)) => matches!(
            (x, y),
            (Hoi, Nga) | (Nga, Hoi) | (Sac, Nga) | (Nga, Sac) | (Hoi, Huyen) | (Huyen, Hoi)
        ),
        _ => false,
    };
    a.0 == b.0 && tones_ok
}

/// Base letters of a word and its tone: "mỗi" -> ("moi", Some(Nga)).
fn skeleton(text: &str) -> (String, Option<Tone>) {
    let mut toned = None;
    let letters = text
        .chars()
        .flat_map(char::to_lowercase)
        .map(|c| {
            let (base, tone) = split_tone(c);
            toned = toned.or(tone);
            match base {
                'ă' | 'â' => 'a',
                'ê' => 'e',
                'ô' | 'ơ' => 'o',
                'ư' => 'u',
                'đ' => 'd',
                other => other,
            }
        })
        .collect();
    (letters, toned)
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

    /// Built like the program does: lexicons, word pairs, misspelling list.
    fn corrector() -> SmartCorrector {
        let vi = Lexicon::parse(include_str!("../../../data/vi_syllables.tsv"));
        let en = Lexicon::parse(include_str!("../../../data/en_words.tsv"));
        let vi_pairs = Bigrams::from_bytes(include_bytes!("../../../data/vi_bigrams.bin"), vi.len());
        let en_pairs = Bigrams::from_bytes(include_bytes!("../../../data/en_bigrams.bin"), en.len());
        let vi_triples = Trigrams::from_bytes(include_bytes!("../../../data/vi_trigrams.bin"), vi.len());
        let en_triples = Trigrams::from_bytes(include_bytes!("../../../data/en_trigrams.bin"), en.len());
        let vi_kn = Kn::from_bytes(include_bytes!("../../../data/vi_kn.bin"), vi.len());
        let en_kn = Kn::from_bytes(include_bytes!("../../../data/en_kn.bin"), en.len());
        assert!(!vi_pairs.is_empty() && !en_pairs.is_empty(), "word pairs do not match the lexicons: rebuild with ac-data");
        SmartCorrector::new(vi, en)
            .with_bigrams(vi_pairs, en_pairs)
            .with_trigrams(vi_triples, en_triples)
            .with_kn(vi_kn, en_kn)
            .with_misspellings(include_str!("../../../data/en_misspellings.tsv"))
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
            ("seperate", "separate"),
            ("thier", "their"),
            ("occured", "occurred"),
            ("definately", "definitely"),
            ("mooir", "mỗi"), // hỏi/ngã mixed up
            ("gruwi", "gửi"),
            ("nhnah", "nhanh"),
            // Two slips: transposed letters and a missing "o".
            ("khogn", "không"),
        ] {
            assert_eq!(c.correct(typed).as_deref(), Some(want), "{typed}: {:?}", c.rank(typed, None));
        }
    }

    #[test]
    fn leaves_good_words() {
        let c = corrector();
        for typed in [
            "hello", "the", "terminal", "dungf", "tieengs", "dduowcj", "khoong", "npm", "git",
            "kubectl", "cargo", "Tuan", "nhanh", "toi",
            // Well-formed syllables: never add/remove a tone or change letters.
            "anj",   // ạn: the tone was typed on purpose, not "an"
            "ieets", // iết: not "siết" (s here would be a letter, not a tone)
            "khoi",  // no tone typed: do not guess "khỏi"
        ] {
            assert_eq!(c.correct(typed), None, "{typed}: {:?}", c.rank(typed, None));
        }
    }

    /// The misspelling list works whatever the corpora say, but only when
    /// English corrections are on, and it never hijacks well-formed Telex.
    #[test]
    fn misspelling_list() {
        let mut c = corrector();
        assert_eq!(c.correct("Seperate").as_deref(), Some("Separate"));
        c.set_languages(true, false);
        assert_eq!(c.correct("seperate"), None);
    }

    /// With the previous word, the language of the sentence decides between
    /// an English slip and a Vietnamese syllable: "launh" alone reads as
    /// Vietnamese, but after an English word it must not become a Vietnamese
    /// word (it used to turn into "anh").
    #[test]
    fn context_prefers_the_sentence_language() {
        let c = corrector();
        assert!(c.correct_after("launh", None).is_some_and(|fix| !fix.is_ascii() || fix == "anh"));
        // An English slip after an English word is an English word: launch, never anh.
        assert_eq!(c.correct_after("launh", Some("pioneering")).as_deref(), Some("launch"));
        assert_ne!(c.correct_after("ays", Some("three")).as_deref(), Some("ấy"));
    }

    /// The hard-case journal: a word left alone with two close readings is
    /// reported; a plainly fine word is not.
    #[test]
    fn near_miss_reports_close_calls_only() {
        let c = corrector();
        let note = c.near_miss("chuww", &["thích", "tớ"]).expect("close call");
        assert!(note.contains("chứ") || note.contains("chưa"), "{note}");
        assert_eq!(c.near_miss("hello", &[]), None);
        assert_eq!(c.near_miss("the", &["is"]), None);
    }

    /// A letter missing from a Vietnamese word ("thics" for "thích"): invalid as
    /// typed ("thíc" is not a syllable), so the fix may add the letter. With
    /// Vietnamese words before it, the Vietnamese reading wins over "this".
    #[test]
    fn restores_a_missing_letter_in_vietnamese_context() {
        let c = corrector();
        assert_eq!(c.correct_in("thics", &["tôi"]).as_deref(), Some("thích"));
        assert_eq!(c.correct_in("thics", &["tôi", "rất"]).as_deref(), Some("thích"));
    }

    /// An English word in the middle of Vietnamese text does not turn the
    /// phrase English: the words before it still count, less each step back.
    #[test]
    fn phrase_language_weighs_earlier_words() {
        let c = corrector();
        let alone = c.context_of(&["pioneering"]).vote;
        let in_phrase = c.context_of(&["nhanh", "pioneering"]).vote;
        assert_eq!(alone, -1.0);
        assert!(in_phrase > alone && in_phrase < 0.0, "{in_phrase}");
        // Without earlier words it is the previous word alone, as before.
        assert_eq!(c.correct_in("launh", &["pioneering"]), c.correct_after("launh", Some("pioneering")));
    }

    /// "đi ban" and friends: what the words before make of a bare syllable.
    /// `cargo test -p ac-core ambiguous -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn ambiguous_bare_words() {
        let c = corrector();
        for history in [
            vec!["đi"],
            vec!["mình", "đi"],
            vec!["cho", "mình", "đi"],
            vec!["cái", "này", "cho", "mình", "đi"],
            vec!["đi", "mua"],
            vec!["tôi", "mua"],
        ] {
            let ctx = c.context_of(&history);
            let mut scored: Vec<(String, f64)> = c.bare_index["ban"]
                .iter()
                .map(|&id| {
                    let (w, f) = &c.vi.words()[id as usize];
                    (w.clone(), c.with_context(*f, c.ngram_vi(ctx, id)))
                })
                .collect();
            scored.sort_by(|a, b| b.1.total_cmp(&a.1));
            let top: Vec<_> = scored.iter().take(4).map(|(w, s)| format!("{w} {s:.1}")).collect();
            println!("{:<28} -> {:<8} {top:?}", history.join(" "), format!("{:?}", c.correct_in("ban", &history)));
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
            "becuase", "definately", "occured", "thier", "jsut", "taht", "wiht", "khogn", "duowcj", "thics", "thichs", "thihc", "thix",
        ] {
            let r = c.rank(w, None);
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
        for w in ["teh", "dunhf", "definately", "nguyeenx", "kubectl", "khogn", "qwertyuiop"] {
            let start = std::time::Instant::now();
            for _ in 0..100 {
                std::hint::black_box(c.correct(w));
            }
            println!("{w:>10}: {:>7.1} us/word", start.elapsed().as_secs_f64() * 1e6 / 100.0);
        }
    }

    /// "khong" is a real (if rare) word in the corpora, yet after "tôi" it
    /// can only be "không".
    #[test]
    fn restores_marks_of_a_bare_word() {
        let c = corrector();
        assert_eq!(c.correct_after("khong", Some("tôi")).as_deref(), Some("không"));
        assert_eq!(c.correct_after("duoc", Some("không")).as_deref(), Some("được"));
    }

    #[test]
    fn restoring_marks_is_careful() {
        let mut c = corrector();
        // Already has its marks, or is an English word, or is a name.
        assert_eq!(c.correct_after("không", Some("tôi")), None);
        assert_eq!(c.correct("the"), None);
        assert_eq!(c.correct_after("Nguyen", Some("anh")), None);
        assert_eq!(c.correct("Khong"), None); // could be a name: leave capitals alone
        // Too many accented forms, none clearly the one.
        assert_eq!(c.correct("ban"), None);
        // Switched off, or in English mode.
        c.set_restore_marks(false);
        assert_eq!(c.correct_after("khong", Some("tôi")), None);
        c.set_restore_marks(true);
        c.set_languages(false, true);
        assert_eq!(c.correct_after("khong", Some("tôi")), None);
    }

    #[test]
    fn personal_dictionary_comes_first() {
        let mut c = corrector();
        assert_eq!(c.correct("teh").as_deref(), Some("the"));
        c.set_personal(Personal::parse("ignore\tteh\nfix\tko\tkhông\nfix\tcty\tcông ty\n"));
        assert_eq!(c.correct("teh"), None); // ignored
        assert_eq!(c.correct("Teh"), None);
        assert_eq!(c.correct("ko").as_deref(), Some("không")); // short words too
        assert_eq!(c.correct("Cty").as_deref(), Some("Công ty"));
        // A learned word is ignored from then on.
        c.add_ignore("recieve");
        assert_eq!(c.correct("recieve"), None);
    }

    #[test]
    fn english_only_mode_skips_vietnamese() {
        let mut c = corrector();
        c.set_languages(false, true);
        assert_eq!(c.correct("dunhf"), None);
        assert_eq!(c.correct("teh").as_deref(), Some("the"));
    }

    /// Terminals and IDEs: commands and code look like English, so only
    /// Vietnamese is corrected there, and English words still count as known.
    #[test]
    fn vietnamese_only_mode_for_code() {
        let mut c = corrector();
        c.set_languages(true, false);
        assert_eq!(c.correct("teh"), None); // "the" is Vietnamese too, but unaccented
        assert_eq!(c.correct("recieve"), None);
        assert_eq!(c.correct("waht"), None);
        assert_eq!(c.correct("dunhf").as_deref(), Some("dùng"));
        assert_eq!(c.correct("git"), None);
        assert_eq!(c.correct("cargo"), None);
    }

    #[test]
    fn log_add_is_stable() {
        assert!((log_add(0.0, 0.0) - 2f64.ln()).abs() < 1e-12);
        assert_eq!(log_add(f64::NEG_INFINITY, 5.0), 5.0);
    }
}
