use std::collections::HashMap;

use ac_telex::{compose, to_keys};

use crate::Corrector;

/// A key event, already decoded by the platform layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A character that belongs to a word.
    Char(char),
    Backspace,
    /// Word boundary that triggers correction; typed as a space.
    Space,
    /// Punctuation that ends a word (, . ; : ! ?). The key itself still
    /// reaches the program, right after any correction.
    Punct(char),
    /// Ctrl+Z. Right after a correction it undoes that correction (and is
    /// swallowed); at any other time it is the program's own undo and passes.
    Undo,
    /// Select all (Ctrl+A): a Backspace right after it empties the field, so
    /// the text that follows starts clean.
    SelectAll,
    /// Anything that may move the caret or change the text out of our sight
    /// (mouse click, arrows, Enter, shortcuts, focus change...).
    Reset,
}

/// What the platform layer must do with the key that was just pressed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Let the key through untouched.
    Pass,
    /// Swallow the key, send `backspaces` Backspaces, then type `text`.
    Replace { backspaces: usize, text: String },
    /// Send the edit, then let the key through too (punctuation).
    ReplaceThenPass { backspaces: usize, text: String },
}

/// One word: the raw keys typed and the text they show on screen.
#[derive(Debug, Clone, Default)]
struct Word {
    keys: String,
    shown: String,
    /// No longer composed: keys show as typed (e.g. after backspacing into a
    /// word that cancelled a mark, "tesst" -> "test" -> "tes").
    literal: bool,
}

struct LastCorrection {
    /// The word as it was before the correction.
    original: Word,
    corrected: String,
    /// The word before it, restored when the correction is undone.
    context: Option<String>,
    /// What ended the word: a space, or the punctuation typed.
    delimiter: char,
    /// A Backspace deleted that delimiter (the word is being resumed), so
    /// the screen shows the fix alone.
    delimiter_removed: bool,
}

/// A word undone this many times is never corrected again this session.
const IGNORE_AFTER_UNDOS: u8 = 2;

/// Outcome of the last word boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Corrected,
    EmptyWord,
    /// The corrector has no fix (word is fine or unknown).
    NoCandidate,
    /// The word was just restored by an undo and finished unchanged.
    JustUndone,
    /// The user undid this word's correction this many times.
    IgnoredAfterUndos(u8),
    /// Backspace reached text we never saw, so the buffer may hold only the
    /// tail of the on-screen word: correcting it could corrupt the text.
    Untracked,
    /// Corrections are off (user setting or app policy).
    Disabled,
}

pub struct Engine<C: Corrector> {
    corrector: C,
    /// Vietnamese mode: letters are composed with Telex as they are typed.
    vietnamese: bool,
    /// Which languages typos are corrected in (user setting, app policy).
    correct_vietnamese: bool,
    correct_english: bool,
    word: Word,
    /// The previous word, followed on screen by one space. Lets a Backspace
    /// over that space resume editing the word.
    prev_word: Option<Word>,
    /// The finished word before the current one, as shown on screen: what
    /// the corrector uses to rank candidates. `None` at the start of a line
    /// or after anything that may have moved the caret.
    context: Option<String>,
    /// The buffer is not known to hold the whole on-screen word.
    untracked: bool,
    /// Characters typed since the last reset that are no longer part of
    /// `word` (finished words and their delimiters). Backspacing exactly that
    /// many brings the caret back to where typing began, a clean start.
    committed: usize,
    /// The previous key was Select all.
    select_all: bool,
    /// Keys are only followed, not acted on: no Telex, no corrections. Used
    /// while it is not yet known whether a password field has focus.
    observing: bool,
    /// A word the user has now undone often enough to never correct again.
    learned: Option<String>,
    last: Option<LastCorrection>,
    /// Keys of the word just restored by an undo: finishing it unchanged
    /// keeps it as is.
    just_undone: Option<String>,
    /// Undo count per lowercased keys, this session.
    undos: HashMap<String, u8>,
    decision: Decision,
}

impl<C: Corrector> Engine<C> {
    /// Starts in English mode (keys are shown as typed).
    pub fn new(corrector: C) -> Self {
        Self {
            corrector,
            vietnamese: false,
            correct_vietnamese: true,
            correct_english: true,
            word: Word::default(),
            prev_word: None,
            context: None,
            untracked: false,
            committed: 0,
            select_all: false,
            observing: false,
            learned: None,
            last: None,
            just_undone: None,
            undos: HashMap::new(),
            decision: Decision::EmptyWord,
        }
    }

