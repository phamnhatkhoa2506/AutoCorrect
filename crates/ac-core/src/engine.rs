use std::collections::HashMap;

use crate::Corrector;

/// A key event, already decoded by the platform layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A character that belongs to a word.
    Char(char),
    Backspace,
    /// Word boundary that triggers correction (Space only in Phase 0).
    Space,
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
}

struct LastCorrection {
    original: String,
    corrected: String,
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
}

pub struct Engine<C: Corrector> {
    corrector: C,
    word: String,
    /// The previous word as it is on screen, followed by one space. Lets a
    /// Backspace over that space resume editing the word.
    prev_word: Option<String>,
    /// The buffer is not known to hold the whole on-screen word.
    untracked: bool,
    last: Option<LastCorrection>,
    /// Word just restored by an undo: finishing it unchanged keeps it as is.
    just_undone: Option<String>,
    /// Undo count per lowercased word, this session.
    undos: HashMap<String, u8>,
    decision: Decision,
}

impl<C: Corrector> Engine<C> {
    pub fn new(corrector: C) -> Self {
        Self {
            corrector,
            word: String::new(),
            prev_word: None,
            untracked: false,
            last: None,
            just_undone: None,
            undos: HashMap::new(),
            decision: Decision::EmptyWord,
        }
    }

    pub fn current_word(&self) -> &str {
        &self.word
    }

    pub fn on_key(&mut self, key: Key) -> Action {
        match key {
            Key::Char(c) => {
                self.word.push(c);
                self.last = None;
                Action::Pass
            }
            Key::Backspace => self.on_backspace(),
            Key::Space => self.on_space(),
            Key::Reset => {
                self.word.clear();
                self.prev_word = None;
                self.untracked = false;
                self.last = None;
                self.just_undone = None;
                Action::Pass
            }
        }
    }

    /// Why the last Space did or did not correct the word (for diagnostics).
    pub fn last_decision(&self) -> Decision {
        self.decision
    }

    fn on_space(&mut self) -> Action {
        let word = std::mem::take(&mut self.word);
        self.last = None;
        let untracked = std::mem::take(&mut self.untracked);
        let just_undone = self.just_undone.take().is_some_and(|w| w == word);
        let undos = self.undos.get(&word.to_lowercase()).copied().unwrap_or(0);
        // Only a fully known, non-empty word can be restored on Backspace.
        self.prev_word = (!word.is_empty() && !untracked).then(|| word.clone());

        self.decision = if word.is_empty() {
            Decision::EmptyWord
        } else if untracked {
            Decision::Untracked
        } else if just_undone {
            Decision::JustUndone
        } else if undos >= IGNORE_AFTER_UNDOS {
            Decision::IgnoredAfterUndos(undos)
        } else {
            match self.corrector.correct(&word) {
                Some(fix) if fix != word => {
                    let action = replace(&word, &format!("{fix} "));
                    self.prev_word = Some(fix.clone());
                    self.last = Some(LastCorrection { original: word, corrected: fix });
                    self.decision = Decision::Corrected;
                    return action;
                }
                _ => Decision::NoCandidate,
            }
        };
        Action::Pass
    }

    /// Backspace right after a correction undoes it: restores the original
    /// word (without the trailing space) and counts the undo.
    fn on_backspace(&mut self) -> Action {
        if let Some(last) = self.last.take() {
            *self.undos.entry(last.original.to_lowercase()).or_default() += 1;
            self.word = last.original.clone();
            self.prev_word = None;
            self.just_undone = Some(last.original.clone());
            return replace(&format!("{} ", last.corrected), &last.original);
        }
        if self.word.pop().is_none() {
            // Deleting the boundary space: resume the previous word if we
            // know it, otherwise we are now editing text we never saw.
            match self.prev_word.take() {
                Some(prev) => self.word = prev,
                None => self.untracked = true,
            }
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
        // Screen shows "dùng " -> back to "dunhf" (common prefix "d").
        assert_eq!(
            e.on_key(Key::Backspace),
            Action::Replace { backspaces: 4, text: "unhf".into() }
        );
        assert_eq!(e.current_word(), "dunhf");
        // Re-finishing the restored word keeps it.
        assert_eq!(e.on_key(Key::Space), Action::Pass);
    }

    fn type_and_undo(e: &mut Engine<DictCorrector>, word: &str) {
        type_str(e, word);
        assert!(matches!(e.on_key(Key::Space), Action::Replace { .. }));
        assert!(matches!(e.on_key(Key::Backspace), Action::Replace { .. }));
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
        e.on_key(Key::Backspace);
        e.on_key(Key::Space);
        assert_eq!(e.last_decision(), Decision::JustUndone);
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
}
