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
use crate::student::Student;

mod split;

/// Score of a well-formed Vietnamese syllable missing from the lexicon
/// (about 20 per billion).
const UNSEEN_SYLLABLE: f64 = 3.0;
/// Shorter tokens are too ambiguous to fix.
const MIN_KEYS: usize = 3;
/// A word of capitals becomes another word only if that one scores at least this (ln per billion; 9 is ~8000).
const ACRONYM_FLOOR: f64 = 9.0;
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
    /// Delayed revision (see [`SmartCorrector::revise`]): how much better than the word
    /// as typed the best reading must score once the next word is known. Readings
    /// that change letters of a well-formed syllable need the larger margin.
    pub revise_margin: f64,
    pub revise_margin_x: f64,
    /// ...and how far it must beat the second best.
    pub revise_ambiguity: f64,
    /// Hindsight check of a correction already made (see [`SmartCorrector::audit`]): the
    /// word as typed must fit the word after it better than the fix, by this much, for
    /// the fix to be taken back.
    pub audit_margin: f64,
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
    /// Keys that are no word are cut into syllables ("quanheej" -> "quan hệ") only if
    /// the best cut beats the second best by this much...
    pub split_margin: f64,
    /// ...every syllable after the first is at least this much likelier after the one
    /// before it than on its own...
    pub split_lift: f64,
    /// ...and every syllable is at least this common (ln per billion).
    pub split_floor: f64,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            known_word: 5.5,
            known_syllable: 6.9,
            rare_typed_penalty: 0.5,
            floor: 5.5,
            margin: 3.5,
            ambiguity: 1.0,
            far_floor: 5.5,
            far_ambiguity: 1.0,
            bigram_weight: 0.5,
            trigram_weight: 0.7,
            kn: true,
            revise_margin: 8.0,
            revise_margin_x: 12.0,
            revise_ambiguity: 3.0,
            audit_margin: 4.0,
            language_penalty: 3.0,
            phrase_decay: 0.7,
            restore_margin: 6.5,
            restore_ambiguity: 1.5,
            restore_english: 8.0,
            split_margin: 2.0,
            split_lift: 1.0,
            split_floor: 5.0,
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
                margin: 5.5,
                floor: 6.5,
                far_floor: 7.5,
                restore_margin: 8.0,
                ..balanced
            },
            2 => Self {
                known_word: 6.9,
                known_syllable: 7.5,
                margin: 1.5,
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
    /// The learned student that decides the delayed revision when present and on.
    student: Option<Box<Student>>,
    student_on: bool,
    /// Probability of "change" the student must reach to propose a fix.
    student_tau: f32,
    /// The fix is chosen among the readings the word statistics find plausible (else: the student's top class).
    student_restricted: bool,
    /// Guards against the classes of wrong fixes seen in the journal (see `protected` and `english_phrase`).
    guards: bool,
    /// Weight of the word statistics next to the student's probabilities when choosing the fix.
    student_lambda: f64,
    /// The fix chosen must carry at least this share of the student's chance of "change".
    student_min_fix: f32,
    /// A word the student leaves alone is still given to the word statistics (their delayed revision).
    student_fallback: bool,
}