    /// Give Vietnamese words typed without marks ("khong") their marks.
    pub fn set_restore_marks(&mut self, on: bool) {
        self.corrector.set_restore_marks(on);
    }

    pub fn set_vietnamese(&mut self, on: bool) {
        self.vietnamese = on;
        self.sync_corrector();
        self.on_key(Key::Reset);
    }

    /// Languages typos may be corrected in. Vietnamese corrections also need
    /// Vietnamese mode, since they read the keys as Telex.
    pub fn set_corrections(&mut self, vietnamese: bool, english: bool) {
        self.correct_vietnamese = vietnamese;
        self.correct_english = english;
        self.sync_corrector();
    }

    fn sync_corrector(&mut self) {
        let vietnamese = self.vietnamese && self.correct_vietnamese;
        self.corrector.set_languages(vietnamese, self.correct_english);
    }

    fn corrections_on(&self) -> bool {
        (self.vietnamese && self.correct_vietnamese) || self.correct_english
    }

    /// While observing, typed keys pass through untouched (nothing is
    /// composed or corrected) but are still tracked, so that the word is
    /// whole when observing ends.
    pub fn set_observing(&mut self, on: bool) {
        self.observing = on;
    }

    pub fn is_vietnamese(&self) -> bool {
        self.vietnamese
    }

    /// The current word as shown on screen.
    pub fn current_word(&self) -> &str {
        &self.word.shown
    }

    /// The raw keys of the current word.
    pub fn current_keys(&self) -> &str {
        &self.word.keys
    }

    pub fn corrector(&self) -> &C {
        &self.corrector
    }

    pub fn corrector_mut(&mut self) -> &mut C {
        &mut self.corrector
    }

    /// The word the user has just undone for the second time, once: to be
    /// remembered for good (the engine itself ignores it for the session).
    pub fn take_learned(&mut self) -> Option<String> {
        self.learned.take()
    }

    /// The word before the current one, as shown on screen.
    pub fn context(&self) -> Option<&str> {
        self.context.as_deref()
    }

    /// The correction a Backspace would undo right now: (keys typed, fix).
    pub fn last_correction(&self) -> Option<(&str, &str)> {
        self.last.as_ref().map(|l| (l.original.keys.as_str(), l.corrected.as_str()))
    }

    pub fn on_key(&mut self, key: Key) -> Action {
        let after_select_all = std::mem::take(&mut self.select_all);
        match key {
            // Select all then delete leaves the field empty: nothing unseen.
            Key::Backspace if after_select_all => {
                self.clear();
                Action::Pass
            }
            Key::SelectAll => {
                self.clear();
                self.select_all = true;
                Action::Pass
            }
            Key::Char(c) => {
                self.last = None;
                self.on_char(c)
            }
            Key::Backspace => self.on_backspace(),
            Key::Space => self.on_boundary(' '),
            Key::Punct(c) => self.on_boundary(c),
            Key::Undo => self.on_undo(),
            Key::Reset => {
                self.clear();
                Action::Pass
            }
        }
    }

    /// Forgets everything about the text on screen.
    fn clear(&mut self) {
        self.word = Word::default();
        self.prev_word = None;
        self.context = None;
        self.untracked = false;
        self.committed = 0;
        self.last = None;
        self.just_undone = None;
    }

    /// Why the last Space did or did not correct the word (for diagnostics).
    pub fn last_decision(&self) -> Decision {
        self.decision
    }

    /// In Vietnamese mode the whole word is recomposed on every key; the key
    /// passes through untouched unless the composition changed earlier text.
    fn on_char(&mut self, c: char) -> Action {
        self.word.keys.push(c);
        if !self.vietnamese || self.word.literal || self.observing {
            self.word.shown.push(c);
            // Typed as is: the rest of this word is not composed either.
            self.word.literal |= self.observing;
            return Action::Pass;
        }
        let composed = compose(&self.word.keys).text;
        let as_typed = self.word.shown.chars().chain([c]).eq(composed.chars());
        let action = if as_typed { Action::Pass } else { replace(&self.word.shown, &composed) };
        self.word.shown = composed;
        action
    }

