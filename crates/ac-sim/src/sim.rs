//! One simulated typist at one kind of app: the app's engine and screen, and
//! a second lane with corrections off that shows each word as typed.

use std::collections::BTreeMap;

use ac_core::{Action, Corrector, DictCorrector, Engine, Key, SmartCorrector, Tuning};
use ac_telex::Method;

use crate::screen::Screen;
use crate::typist::{mark_key, Profile, Slip, Token};
use crate::Rng;

/// The kind of app the text is typed in (`AppKind` in the app).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Env {
    Normal,
    /// Terminals and code editors.
    Code,
}

impl Env {
    pub fn name(self) -> &'static str {
        match self {
            Env::Normal => "normal",
            Env::Code => "code",
        }
    }
}

/// The app settings that change what the engine does.
#[derive(Debug, Clone, Copy)]
pub struct AppSettings {
    pub delayed: bool,
    pub restore_marks: bool,
    pub code_english: bool,
    /// `Tuning::preset` level: 0 careful, 1 balanced, 2 bold.
    pub strength: u8,
}

impl Default for AppSettings {
    /// The app's defaults, with delayed revision on.
    fn default() -> Self {
        Self { delayed: true, restore_marks: true, code_english: false, strength: 1 }
    }
}

/// Sets the engine up as `hook.rs` (`State::apply`) does for this kind of app,
/// in Vietnamese mode with Telex and corrections on: keep the two in step.
pub fn configure(engine: &mut Engine<SmartCorrector>, env: Env, s: AppSettings) {
    engine.set_method(Method::Telex);
    engine.set_delayed(s.delayed && env == Env::Normal);
    engine.set_vietnamese(true);
    let english = env == Env::Normal || s.code_english;
    engine.set_corrections(true, english);
    engine.set_restore_marks(s.restore_marks && env == Env::Normal);
    engine.corrector_mut().set_tuning(Tuning::preset(s.strength));
}

/// What became of a word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Outcome {
    /// Typed right and left alone.
    Kept,
    /// Typed right, changed by the app: the worst outcome.
    Broken,
    /// Typed wrong, corrected to the word meant.
    Fixed,
    /// Typed wrong, left as typed.
    Missed,
    /// Typed wrong, changed into another wrong word.
    Wrong,
}

impl Outcome {
    pub fn name(self) -> &'static str {
        match self {
            Outcome::Kept => "kept",
            Outcome::Broken => "broken",
            Outcome::Fixed => "fixed",
            Outcome::Missed => "missed",
            Outcome::Wrong => "wrong",
        }
    }
}

#[derive(Debug, Clone)]
pub struct WordResult {
    pub intended: String,
    /// The keys typed for it, slip included (before any fix by the typist).
    pub keys: String,
    pub slip: Option<Slip>,
    /// The typist saw the slip and fixed it with Backspace before ending the word.
    pub typist_fixed: bool,
    /// The word as typed: the second lane, corrections off.
    pub baseline: String,
    /// The word on screen at the end of the sentence.
    pub shown: String,
    /// What the app last changed it into, if it did.
    pub app_wrote: Option<String>,
    /// Ctrl+Z was pressed after the app changed it.
    pub undone: bool,
    /// What the app's decision was worth: a change that was undone is judged by
    /// what the app wrote, not by the screen the typist cleaned up.
    pub outcome: Outcome,
}

impl WordResult {
    /// A word as typed, before it is scored.
    fn typed(intended: String, keys: String, slip: Option<Slip>, typist_fixed: bool) -> Self {
        Self {
            intended,
            keys,
            slip,
            typist_fixed,
            baseline: String::new(),
            shown: String::new(),
            app_wrote: None,
            undone: false,
            outcome: Outcome::Kept,
        }
    }
}

/// The screen text of each word meant, in order, or `None` if the screen no
/// longer lines up with them. Several words typed without the spaces between
/// them ("quan hệ" as "quanheej") are one word meant that covers one screen
/// word, or as many as the app split it into; there is at most one such run per
/// sentence, so the number of extra screen words tells how many.
fn per_word(screen: &[String], words: &[WordResult]) -> Option<Vec<String>> {
    let extra = screen.len().checked_sub(words.len())?;
    if extra > 0 && !words.iter().any(|w| w.intended.contains(' ')) {
        return None;
    }
    let mut out = Vec::with_capacity(words.len());
    let mut at = 0;
    for w in words {
        let take = if w.intended.contains(' ') { 1 + extra } else { 1 };
        out.push(screen.get(at..at + take)?.join(" "));
        at += take;
    }
    (at == screen.len()).then_some(out)
}