/// Default confidence of the student. On Viwiki-Spelling (RESEARCH.md, section 5) 0.99 corrects about 13% of
/// the mistakes with about 0.14 wrong changes per 1000 correct words; 0.9 corrects about 21% with about 1.2.
pub const STUDENT_TAU: f32 = 0.99;

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
            student: None,
            student_on: true,
            student_tau: STUDENT_TAU,
            // Student and word statistics together: the student says whether to change, the fix is chosen among the
            // readings the statistics find plausible (their score weighs in), and what the student leaves alone
            // goes to the statistics' own delayed revision.
            student_restricted: true,
            guards: true,
            student_lambda: 0.3,
            student_min_fix: 0.0,
            student_fallback: true,
        }
    }

    /// Whether a word the student leaves alone goes on to the word statistics' delayed revision.
    pub fn set_student_fallback(&mut self, on: bool) {
        self.student_fallback = on;
    }

    /// Whether the student's fix must be one of the readings the word statistics find plausible.
    pub fn set_student_restricted(&mut self, on: bool) {
        self.student_restricted = on;
    }

    /// Switches the guards of `protected`, `english_phrase` and the one-letter pieces of a cut on or off
    /// (for A/B measurements).
    pub fn set_guards(&mut self, on: bool) {
        self.guards = on;
    }

    /// Words that are left alone whatever the statistics say: keys that start with a key no Vietnamese syllable
    /// starts with ("json" is not "son": j, f and z are tone or foreign letters).
    fn protected(&self, keys: &str) -> bool {
        self.guards && keys.starts_with(['j', 'f', 'z'])
    }

    /// An acronym ("LLM" is not "LL"): a word of capitals is changed only into a word as common as "the" is
    /// ("TEH", typed with Caps Lock on), never into a rare one.
    fn acronym_weak(&self, word: &str, score: f64) -> bool {
        self.guards
            && word.chars().count() >= 2
            && word.chars().all(|c| !c.is_lowercase())
            && word.chars().any(char::is_uppercase)
            && score < ACRONYM_FLOOR
    }

    /// The word before is English (and no Vietnamese word) and the one-slip readings include an English word: the
    /// writer is more likely in English ("vibe codin" is "coding", not "con").
    fn english_phrase(&self, history: &[&str], near: &[(String, f64)], best: &str) -> bool {
        let english_only = |w: &str| {
            let w = w.to_lowercase();
            self.en.id(&w).is_some() && self.vi.id(&w).is_none()
        };
        self.guards
            && self.english
            && history.last().is_some_and(|w| english_only(w))
            && self.vi.id(&best.to_lowercase()).is_some()
            && near.iter().any(|(w, _)| english_only(w))
    }

    /// How the fix is chosen among the plausible readings: the weight of the word statistics next to the
    /// student's probabilities, and the least share of the student's chance of "change" the fix must carry.
    pub fn set_student_blend(&mut self, lambda: f64, min_fix: f32) {
        self.student_lambda = lambda;
        self.student_min_fix = min_fix;
    }

    /// Adds the learned student: with it, the delayed revision ([`SmartCorrector::revise`]) is decided
    /// by the student instead of the word statistics.
    pub fn with_student(mut self, student: Student) -> Self {
        self.set_student(student);
        self
    }

    pub fn set_student(&mut self, student: Student) {
        self.student = Some(Box::new(student));
    }

    /// Turns the student on or off (off: the word statistics decide the delayed revision again).
    pub fn set_student_enabled(&mut self, on: bool) {
        self.student_on = on;
    }

    /// The probability of "change" the student must reach to propose a fix (0.5 to 1).
    pub fn set_student_threshold(&mut self, tau: f32) {
        self.student_tau = tau.clamp(0.5, 0.999_999);
    }

    pub fn has_student(&self) -> bool {
        self.student.is_some()
    }

    /// Every reading of `word` the word statistics find plausible for what was typed, lower case and
    /// without the word itself: one slip in the marks or letters, a letter run typed too long, two slips
    /// for a word that is not a word, and the accented forms of a word typed bare.
    pub fn revision_candidates(&self, word: &str, history: &[&str]) -> Vec<(String, f64)> {
        let Some(keys) = Self::keys_of(word) else { return Vec::new() };
        let (typed, vietnamese) = self.typed(&keys);
        let shown = vietnamese.clone().unwrap_or_else(|| keys.clone()).to_lowercase();
        let ctx = self.context_of(history);
        let mut all: Vec<(String, f64)> = self.candidates(&keys, vietnamese.clone(), ctx, true);
        if vietnamese.is_some() {
            all.extend(self.candidates(&keys, vietnamese, ctx, false));
        }
        if typed == f64::NEG_INFINITY && keys.len() >= FAR_MIN_KEYS {
            all.extend(self.far_candidates(&keys, ctx));
        }
        if self.vietnamese && self.restore && compose(&keys).text == keys {
            if let Some(r) = self.rank_bare(&keys, history) {
                all.extend(r.candidates);
            }
        }
        // Each reading once, with its best score (ln frequency given the words before).
        let mut best: HashMap<String, f64> = HashMap::new();
        for (w, s) in all {
            let w = w.to_lowercase();
            if w != shown {
                let e = best.entry(w).or_insert(f64::NEG_INFINITY);
                *e = e.max(s);
            }
        }
        sorted(best)
    }

    /// The student's decision for a word and the one typed after it: the fix, or `None`.
    fn revise_with_student(&self, student: &Student, word: &str, history: &[&str], right: &[String]) -> Option<String> {
        let keys = Self::keys_of(word)?;
        let (_, vietnamese) = self.typed(&keys);
        let shown = vietnamese.unwrap_or_else(|| keys.clone());
        let lower = shown.to_lowercase();
        // An English word in the phrase is left alone: the student was not taught mixed text.
        if self.en.id(&lower).is_some() && self.vi.id(&lower).is_none() {
            return None;
        }
        let window: Vec<&str> = history.iter().copied().chain(std::iter::once(shown.as_str())).chain(right.iter().map(String::as_str)).collect();
        let probs = student.probs(&window, history.len())?;
        if 1.0 - probs[0] < self.student_tau {
            return None;
        }
        let fix = if self.student_restricted {
            // The student says the word is wrong; what it becomes is chosen among the readings the word
            // statistics find plausible for the keys typed, by the student's own probabilities.
            // With a weight above zero the word statistics weigh in too: the score of the reading given
            // the words before, and what the words after it say for it.
            let finite = |s: f64| if s.is_finite() { s } else { -99.0 };
            let (fix, p, _) = self
                .revision_candidates(word, history)
                .into_iter()
                .filter_map(|(c, s)| {
                    let p = student.prob_of(&probs, &c);
                    if p <= 0.0 {
                        return None;
                    }
                    let vi = self.vietnamese && self.vi.id(&c).is_some();
                    let ngram = finite(s) + self.right_context_score(vi, history, &c, right);
                    let score = f64::from(p.max(1e-6)).ln() + self.student_lambda * ngram;
                    Some((c, p, score))
                })
                .max_by(|a, b| a.2.total_cmp(&b.2))?;
            (p >= self.student_min_fix * (1.0 - probs[0])).then_some(fix)?
        } else {
            student.judge(&window, history.len())?.fixes.into_iter().next()?.0
        };
        (fix.to_lowercase() != lower).then(|| match_case(&shown, &fix))
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
    /// A word typed with Telex's escape (a tone key pressed twice shows the key: "tesst" shows "test"):
    /// when what appears is a known English word, the writer meant it.
    fn escaped_english(&self, keys: &str) -> Option<f64> {
        let shown = compose(keys);
        if shown.kind != Kind::Literal || shown.text == keys {
            return None;
        }
        self.en.log_freq(&shown.text).filter(|f| *f >= self.tuning.known_word)
    }

    fn typed(&self, keys: &str) -> (f64, Option<String>) {
        let vietnamese = self.vietnamese_text(keys);
        let score = self
            .readings(keys, true, Context::default())
            .into_iter()
            .map(|(_, f)| f)
            .chain(self.escaped_english(keys))
            .chain(vietnamese.as_ref().map(|_| UNSEEN_SYLLABLE))
            .fold(f64::NEG_INFINITY, f64::max);
        (score, vietnamese)
    }

    /// Corrections of `keys` one slip away, best first.
    fn candidates(&self, keys: &str, vietnamese: Option<String>, ctx: Context, restrict: bool) -> Vec<(String, f64)> {
        // A well-formed syllable was typed: Vietnamese syllables are so dense
        // that changing a letter almost always lands on another one ("khoi" ->
        // "khi", "iết" -> "siết"), a real-word correction that needs context.
        // Only the kind of mark may change (hỏi <-> ngã, ô <-> ơ): same letters,
        // and a tone stays a tone ("ạn" was typed with "j" on purpose).
        let required = vietnamese.as_deref().map(skeleton).filter(|_| restrict);
        let typed_texts: Vec<String> = std::iter::once(keys.to_string()).chain(vietnamese).collect();

        // Several slips can lead to the same word: their probabilities add up.
        let mut scores: HashMap<String, f64> = HashMap::new();
        for slip in edits1(keys).into_iter().filter(|s| s.marks_only || required.is_none()) {
            for (text, f) in self.readings(&slip.keys, self.english, ctx) {
                if typed_texts.contains(&text) || required.as_ref().is_some_and(|r| !same_word(r, &skeleton(&text))) {
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
        sorted(scores)
    }

    /// In terminals and IDEs (no English corrections) an unaccented fix is just another
    /// English-looking word: "teh" must not become "the" there. But such a reading must
    /// still compete, and when it is the best one the word is left alone: dropping it
    /// would let a worse accented reading win ("namk" -> "năm" instead of "nam").
    fn unaccented_wins(&self, candidates: &[(String, f64)]) -> bool {
        !self.english && candidates.first().is_some_and(|(text, _)| text.is_ascii())
    }

    /// Scores every correction of `word` (raw keys), for diagnostics.
    pub fn rank(&self, word: &str, prev: Option<&str>) -> Option<Ranking> {
        self.rank_in(word, prev.as_slice())
    }

    /// Delayed revision: a word the immediate correction left alone, looked at again
    /// once the word after it is typed. `right` holds the words typed since (one in
    /// practice). Candidates include letter changes to well-formed syllables ("ngẫy"
    /// -> "ngẫu"), which the immediate correction never tries; they need a larger
    /// margin. Not used by the app yet: measured by `ac-bench --delayed`.
    pub fn revise(&self, word: &str, history: &[&str], right: &[String]) -> Option<String> {
        if right.is_empty() || !self.vietnamese || self.personal.ignores(&word.to_lowercase()) {
            return None;
        }
        if let Some(student) = self.student.as_deref().filter(|_| self.student_on) {
            let fix = self.revise_with_student(student, word, history, right);
            // Without the fallback the student alone decides; with it, what the student leaves alone
            // goes to the word statistics ("that là" -> "thật" is theirs).
            if fix.is_some() || !self.student_fallback {
                return fix;
            }
        }
        let keys = Self::keys_of(word)?;
        if self.protected(&keys) {
            return None;
        }
        if self.acronym_weak(word, f64::NEG_INFINITY) {
            return None;
        }
        let capital = word.chars().next().is_some_and(char::is_uppercase);
        let (typed_score, vietnamese) = self.typed(&keys);
        // A capitalised word that is known or follows another word is most likely a name.
        if capital && (typed_score > f64::NEG_INFINITY || !history.is_empty()) {
            return None;
        }
        let shown = vietnamese.clone().unwrap_or_else(|| keys.clone());
        let bare = self.vietnamese && self.restore && !capital && compose(&keys).text == keys;
        let (ranking, extra) = if bare {
            (self.rank_bare(&keys, history)?, Vec::new())
        } else {
            let ranking = self.rank_in(word, history)?;
            let extra: Vec<(String, f64)> = if vietnamese.is_some() {
                self.rank_expanded(word, history)
                    .map(|r| r.candidates)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|(w, _)| !ranking.candidates.iter().any(|(b, _)| b == w))
                    .collect()
            } else {
                Vec::new()
            };
            (ranking, extra)
        };
        let after = |text: &str| {
            let vi = self.vietnamese && self.vi.id(&text.to_lowercase()).is_some();
            self.right_context_score(vi, history, text, right)
        };
        let finite = |s: f64| if s.is_finite() { s } else { -99.0 };
        let keep = finite(ranking.typed) + after(&shown);
        let mut all: Vec<(String, f64, bool)> = ranking
            .candidates
            .iter()
            .map(|(w, s)| (w.clone(), finite(*s) + after(w), false))
            .chain(extra.iter().map(|(w, s)| (w.clone(), finite(*s) + after(w), true)))
            .collect();
        // Only Vietnamese readings (with marks): this is a Vietnamese revision, and
        // English words changed this way cost far more wrong fixes than they gain.
        all.retain(|(w, _, _)| !w.is_ascii());
        all.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let (top, top_score, expanded) = all.first()?.clone();
        let second = all.get(1).map_or(f64::NEG_INFINITY, |c| c.1);
        let margin = if expanded { self.tuning.revise_margin_x } else { self.tuning.revise_margin };
        (top_score - keep >= margin && top_score - second >= self.tuning.revise_ambiguity).then(|| match_case(word, &top))
    }

    /// Hindsight check of a correction already made: with the word that followed, is
    /// the word as typed a better fit than `fix`? True means take the fix back.
    /// Independent of how the fix was found: it only compares the two readings, each
    /// with the words before it and the words after it. Not used by the app yet.
    pub fn audit(&self, word: &str, fix: &str, history: &[&str], right: &[String]) -> bool {
        if right.is_empty() {
            return false;
        }
        let Some(keys) = Self::keys_of(word) else { return false };
        let capital = word.chars().next().is_some_and(char::is_uppercase);
        let (_, vietnamese) = self.typed(&keys);
        let shown = vietnamese.clone().unwrap_or_else(|| keys.clone());
        let bare = self.vietnamese && self.restore && !capital && compose(&keys).text == keys;
        let ranking = if bare { self.rank_bare(&keys, history) } else { self.rank_in(word, history) };
        let Some(ranking) = ranking else { return false };
        let fix_lower = fix.to_lowercase();
        let in_base = ranking.candidates.iter().find(|(w, _)| w.to_lowercase() == fix_lower).map(|c| c.1);
        let fix_score = in_base.or_else(|| {
            let expanded = if vietnamese.is_some() { self.rank_expanded(word, history) } else { None };
            expanded.and_then(|r| r.candidates.into_iter().find(|(w, _)| w.to_lowercase() == fix_lower).map(|c| c.1))
        });
        let Some(fix_score) = fix_score else { return false };
        let after = |text: &str| {
            let vi = self.vietnamese && self.vi.id(&text.to_lowercase()).is_some();
            self.right_context_score(vi, history, text, right)
        };
        let finite = |s: f64| if s.is_finite() { s } else { -99.0 };
        let original = finite(ranking.typed) + after(&shown);
        let fixed = finite(fix_score) + after(fix);
        original - fixed >= self.tuning.audit_margin
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

    /// Diagnostics: like [`Self::rank_in`], but a well-formed syllable may also be
    /// corrected by changing letters, not only marks. The app does not do this: such a
    /// correction needs more evidence than the word before.
    pub fn rank_expanded(&self, word: &str, history: &[&str]) -> Option<Ranking> {
        let keys = Self::keys_of(word)?;
        let (typed, vietnamese) = self.typed(&keys);
        let ctx = self.context_of(history);
        Some(Ranking { typed, candidates: self.candidates(&keys, vietnamese, ctx, false) })
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

    /// Diagnostics: ln probability of the words that follow `candidate`, given it and
    /// the words before (Kneser-Ney chain). Right context is what a delayed correction
    /// would have; the app does not have it when it decides. Words outside the lexicon
    /// cost a flat penalty. `vietnamese` picks the language tables.
    pub fn right_context_score(&self, vietnamese: bool, history: &[&str], candidate: &str, right: &[String]) -> f64 {
        let (lexicon, kn) = if vietnamese { (&self.vi, &self.vi_kn) } else { (&self.en, &self.en_kn) };
        if right.is_empty() || kn.is_empty() {
            return 0.0;
        }
        let mut ids: Vec<Option<u32>> = history.iter().rev().take(2).rev().map(|w| lexicon.id(&w.to_lowercase())).collect();
        ids.push(lexicon.id(&candidate.to_lowercase()));
        let prefix = ids.len();
        ids.extend(right.iter().map(|w| lexicon.id(&w.to_lowercase())));
        let mut total = 0.0;
        for k in prefix..ids.len() {
            total += match (ids[k], ids[k - 1]) {
                (Some(word), Some(prev)) => kn.ln_prob(if k >= 2 { ids[k - 2] } else { None }, prev, word),
                _ => -16.0,
            };
        }
        total
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
        let mut candidates = self.candidates(&keys, vietnamese, ctx, true);
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

    fn revise(&self, word: &str, history: &[&str], right: &[String]) -> Option<String> {
        SmartCorrector::revise(self, word, history, right)
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
        if self.protected(&keys) {
            return None;
        }
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

        let near = self.candidates(&keys, vietnamese, ctx, true);
        if self.unaccented_wins(&near) {
            return None;
        }
        let t = &self.tuning;
        if let Some(best) = confident(&near, typed_penalised, t.margin, t.floor, t.ambiguity) {
            if self.english_phrase(history, &near, best) || self.acronym_weak(word, near[0].1) {
                return None;
            }
            return Some(match_case(word, best));
        }
        // Keys that are no word and no word's slip: perhaps several words typed without
        // the spaces ("quanheej"). After the one-slip fixes, so that a plain typo is
        // never cut up, and before the two-slip ones, which would make one word of it.
        if typed == f64::NEG_INFINITY {
            if let Some(fix) = self.split_run(&keys, history) {
                return Some(match_case(word, &fix));
            }
        }
        // Two slips: only for unknown words, never names.
        if typed == f64::NEG_INFINITY && !capitalised && keys.len() >= FAR_MIN_KEYS {
            let far = self.far_candidates(&keys, ctx);
            if self.unaccented_wins(&far) {
                return None;
            }
            return confident(&far, typed, t.margin, t.far_floor, t.far_ambiguity)
                .filter(|best| !self.english_phrase(history, &far, best) && !self.acronym_weak(word, far[0].1))
                .map(|best| match_case(word, best));
        }
        None
    }
}

/// What `correct_in` weighs for one word, without deciding: the evidence a learned decision can use in
/// place of the thresholds (the experiment of RESEARCH.md, section 5). `correct_in` stays the reference.
#[derive(Debug, Clone)]
pub struct Evidence {
    /// A fix made before any scoring (the user's dictionary, a known misspelling, marks restored).
    pub forced: Option<String>,
    /// Left alone without scoring: ignored, too short, a common word, a name.
    pub skipped: bool,
    /// Score of the keys as typed (`-inf`: no known word).
    pub typed: f64,
    pub vietnamese: bool,
    pub capitalised: bool,
    /// Number of keys of the word.
    pub keys: usize,
    /// One-slip candidates, best first, at most six.
    pub near: Vec<(String, f64)>,
    /// The best near candidate is the same word without marks, which `correct_in` leaves alone.
    pub near_unaccented: bool,
    /// The keys cut into several syllables ("quanheej" -> "quan hệ"), if one cut stands out.
    pub split: Option<String>,
    /// Two-slip candidates, only for unknown words, best first, at most six.
    pub far: Vec<(String, f64)>,
    pub far_unaccented: bool,
}

impl Default for Evidence {
    fn default() -> Self {
        Self {
            forced: None,
            skipped: false,
            typed: f64::NEG_INFINITY,
            vietnamese: false,
            capitalised: false,
            keys: 0,
            near: Vec::new(),
            near_unaccented: false,
            split: None,
            far: Vec::new(),
            far_unaccented: false,
        }
    }
}

impl SmartCorrector {
    /// The same steps as [`Corrector::correct_in`], stopping where it would compare thresholds.
    pub fn evidence(&self, word: &str, history: &[&str]) -> Evidence {
        let mut e = Evidence::default();
        let lower = word.to_lowercase();
        if let Some(instead) = self.personal.fix(&lower) {
            e.forced = Some(match_case(word, instead));
            return e;
        }
        let Some(keys) = Self::keys_of(word).filter(|_| !self.personal.ignores(&lower)) else {
            e.skipped = true;
            return e;
        };
        e.keys = keys.len();
        if self.english {
            if let Some(fix) = self.misspellings.get(&keys) {
                e.forced = Some(match_case(word, fix));
                return e;
            }
        }
        if self.vietnamese && self.restore {
            if let Some(fix) = self.restore_marks(word, &keys, history) {
                e.forced = Some(fix);
                return e;
            }
        }
        let (typed, vietnamese) = self.typed(&keys);
        e.typed = typed;
        e.vietnamese = vietnamese.is_some();
        let known = if e.vietnamese { self.tuning.known_syllable } else { self.tuning.known_word };
        if typed >= known {
            e.skipped = true;
            return e;
        }
        e.capitalised = word.chars().next().is_some_and(char::is_uppercase);
        if e.capitalised && (typed > f64::NEG_INFINITY || !history.is_empty()) {
            e.skipped = true;
            return e;
        }
        let ctx = self.context_of(history);
        e.near = self.candidates(&keys, vietnamese, ctx, true);
        e.near_unaccented = self.unaccented_wins(&e.near);
        e.near.truncate(6);
        if typed == f64::NEG_INFINITY {
            e.split = self.split_run(&keys, history);
            if !e.capitalised && keys.len() >= FAR_MIN_KEYS {
                let far = self.far_candidates(&keys, ctx);
                e.far_unaccented = self.unaccented_wins(&far);
                e.far = far.into_iter().take(6).collect();
            }
        }
        e
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

    /// What "namk" becomes with and without the word before it: `cargo test -p ac-core probe_namk -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn probe_namk() {
        let c = corrector();
        for history in [vec![], vec!["việt"], vec!["nhất", "việt"], vec!["tiếng", "hay", "nhất", "việt"]] {
            let r = c.rank_in("namk", &history);
            let top: Vec<String> = r.iter().flat_map(|r| r.candidates.iter().take(4)).map(|(w, s)| format!("{w} {s:.1}")).collect();
            println!("{:<30} -> {:<10} typed {:>6.1} {top:?}", history.join(" "), format!("{:?}", c.correct_in("namk", &history)), r.as_ref().map_or(f64::NAN, |r| r.typed));
        }
    }

    /// English words typed through Telex's "press the tone key twice" escape, as in the user's journal:
    /// `cargo test -p ac-core probe_telex_escapes -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn probe_telex_escapes() {
        let c = corrector();
        for keys in ["tesst", "casse", "json", "roff", "boff", "iff", "jsont", "thatj", "khooong", "banjj"] {
            let composed = compose(keys);
            println!("{keys:<8} shows {:<8} ({:?})  corrected to {:?}", composed.text, composed.kind, c.correct_in(keys, &["bạn", "ơi"]));
        }
    }

    /// An English word typed through Telex's escape (a tone key pressed twice) is what the writer meant,
    /// not a Vietnamese word to be restored (journal 2026-10-07: "tesst" -> "tết" ten times).
    #[test]
    fn an_escaped_english_word_is_left_alone() {
        let c = corrector();
        for keys in ["tesst", "casse", "iff"] {
            assert_eq!(c.correct_in(keys, &["bạn", "ơi"]), None, "{keys}");
            assert_eq!(c.correct_in(keys, &[]), None, "{keys}");
        }
        // Telex words and plain typos are still corrected.
        assert_eq!(c.correct_in("thatj", &[]).as_deref(), Some("thật"));
        assert_eq!(c.correct_in("banjj", &["bạn", "ơi"]).as_deref(), Some("bạn"));
    }

    /// Wrong fixes seen in the journal (2026-10-08, AUGMENT_RULES.md F2 and F3): an acronym, keys that start
    /// with j, an English word after an English word, one word cut in two with a one-letter piece.
    #[test]
    fn guards_keep_journal_wrong_fixes_away() {
        let c = corrector();
        assert_eq!(c.correct_in("json", &[]), None);
        assert_eq!(c.correct_in("LLM", &[]), None);
        assert_eq!(c.correct_in("Tiees", &[]), None);
        assert_eq!(c.correct_in("tiees", &[]), None);
        assert_eq!(c.correct_in("Giowow", &[]), None);
        assert_eq!(c.correct_in("codin", &["vibe"]), None);
        // What they must not cost: a word of capitals typed with Caps Lock, a cut into real syllables.
        assert_eq!(c.correct_in("TEH", &[]).as_deref(), Some("THE"));
        assert_eq!(c.correct_in("quanheej", &[]).as_deref(), Some("quan hệ"));
    }

    /// A letter typed twice or three times, with and without the word after it:
    /// `cargo test -p ac-core probe_repeated_letters -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn probe_repeated_letters() {
        let c = corrector();
        for typed in ["nguu", "nguuu", "ngguu", "ngoocc"] {
            let history = ["con", "vịt"];
            let right = vec!["ngốc".to_string()];
            println!("{typed:<8} now: {:?}   revised with 'ngốc': {:?}", c.correct_in(typed, &history), c.revise(typed, &history, &right));
        }
    }

    /// Delayed revision: the word after it settles what a word could not say alone.
    #[test]
    fn revises_a_word_once_the_next_one_is_known() {
        let mut c = corrector();
        let words = |w: &str| vec![w.to_string()];
        // "that" is an English word and a bare Vietnamese syllable: alone, undecided.
        assert_eq!(c.correct_in("that", &[]), None);
        assert_eq!(c.revise("that", &[], &words("là")).as_deref(), Some("thật"));
        // "ngẫy" is a valid syllable; "ngẫy nhiên" is not a pair, "ngẫu nhiên" is.
        let typed = to_keys("ngẫy");
        assert_eq!(c.correct_in(&typed, &[]), None);
        assert_eq!(c.revise(&typed, &["là"], &words("nhiên")).as_deref(), Some("ngẫu"));
        // Plain English and a finished correct word stay as they are.
        assert_eq!(c.revise("hello", &["say"], &words("there")), None);
        assert_eq!(c.revise("bạn", &["cho"], &words("nhé")), None);
        // Not in Vietnamese mode: no revision.
        c.set_languages(false, true);
        assert_eq!(c.revise("that", &[], &words("là")), None);
    }

    /// The engine with the real corrector, typing Telex: the next word revises the one before.
    #[test]
    fn engine_revises_with_the_real_corrector() {
        use crate::{Action, Engine, Key};
        let run = |text: &str| -> Vec<Action> {
            let mut e = Engine::new(corrector());
            e.set_vietnamese(true);
            e.set_delayed(true);
            text.chars().map(|c| e.on_key(if c == ' ' { Key::Space } else { Key::Char(c) })).collect()
        };
        // "that laf " -> "thật là ": the last Space rewrites both words.
        let actions = run("that laf ");
        match actions.last() {
            Some(Action::Replace { text, .. }) => assert!(text.ends_with("ật là "), "{text:?}"),
            other => panic!("expected a revision, got {other:?}"),
        }
        // Without the second word there is nothing to revise.
        assert!(run("that ").iter().all(|a| matches!(a, Action::Pass | Action::Replace { .. })));
        assert_eq!(run("that ").last(), Some(&Action::Pass));
    }

    /// Words typed without the spaces between them are cut into syllables.
    #[test]
    fn splits_words_typed_without_spaces() {
        let c = corrector();
        assert_eq!(c.correct_in("quanheej", &[]).as_deref(), Some("quan hệ"));
        assert_eq!(c.correct_in("Quanheej", &[]).as_deref(), Some("Quan hệ"));
        // Real words and ordinary typos are not cut.
        assert_eq!(c.correct_in("nhieeu", &[]), None);
        assert_eq!(c.correct_in("teh", &[]).as_deref(), Some("the"));
    }

    /// The engine puts the spaces in, keeps the last word as context, and Ctrl+Z
    /// brings the keys back.
    #[test]
    fn engine_cuts_a_run_of_words_and_undoes_it() {
        use crate::{Action, Engine, Key};
        let mut e = Engine::new(corrector());
        e.set_vietnamese(true);
        for c in "mooitruwowngf".chars() {
            e.on_key(Key::Char(c));
        }
        e.on_key(Key::Space);
        for c in "quanheej".chars() {
            e.on_key(Key::Char(c));
        }
        match e.on_key(Key::Space) {
            // Only what differs from the screen is sent: "quan" is already there.
            Action::Replace { text, .. } => assert!(text.ends_with(" hệ "), "{text:?}"),
            other => panic!("expected a split, got {other:?}"),
        }
        assert_eq!(e.context(), Some("hệ"));
        assert_eq!(e.history().last().map(String::as_str), Some("hệ"));
        assert!(matches!(e.on_key(Key::Undo), Action::Replace { .. }));
    }

    /// `evidence` and `correct_in` agree: what one calls skipped the other leaves alone, a forced fix is
    /// the fix, and any other fix is one of the candidates listed.
    #[test]
    fn evidence_agrees_with_correct_in() {
        let c = corrector();
        let histories: [&[&str]; 3] = [&[], &["tôi"], &["chúng", "ta"]];
        for word in ["teh", "quanheej", "nhieeu", "khong", "Tuan", "the", "chaof", "xin", "thicsk", "Wolff", "that", "ngẫy", "cuar", "dduowcj", "recieve"] {
            for history in histories {
                let e = c.evidence(word, history);
                let fix = c.correct_in(word, history);
                if let Some(forced) = &e.forced {
                    assert_eq!(fix.as_ref(), Some(forced), "{word}");
                } else if e.skipped {
                    assert_eq!(fix, None, "{word} {history:?}");
                } else if let Some(fix) = fix {
                    let lower = fix.to_lowercase();
                    let listed = e.near.iter().chain(&e.far).any(|(w, _)| w.to_lowercase() == lower)
                        || e.split.as_ref().is_some_and(|s| s.to_lowercase() == lower);
                    assert!(listed, "{word} {history:?}: {fix:?} is not in {e:?}");
                }
            }
        }
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

    /// What a delayed revision costs per word: `cargo test -p ac-core --release speed_revise -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn speed_revise() {
        let c = corrector();
        for (w, history, next) in [("that", vec![], "là"), ("bạn", vec!["cho"], "nhé"), ("hello", vec!["say"], "there"), ("không", vec!["tôi"], "biết")] {
            let right = vec![next.to_string()];
            let start = std::time::Instant::now();
            for _ in 0..200 {
                std::hint::black_box(c.revise(&to_keys(w), &history, &right));
            }
            println!("{w:>8}: {:>7.1} us/word", start.elapsed().as_secs_f64() * 1e6 / 200.0);
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

    /// "namk" in a terminal (typed after "việt"): the best reading is the unaccented
    /// "nam". It is not applied there, but it must not hand the win to "năm" either.
    #[test]
    fn code_mode_does_not_let_a_worse_accented_word_win() {
        let mut c = corrector();
        c.set_languages(true, false);
        assert_eq!(c.correct_in("namk", &["việt"]), None);
        assert_eq!(c.correct_in("namk", &[]), None);
        // With English corrections on the unaccented fix is allowed, as before.
        c.set_languages(true, true);
        assert_eq!(c.correct_in("namk", &["việt"]).as_deref(), Some("nam"));
    }

    #[test]
    fn log_add_is_stable() {
        assert!((log_add(0.0, 0.0) - 2f64.ln()).abs() < 1e-12);
        assert_eq!(log_add(f64::NEG_INFINITY, 5.0), 5.0);
    }
}