    /// The word whose screen text is `text`, with keys that recompose to it.
    fn word_showing(&self, text: &str) -> Word {
        if self.vietnamese {
            let keys = to_keys(text);
            if compose(&keys).text == text {
                return Word { keys, shown: text.to_string(), literal: false };
            }
        }
        // English mode, or text the composer would not produce ("receive").
        Word { keys: text.to_string(), shown: text.to_string(), literal: self.vietnamese }
    }

    /// A word ended by a space or by punctuation.
    fn on_boundary(&mut self, delimiter: char) -> Action {
        let punct = delimiter != ' ';
        let word = std::mem::take(&mut self.word);
        self.last = None;
        let shown_len = word.shown.chars().count();
        self.committed += shown_len + 1; // the word and its delimiter
        let untracked = std::mem::take(&mut self.untracked);
        let context = self.context.take();
        let just_undone = self.just_undone.take().is_some_and(|k| k == word.keys);
        let undos = self.undos.get(&word.keys.to_lowercase()).copied().unwrap_or(0);
        // Only a fully known, non-empty word can be restored on Backspace.
        self.prev_word = (!word.keys.is_empty() && !untracked).then(|| word.clone());
        // What the next word sees as context: this one, if we know it, unless
        // punctuation starts a new phrase (as when the word pairs were counted).
        self.context = (!word.keys.is_empty() && !untracked && !punct).then(|| word.shown.clone());

        self.decision = if word.keys.is_empty() {
            Decision::EmptyWord
        } else if !self.corrections_on() || self.observing {
            Decision::Disabled
        } else if untracked {
            Decision::Untracked
        } else if just_undone {
            Decision::JustUndone
        } else if undos >= IGNORE_AFTER_UNDOS {
            Decision::IgnoredAfterUndos(undos)
        } else {
            match self.corrector.correct_after(&word.keys, context.as_deref()) {
                Some(fix) if fix != word.shown => {
                    let action = if punct {
                        match replace(&word.shown, &fix) {
                            Action::Replace { backspaces, text } => Action::ReplaceThenPass { backspaces, text },
                            other => other,
                        }
                    } else {
                        replace(&word.shown, &format!("{fix} "))
                    };
                    self.committed = self.committed + fix.chars().count() - shown_len;
                    self.prev_word = Some(self.word_showing(&fix));
                    self.context = (!punct).then(|| fix.clone());
                    self.last = Some(LastCorrection { original: word, corrected: fix, context, delimiter, delimiter_removed: false });
                    self.decision = Decision::Corrected;
                    return action;
                }
                _ => Decision::NoCandidate,
            }
        };
        Action::Pass
    }

    /// Ctrl+Z right after a correction restores the original word, keeping
    /// the space or punctuation that ended it, and counts the undo. With no
    /// correction pending it is the program's own undo: the text changes
    /// out of our sight, so everything is forgotten and the key passes.
    fn on_undo(&mut self) -> Action {
        let Some(last) = self.last.take() else {
            self.clear();
            return Action::Pass;
        };
        let undone = self.undos.entry(last.original.keys.to_lowercase()).or_default();
        *undone += 1;
        if *undone == IGNORE_AFTER_UNDOS {
            self.learned = Some(last.original.keys.to_lowercase());
        }
        self.just_undone = Some(last.original.keys.clone());
        self.context = last.context.clone();
        if last.delimiter_removed {
            // The fixed word is on screen, resumed, with no delimiter.
            let action = replace(&last.corrected, &last.original.shown);
            self.word = last.original;
            self.prev_word = None;
            return action;
        }
        let d = last.delimiter;
        let (fixed, original) = (last.corrected.chars().count(), last.original.shown.chars().count());
        let action = replace(&format!("{}{d}", last.corrected), &format!("{}{d}", last.original.shown));
        self.committed = (self.committed + original).saturating_sub(fixed);
        // The delimiter stays; a Backspace over it resumes the original word.
        self.context = (d == ' ').then(|| last.original.shown.clone());
        self.prev_word = Some(last.original);
        action
    }