/// A sentence typed in full: its words, or why they could not be scored.
pub enum Typed {
    Words(Vec<WordResult>),
    /// The words on screen no longer line up with the words meant.
    Misaligned,
}

struct Lane<C: Corrector> {
    engine: Engine<C>,
    screen: Screen,
}

impl<C: Corrector> Lane<C> {
    fn press(&mut self, key: Key, typed: Option<char>) -> Action {
        let action = self.engine.on_key(key);
        self.screen.apply(key, typed, &action);
        action
    }
}

pub struct Sim {
    main: Lane<SmartCorrector>,
    /// The same keys with corrections off (Telex still composes).
    plain: Lane<DictCorrector>,
    profile: Profile,
    rng: Rng,
}

impl Sim {
    pub fn new(corrector: SmartCorrector, env: Env, settings: AppSettings, profile: Profile, seed: u64) -> Self {
        let mut engine = Engine::new(corrector);
        configure(&mut engine, env, settings);
        let mut plain = Engine::new(DictCorrector::new(std::iter::empty::<(&str, &str)>()));
        plain.set_method(Method::Telex);
        plain.set_vietnamese(true);
        plain.set_corrections(false, false);
        Self {
            main: Lane { engine, screen: Screen::default() },
            plain: Lane { engine: plain, screen: Screen::default() },
            profile,
            rng: Rng::new(seed),
        }
    }

    /// Where a run of 2 to `join_max` Vietnamese words typed without spaces starts, and
    /// its length, if this sentence gets one.
    fn pick_run(&mut self, plan: &[Token]) -> Option<(usize, usize)> {
        if self.profile.join <= 0.0 || !self.rng.chance(self.profile.join) {
            return None;
        }
        let vi = |i: usize| matches!(plan.get(i), Some(Token::Word { vietnamese: true, .. }));
        // How many Vietnamese words follow each other, a single space apart, from `i`.
        let reach = |i: usize| (0..).take_while(|n| vi(i + 2 * n) && (plan.get(i + 2 * n + 1) == Some(&Token::Space) || !vi(i + 2 * n + 2))).count();
        let starts: Vec<(usize, usize)> =
            (0..plan.len()).map(|i| (i, reach(i).min(self.profile.join_max))).filter(|&(_, k)| k >= 2).collect();
        let (at, longest) = *starts.get(self.rng.below(starts.len().max(1)))?;
        Some((at, 2 + self.rng.below(longest - 1)))
    }

    /// A key to both lanes; what the app's engine did.
    fn press(&mut self, key: Key, typed: Option<char>) -> Action {
        self.plain.press(key, typed);
        self.main.press(key, typed)
    }

    /// Types one sentence on a fresh line and scores every word of it.
    pub fn type_sentence(&mut self, plan: &[Token]) -> Typed {
        // A new line: Enter is a Reset, nothing before it is known any more.
        self.press(Key::Reset, None);
        self.main.screen.clear();
        self.plain.screen.clear();
        let mut words: Vec<WordResult> = Vec::new();
        // Now and then the spaces between a run of Vietnamese words are left out.
        let join = self.pick_run(plan);
        let mut i = 0;
        while i < plan.len() {
            if let Some((at, k)) = join {
                if at == i {
                    let run: Vec<(&String, &String)> = (0..k)
                        .filter_map(|n| match &plan[i + 2 * n] {
                            Token::Word { text, keys, .. } => Some((text, keys)),
                            _ => None,
                        })
                        .collect();
                    let typed: String = run.iter().map(|(_, keys)| keys.as_str()).collect();
                    for c in typed.chars() {
                        self.press(Key::Char(c), Some(c));
                    }
                    let meant = run.iter().map(|(text, _)| text.as_str()).collect::<Vec<_>>().join(" ");
                    words.push(WordResult::typed(meant, typed, Some(Slip::SpaceMissing), false));
                    i += 2 * k - 1;
                    continue;
                }
            }
            match &plan[i] {
                Token::Word { text, keys, vietnamese } => {
                    // Now and then a word in brackets or quotes, right against it.
                    let alone = (i == 0 || plan[i - 1] == Token::Space) && plan.get(i + 1).is_none_or(|t| *t == Token::Space);
                    let wrap = (alone && self.rng.chance(self.profile.wrap))
                        .then(|| if self.rng.chance(0.5) { ('(', ')') } else { ('"', '"') });
                    if let Some((open, _)) = wrap {
                        self.press(mark_key(open), Some(open));
                    }
                    let slip =
                        if self.rng.chance(self.profile.rate) { self.profile.slip(keys, *vietnamese, &mut self.rng) } else { None };
                    let typed = slip.as_ref().map_or_else(|| keys.clone(), |(_, k)| k.clone());
                    for c in typed.chars() {
                        self.press(Key::Char(c), Some(c));
                    }
                    let typist_fixed = slip.is_some() && self.rng.chance(self.profile.notice) && self.retype(text, keys);
                    words.push(WordResult::typed(text.clone(), typed, slip.map(|(kind, _)| kind), typist_fixed));
                    if let Some((_, close)) = wrap {
                        self.boundary(mark_key(close), Some(close), &mut words);
                    }
                }
                Token::Space => self.boundary(Key::Space, Some(' '), &mut words),
                Token::Mark(c) => match mark_key(*c) {
                    key @ (Key::Punct(_) | Key::Close(_)) => self.boundary(key, Some(*c), &mut words),
                    key => {
                        self.press(key, Some(*c));
                    }
                },
            }
            i += 1;
        }
        let (Some(shown), Some(baseline)) =
            (per_word(&self.main.screen.words(), &words), per_word(&self.plain.screen.words(), &words))
        else {
            return Typed::Misaligned;
        };
        for ((w, shown), baseline) in words.iter_mut().zip(shown).zip(baseline) {
            let judged = if w.undone { w.app_wrote.clone().unwrap_or_else(|| shown.clone()) } else { shown.clone() };
            w.outcome = match (baseline == w.intended, is_meant(&judged, &w.intended, &baseline)) {
                (true, true) => Outcome::Kept,
                (true, false) => Outcome::Broken,
                (false, true) => Outcome::Fixed,
                (false, false) if judged == baseline => Outcome::Missed,
                (false, false) => Outcome::Wrong,
            };
            w.shown = shown;
            w.baseline = baseline;
        }
        Typed::Words(words)
    }

    /// A key that ends a word, which may correct it or revise the one before.
    /// The typist looks at the words that changed and may press Ctrl+Z.
    fn boundary(&mut self, key: Key, typed: Option<char>, words: &mut [WordResult]) {
        let before = per_word(&self.main.screen.words(), words);
        if self.press(key, typed) == Action::Pass {
            return;
        }
        let (Some(before), Some(after)) = (before, per_word(&self.main.screen.words(), words)) else {
            return;
        };
        let changed: Vec<usize> = (0..after.len()).filter(|&i| before[i] != after[i]).collect();
        for &i in &changed {
            words[i].app_wrote = Some(after[i].clone());
        }
        let typed = per_word(&self.plain.screen.words(), words).unwrap_or_default();
        let wrong =
            changed.iter().any(|&i| !is_meant(&after[i], &words[i].intended, typed.get(i).map_or("", String::as_str)));
        if wrong && self.rng.chance(self.profile.undo) {
            // The main lane only: in the plain one it would be the program's own undo.
            if self.main.press(Key::Undo, None) != Action::Pass {
                for i in changed {
                    words[i].undone = true;
                }
            }
        }
    }

    /// The typist fixes the word being typed: Backspace back to where it went
    /// wrong, then the right keys; if that does not give the word, the whole
    /// word again. True if the word is right in the end.
    fn retype(&mut self, text: &str, keys: &str) -> bool {
        for attempt in 0..2 {
            let shown: Vec<char> = self.main.screen.last_word().chars().collect();
            if shown.iter().copied().eq(text.chars()) {
                return true;
            }
            let common = shown.iter().zip(text.chars()).take_while(|(a, b)| **a == *b).count();
            // The keys the engine reads back for what is kept ("việ" -> "vieej").
            let kept_keys = Method::Telex.keys_for(&text.chars().take(common).collect::<String>());
            let (delete, rest) = match keys.strip_prefix(kept_keys.as_str()) {
                Some(rest) if attempt == 0 && common > 0 => (shown.len() - common, rest),
                _ => (shown.len(), keys),
            };
            for _ in 0..delete {
                self.press(Key::Backspace, None);
            }
            for c in rest.chars() {
                self.press(Key::Char(c), Some(c));
            }
        }
        self.main.screen.last_word() == text
    }
}

/// The word meant, allowing for a capital the typist's own slip took away:
/// for "haatj" (meant "Nhật"), "nhật" is the best anyone can do.
fn is_meant(shown: &str, intended: &str, baseline: &str) -> bool {
    let capital_lost = intended.chars().next().is_some_and(char::is_uppercase) && !baseline.chars().any(char::is_uppercase);
    shown == intended || (capital_lost && shown.to_lowercase() == intended.to_lowercase())
}