    /// Backspace only ever edits. The first one after a correction deletes
    /// the space or punctuation that ended the word (so a comma can be typed
    /// there instead) and keeps Ctrl+Z available; any further one is editing
    /// the fixed word, so the undo is gone.
    fn on_backspace(&mut self) -> Action {
        match self.last.as_mut() {
            Some(last) if !last.delimiter_removed => last.delimiter_removed = true,
            _ => self.last = None,
        }
        if self.word.shown.pop().is_some() {
            // The app deleted the last character on screen; find keys that
            // type what is left ("việt" -> "việ" = "vieej").
            let shown = std::mem::take(&mut self.word.shown);
            self.word = if self.word.literal {
                Word { keys: shown.clone(), shown, literal: true }
            } else {
                self.word_showing(&shown)
            };
            return Action::Pass;
        }
        // Deleting the boundary space: resume the previous word if we know
        // it, otherwise we are now editing text we never saw.
        self.context = None; // the word before the resumed one is unknown
        match self.prev_word.take() {
            Some(prev) => {
                self.committed = self.committed.saturating_sub(prev.shown.chars().count() + 1);
                self.word = prev;
            }
            None => match self.committed {
                // Before anything we typed: text we never saw.
                0 => self.untracked = true,
                // Deleted the last character we typed: the caret is back
                // where typing began, so the next word starts clean.
                1 => {
                    self.committed = 0;
                    self.untracked = false;
                }
                // Inside an earlier word we can no longer read back.
                n => {
                    self.committed = n - 1;
                    self.untracked = true;
                }
            },
        }
        Action::Pass
    }
}