/// Counts of outcomes, overall and per slip kind.
#[derive(Debug, Default)]
pub struct Tally {
    pub sentences: u32,
    pub skipped: u32,
    pub misaligned: u32,
    pub outcomes: BTreeMap<Outcome, u32>,
    /// Per slip kind (`None`: no slip, yet the word as typed was not the word
    /// meant: Telex changed it, as with English words typed in Vietnamese mode).
    pub by_slip: BTreeMap<Option<Slip>, BTreeMap<Outcome, u32>>,
    /// Runs of words typed without spaces, by the number of words in the run.
    pub by_run: BTreeMap<usize, BTreeMap<Outcome, u32>>,
    pub typist_fixed: u32,
    pub undone: u32,
}

impl Tally {
    pub fn add(&mut self, w: &WordResult) {
        *self.outcomes.entry(w.outcome).or_default() += 1;
        // Only words still wrong when they were ended: one the typist fixed is not
        // the app's to fix.
        if w.slip == Some(Slip::SpaceMissing) {
            let k = w.intended.split(' ').count();
            *self.by_run.entry(k).or_default().entry(w.outcome).or_default() += 1;
        }
        if w.baseline != w.intended {
            *self.by_slip.entry(w.slip).or_default().entry(w.outcome).or_default() += 1;
        }
        self.typist_fixed += u32::from(w.typist_fixed);
        self.undone += u32::from(w.undone);
    }

    pub fn count(&self, outcome: Outcome) -> u32 {
        self.outcomes.get(&outcome).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::typist::plan;
    use ac_core::{Kn, Lexicon};

    fn sim(profile: Profile) -> Sim {
        let vi = Lexicon::parse(include_str!("../../../data/vi_syllables.tsv"));
        let en = Lexicon::parse(include_str!("../../../data/en_words.tsv"));
        let vi_kn = Kn::from_bytes(include_bytes!("../../../data/vi_kn.bin"), vi.len());
        let en_kn = Kn::from_bytes(include_bytes!("../../../data/en_kn.bin"), en.len());
        let corrector = SmartCorrector::new(vi, en).with_kn(vi_kn, en_kn);
        Sim::new(corrector, Env::Normal, AppSettings::default(), profile, 1)
    }

    fn quiet() -> Profile {
        Profile { rate: 0.0, wrap: 0.0, join: 0.0, ..Profile::default() }
    }

    fn outcomes(typed: Typed) -> Vec<(String, Outcome)> {
        let Typed::Words(words) = typed else { panic!("misaligned") };
        words.into_iter().map(|w| (w.shown, w.outcome)).collect()
    }

    #[test]
    fn typed_right_stays_right() {
        let mut s = sim(quiet());
        let plan = plan("Hôm nay (thứ Hai) trời đẹp, tôi đi học \"sớm\".").unwrap();
        assert!(outcomes(s.type_sentence(&plan)).iter().all(|(_, o)| *o == Outcome::Kept));
    }

    #[test]
    fn a_slip_right_before_a_closing_bracket_is_fixed() {
        let mut s = sim(quiet());
        let mut plan = plan("vì sao)").unwrap();
        plan[2] = Token::Word { text: "sao".into(), keys: "saoi".into(), vietnamese: true };
        assert_eq!(outcomes(s.type_sentence(&plan))[1], ("sao".to_string(), Outcome::Fixed));
    }

    #[test]
    fn two_words_typed_without_the_space_are_scored_as_one() {
        let mut s = sim(Profile { join: 1.0, ..quiet() });
        let plan = plan("mối quan hệ").unwrap();
        // The run is at most `join_max` words long.
        let mut short = sim(Profile { join: 1.0, join_max: 2, ..quiet() });
        let Typed::Words(words) = short.type_sentence(&plan) else { panic!("misaligned") };
        assert!(words.iter().all(|w| w.intended.split(' ').count() <= 2));
        let Typed::Words(words) = s.type_sentence(&plan) else { panic!("misaligned") };
        let joined: Vec<_> = words.iter().filter(|w| w.slip == Some(Slip::SpaceMissing)).collect();
        assert_eq!(joined.len(), 1);
        assert!(["mối quan", "quan hệ", "mối quan hệ"].contains(&joined[0].intended.as_str()));
        assert!(words.len() <= 2);
    }
}