/// Edits `on_screen` into `wanted`, keeping their common prefix: every
/// injected key is slow (it passes through all system keyboard hooks).
fn replace(on_screen: &str, wanted: &str) -> Action {
    let common = on_screen
        .chars()
        .zip(wanted.chars())
        .take_while(|(a, b)| a == b)
        .count();
    Action::Replace {
        backspaces: on_screen.chars().count() - common,
        text: wanted.chars().skip(common).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DictCorrector;

    fn engine() -> Engine<DictCorrector> {
        Engine::new(DictCorrector::builtin())
    }

    fn type_str(e: &mut Engine<DictCorrector>, s: &str) {
        for c in s.chars() {
            assert_eq!(e.on_key(Key::Char(c)), Action::Pass);
        }
    }

    #[test]
    fn corrects_on_space() {
        let mut e = engine();
        type_str(&mut e, "teh");
        // Keeps the common prefix "t".
        assert_eq!(
            e.on_key(Key::Space),
            Action::Replace { backspaces: 2, text: "he ".into() }
        );
    }

    #[test]
    fn rewrites_only_the_differing_suffix() {
        let mut e = engine();
        type_str(&mut e, "Recieve");
        assert_eq!(
            e.on_key(Key::Space),
            Action::Replace { backspaces: 4, text: "eive ".into() }
        );
        let mut e = engine();
        type_str(&mut e, "dunhf");
        assert_eq!(
            e.on_key(Key::Space),
            Action::Replace { backspaces: 4, text: "ùng ".into() }
        );
    }

    #[test]
    fn correct_word_passes() {
        let mut e = engine();
        type_str(&mut e, "the");
        assert_eq!(e.on_key(Key::Space), Action::Pass);
    }

    #[test]
    fn backspace_edits_buffer() {
        let mut e = engine();
        type_str(&mut e, "tehx");
        e.on_key(Key::Backspace);
        assert_eq!(e.current_word(), "teh");
        assert!(matches!(e.on_key(Key::Space), Action::Replace { .. }));
    }

    #[test]
    fn undo_restores_original() {
        let mut e = engine();
        type_str(&mut e, "dunhf");
        e.on_key(Key::Space);
        // Ctrl+Z restores "dunhf " ("dùng " -> common prefix "d")...
        assert_eq!(
            e.on_key(Key::Undo),
            Action::Replace { backspaces: 4, text: "unhf ".into() }
        );
        assert_eq!(e.current_word(), "");
        // ...and a Backspace over the space resumes it; finishing it again
        // keeps it as typed.
        assert_eq!(e.on_key(Key::Backspace), Action::Pass);
        assert_eq!(e.current_word(), "dunhf");
        assert_eq!(e.on_key(Key::Space), Action::Pass);
        assert_eq!(e.last_decision(), Decision::JustUndone);
    }

    fn type_and_undo(e: &mut Engine<DictCorrector>, word: &str) {
        type_str(e, word);
        assert!(matches!(e.on_key(Key::Space), Action::Replace { .. }));
        assert!(matches!(e.on_key(Key::Undo), Action::Replace { .. }));
        e.on_key(Key::Reset); // user clears the line
    }

    #[test]
    fn single_undo_does_not_disable_word() {
        let mut e = engine();
        type_and_undo(&mut e, "teh");
        // Any casing of the word is still corrected later.
        type_str(&mut e, "TEH");
        assert!(matches!(e.on_key(Key::Space), Action::Replace { .. }));
    }

    #[test]
    fn two_undos_disable_word() {
        let mut e = engine();
        type_and_undo(&mut e, "teh");
        type_and_undo(&mut e, "Teh");
        type_str(&mut e, "teh");
        assert_eq!(e.on_key(Key::Space), Action::Pass);
        assert_eq!(e.last_decision(), Decision::IgnoredAfterUndos(2));
    }

    #[test]
    fn backspace_over_space_resumes_previous_word() {
        let mut e = engine();
        type_str(&mut e, "tehx");
        assert_eq!(e.on_key(Key::Space), Action::Pass);
        e.on_key(Key::Backspace); // deletes the space
        assert_eq!(e.current_word(), "tehx");
        e.on_key(Key::Backspace); // deletes "x"
        assert!(matches!(e.on_key(Key::Space), Action::Replace { .. }));
    }

    #[test]
    fn backspace_into_unseen_text_blocks_correction() {
        let mut e = engine();
        e.on_key(Key::Backspace); // into text typed before we started
        type_str(&mut e, "teh"); // screen may be "xxteh"
        assert_eq!(e.on_key(Key::Space), Action::Pass);
        assert_eq!(e.last_decision(), Decision::Untracked);
        // The next word starts clean.
        type_str(&mut e, "teh");
        assert!(matches!(e.on_key(Key::Space), Action::Replace { .. }));
    }

    #[test]
    fn untracked_word_cannot_be_resumed() {
        let mut e = engine();
        e.on_key(Key::Backspace);
        type_str(&mut e, "abc");
        e.on_key(Key::Space);
        e.on_key(Key::Backspace); // back into the untracked word
        type_str(&mut e, "teh"); // screen may be "...abcteh"
        assert_eq!(e.on_key(Key::Space), Action::Pass);
        assert_eq!(e.last_decision(), Decision::Untracked);
    }

    #[test]
    fn decision_explains_outcome() {
        let mut e = engine();
        type_str(&mut e, "hello");
        e.on_key(Key::Space);
        assert_eq!(e.last_decision(), Decision::NoCandidate);
        type_str(&mut e, "teh");
        e.on_key(Key::Space);
        assert_eq!(e.last_decision(), Decision::Corrected);
        e.on_key(Key::Undo);
        e.on_key(Key::Backspace);
        e.on_key(Key::Space);
        assert_eq!(e.last_decision(), Decision::JustUndone);
    }

    #[test]
    fn punctuation_ends_a_word_and_passes_through() {
        let mut e = engine();
        type_str(&mut e, "teh");
        // The edit is sent first; the comma itself is not part of it.
        assert_eq!(
            e.on_key(Key::Punct(',')),
            Action::ReplaceThenPass { backspaces: 2, text: "he".into() }
        );
        assert_eq!(e.last_decision(), Decision::Corrected);
        // A correct word is left alone, and punctuation alone does nothing.
        type_str(&mut e, "the");
        assert_eq!(e.on_key(Key::Punct(',')), Action::Pass);
        assert_eq!(e.on_key(Key::Punct(',')), Action::Pass);
    }

    #[test]
    fn space_then_backspace_keeps_the_fix_so_a_comma_can_follow() {
        let mut e = engine();
        type_str(&mut e, "teh");
        e.on_key(Key::Space);
        // Backspace removes only the space: no undo, the fixed word resumes.
        assert_eq!(e.on_key(Key::Backspace), Action::Pass);
        assert_eq!(e.current_word(), "the");
        assert_eq!(e.on_key(Key::Punct(',')), Action::Pass);
        // Typing on after the backspace also keeps the fix (and drops the undo).
        let mut e = engine();
        type_str(&mut e, "teh");
        e.on_key(Key::Space);
        e.on_key(Key::Backspace);
        type_str(&mut e, "s");
        assert_eq!(e.on_key(Key::Backspace), Action::Pass); // plain delete of "s"
        assert_eq!(e.current_word(), "the");
    }

    #[test]
    fn undo_after_punctuation_keeps_the_comma() {
        let mut e = engine();
        type_str(&mut e, "teh");
        e.on_key(Key::Punct(',')); // "the," on screen
        assert_eq!(
            e.on_key(Key::Undo),
            Action::Replace { backspaces: 3, text: "eh,".into() } // "he," -> "eh,"
        );
        assert_eq!(e.on_key(Key::Backspace), Action::Pass); // over the comma
        assert_eq!(e.current_word(), "teh");
    }

    #[test]
    fn backspace_alone_never_undoes() {
        // Walking back through a fixed word to edit it must stay plain editing.
        let mut e = engine();
        type_str(&mut e, "teh");
        e.on_key(Key::Space);
        for _ in 0..3 {
            assert_eq!(e.on_key(Key::Backspace), Action::Pass);
        }
        assert_eq!(e.current_word(), "t");
        // Nothing is left to undo: Ctrl+Z is the program's own.
        assert_eq!(e.on_key(Key::Undo), Action::Pass);
    }

    #[test]
    fn undo_with_nothing_pending_passes_and_forgets() {
        let mut e = engine();
        type_str(&mut e, "ab");
        assert_eq!(e.on_key(Key::Undo), Action::Pass);
        assert_eq!(e.current_word(), "");
    }

    #[test]
    fn undo_still_works_after_the_space_was_deleted() {
        let mut e = engine();
        type_str(&mut e, "teh");
        e.on_key(Key::Space);
        assert_eq!(e.on_key(Key::Backspace), Action::Pass); // only the space
        assert_eq!(e.current_word(), "the");
        assert_eq!(
            e.on_key(Key::Undo),
            Action::Replace { backspaces: 2, text: "eh".into() }
        );
        assert_eq!(e.current_word(), "teh");
    }

    #[test]
    fn punctuation_starts_a_new_phrase_for_context() {
        let mut e = Engine::new(AfterIn);
        type_words(&mut e, "in ");
        assert_eq!(e.context(), Some("in"));
        type_words(&mut e, "a");
        e.on_key(Key::Punct(','));
        assert_eq!(e.context(), None);
    }

    #[test]
    fn reset_drops_partial_word() {
        let mut e = engine();
        type_str(&mut e, "te");
        e.on_key(Key::Reset);
        type_str(&mut e, "h");
        assert_eq!(e.on_key(Key::Space), Action::Pass);
    }

    #[test]
    fn typing_after_correction_disables_undo() {
        let mut e = engine();
        type_str(&mut e, "teh");
        e.on_key(Key::Space);
        type_str(&mut e, "c");
        assert_eq!(e.on_key(Key::Backspace), Action::Pass);
    }

    // ---- Vietnamese (Telex) mode ----

    fn vn_engine() -> Engine<DictCorrector> {
        let mut e = engine();
        e.set_vietnamese(true);
        e
    }

    fn press(e: &mut Engine<DictCorrector>, keys: &str) -> Vec<Action> {
        keys.chars().map(|c| e.on_key(Key::Char(c))).collect()
    }

    fn rep(backspaces: usize, text: &str) -> Action {
        Action::Replace { backspaces, text: text.into() }
    }

    #[test]
    fn composes_while_typing() {
        let mut e = vn_engine();
        let pass = Action::Pass;
        // Plain letters pass through; only marks rewrite the screen.
        assert_eq!(
            press(&mut e, "vieetj"),
            [pass.clone(), pass.clone(), pass.clone(), rep(1, "ê"), pass, rep(2, "ệt")]
        );
        assert_eq!(e.current_word(), "việt");
        assert_eq!(e.current_keys(), "vieetj");
    }

    #[test]
    fn repeated_key_cancels_mark_on_screen() {
        let mut e = vn_engine();
        let actions = press(&mut e, "tesst");
        assert_eq!(actions[2], rep(1, "é"));
        assert_eq!(actions[3], rep(1, "es"));
        assert_eq!(actions[4], Action::Pass);
        assert_eq!(e.current_word(), "test");
    }

    #[test]
    fn backspace_then_continue_composing() {
        let mut e = vn_engine();
        press(&mut e, "vieetj");
        assert_eq!(e.on_key(Key::Backspace), Action::Pass);
        assert_eq!(e.current_word(), "việ");
        assert_eq!(e.current_keys(), "vieej");
        assert_eq!(press(&mut e, "n"), [Action::Pass]);
        assert_eq!(e.current_word(), "viện");
    }

    #[test]
    fn backspace_into_literal_word_stays_literal() {
        let mut e = vn_engine();
        press(&mut e, "tesst");
        e.on_key(Key::Backspace);
        assert_eq!(e.current_word(), "tes");
        assert_eq!(press(&mut e, "s"), [Action::Pass]); // no tone this time
        assert_eq!(e.current_word(), "tess");
    }

    #[test]
    fn corrects_and_undoes_in_vietnamese_mode() {
        let mut e = vn_engine();
        assert!(press(&mut e, "dunhf").iter().all(|a| *a == Action::Pass));
        assert_eq!(e.on_key(Key::Space), rep(4, "ùng "));
        assert_eq!(e.on_key(Key::Undo), rep(4, "unhf "));
        assert_eq!(e.on_key(Key::Backspace), Action::Pass);
        assert_eq!(e.current_word(), "dunhf");
    }

    #[test]
    fn resumed_word_can_change_tone() {
        let mut e = vn_engine();
        press(&mut e, "vieetj");
        assert_eq!(e.on_key(Key::Space), Action::Pass);
        e.on_key(Key::Backspace); // back into "việt"
        assert_eq!(e.current_keys(), "vieetj");
        assert_eq!(press(&mut e, "s"), [rep(2, "ết")]);
        assert_eq!(e.current_word(), "viết");
    }

    #[test]
    fn corrections_can_be_turned_off() {
        let mut e = vn_engine();
        e.set_corrections(false, false);
        press(&mut e, "dunhf");
        assert_eq!(e.on_key(Key::Space), Action::Pass);
        assert_eq!(e.last_decision(), Decision::Disabled);
        // Telex composition is unaffected.
        assert_eq!(press(&mut e, "aa"), [Action::Pass, rep(1, "â")]);
    }

    /// Corrector that only fixes "teh" -> "the" after "in", to see the context.
    struct AfterIn;

    impl Corrector for AfterIn {
        fn correct(&self, _word: &str) -> Option<String> {
            None
        }

        fn correct_after(&self, word: &str, prev: Option<&str>) -> Option<String> {
            (word == "teh" && prev == Some("in")).then(|| "the".to_string())
        }
    }

    fn type_words(e: &mut Engine<AfterIn>, text: &str) -> Vec<Action> {
        let mut out = Vec::new();
        for c in text.chars() {
            out.push(e.on_key(if c == ' ' { Key::Space } else { Key::Char(c) }));
        }
        out
    }

    #[test]
    fn corrector_sees_the_previous_word() {
        let mut e = Engine::new(AfterIn);
        let actions = type_words(&mut e, "in teh ");
        assert_eq!(actions.last(), Some(&rep(2, "he ")));
        assert_eq!(e.context(), Some("the")); // the fix, as on screen

        let mut e = Engine::new(AfterIn);
        let actions = type_words(&mut e, "on teh ");
        assert_eq!(actions.last(), Some(&Action::Pass));
        assert_eq!(e.context(), Some("teh"));
    }

    #[test]
    fn context_is_lost_when_the_caret_may_have_moved() {
        let mut e = Engine::new(AfterIn);
        type_words(&mut e, "in ");
        assert_eq!(e.context(), Some("in"));
        e.on_key(Key::Reset); // click, arrows, Enter...
        assert_eq!(e.context(), None);
        assert_eq!(type_words(&mut e, "teh ").last(), Some(&Action::Pass));
    }

    #[test]
    fn undo_restores_the_context() {
        let mut e = Engine::new(AfterIn);
        type_words(&mut e, "in teh ");
        e.on_key(Key::Undo); // "teh " is back on screen
        assert_eq!(e.context(), Some("teh")); // what the next word follows
        e.on_key(Key::Backspace); // over the space: "teh" is being edited again
        assert_eq!(e.current_word(), "teh");
    }

    #[test]
    fn deleting_exactly_what_was_typed_starts_clean() {
        // Type "ab cd " (6 characters), then delete all six: the caret is
        // back where typing began, so the next word is corrected normally.
        let mut e = engine();
        type_str(&mut e, "ab");
        e.on_key(Key::Space);
        type_str(&mut e, "cd");
        e.on_key(Key::Space);
        for _ in 0..6 {
            assert_eq!(e.on_key(Key::Backspace), Action::Pass);
        }
        type_str(&mut e, "teh");
        assert!(matches!(e.on_key(Key::Space), Action::Replace { .. }));
        assert_eq!(e.last_decision(), Decision::Corrected);
    }

    #[test]
    fn deleting_more_than_was_typed_is_untracked() {
        let mut e = engine();
        type_str(&mut e, "ab");
        e.on_key(Key::Space);
        for _ in 0..4 {
            e.on_key(Key::Backspace); // one more than we typed
        }
        type_str(&mut e, "teh");
        assert_eq!(e.on_key(Key::Space), Action::Pass);
        assert_eq!(e.last_decision(), Decision::Untracked);
    }

    #[test]
    fn deleting_into_an_earlier_word_is_untracked() {
        // "ab cd " then four Backspaces: the space, "d", "c", and the space
        // after "ab": the caret now sits at the end of "ab", which was typed
        // earlier and cannot be read back as a word.
        let mut e = engine();
        type_str(&mut e, "ab");
        e.on_key(Key::Space);
        type_str(&mut e, "cd");
        e.on_key(Key::Space);
        for _ in 0..4 {
            e.on_key(Key::Backspace);
        }
        type_str(&mut e, "teh");
        assert_eq!(e.on_key(Key::Space), Action::Pass);
        assert_eq!(e.last_decision(), Decision::Untracked);
    }

    #[test]
    fn select_all_then_backspace_empties_the_field() {
        let mut e = engine();
        e.on_key(Key::SelectAll);
        assert_eq!(e.on_key(Key::Backspace), Action::Pass);
        type_str(&mut e, "teh");
        assert!(matches!(e.on_key(Key::Space), Action::Replace { .. }));
        // Without Select all, a Backspace at the start is untracked.
        let mut e = engine();
        e.on_key(Key::Backspace);
        type_str(&mut e, "teh");
        assert_eq!(e.on_key(Key::Space), Action::Pass);
        // Select all only counts for the very next key.
        let mut e = engine();
        e.on_key(Key::SelectAll);
        e.on_key(Key::Reset);
        e.on_key(Key::Backspace);
        type_str(&mut e, "teh");
        assert_eq!(e.on_key(Key::Space), Action::Pass);
    }

    #[test]
    fn observing_follows_keys_without_acting_on_them() {
        let mut e = vn_engine();
        e.set_observing(true);
        // No Telex: the keys stay as typed, and the word is still known.
        assert!(press(&mut e, "tooi").iter().all(|a| *a == Action::Pass));
        assert_eq!(e.current_word(), "tooi");
        assert_eq!(e.on_key(Key::Space), Action::Pass);
        assert_eq!(e.last_decision(), Decision::Disabled);
    }

    #[test]
    fn a_word_started_while_observing_is_corrected_when_it_ends() {
        let mut e = engine();
        e.set_observing(true);
        type_str(&mut e, "te");
        e.set_observing(false);
        type_str(&mut e, "h");
        assert_eq!(e.on_key(Key::Space), Action::Replace { backspaces: 2, text: "he ".into() });
        // In Vietnamese mode the rest of such a word is not composed.
        let mut e = vn_engine();
        e.set_observing(true);
        press(&mut e, "to");
        e.set_observing(false);
        assert!(press(&mut e, "oi").iter().all(|a| *a == Action::Pass));
        assert_eq!(e.current_word(), "tooi");
    }

    #[test]
    fn a_word_undone_twice_is_learned_once() {
        let mut e = engine();
        type_and_undo(&mut e, "teh");
        assert_eq!(e.take_learned(), None); // once is not enough
        type_and_undo(&mut e, "Teh");
        assert_eq!(e.take_learned().as_deref(), Some("teh"));
        assert_eq!(e.take_learned(), None); // handed out only once
    }

    #[test]
    fn english_mode_shows_keys() {
        let mut e = vn_engine();
        e.set_vietnamese(false);
        assert_eq!(press(&mut e, "aas"), [Action::Pass, Action::Pass, Action::Pass]);
        assert_eq!(e.current_word(), "aas");
    }
}
