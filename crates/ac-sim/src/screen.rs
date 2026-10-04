//! The text field: what the program shows after each key, given what the
//! engine decided (as `inject.rs` sends it).

use ac_core::{Action, Key};

#[derive(Debug, Clone, Default)]
pub struct Screen {
    text: Vec<char>,
}

impl Screen {
    pub fn text(&self) -> String {
        self.text.iter().collect()
    }

    pub fn clear(&mut self) {
        self.text.clear();
    }

    /// Applies the engine's `action` for `key`. `typed` is the character the
    /// key types when it reaches the program (`None` for keys that type nothing).
    pub fn apply(&mut self, key: Key, typed: Option<char>, action: &Action) {
        match action {
            Action::Pass => self.native(key, typed),
            Action::Replace { backspaces, text } => self.edit(*backspaces, text),
            Action::ReplaceThenPass { backspaces, text } => {
                self.edit(*backspaces, text);
                self.native(key, typed);
            }
        }
    }

    /// The key itself, reaching the program.
    fn native(&mut self, key: Key, typed: Option<char>) {
        match key {
            Key::Backspace => {
                self.text.pop();
            }
            Key::Char(c) | Key::Punct(c) | Key::Close(c) => self.text.push(c),
            Key::Space => self.text.push(' '),
            Key::Reset => self.text.extend(typed),
            // The program's own undo and select-all are not modelled: the typist
            // only sends them when the engine takes them.
            Key::Undo | Key::SelectAll => {}
        }
    }

    fn edit(&mut self, backspaces: usize, text: &str) {
        self.text.truncate(self.text.len().saturating_sub(backspaces));
        self.text.extend(text.chars());
    }

    /// The words on screen: runs of letters and digits.
    pub fn words(&self) -> Vec<String> {
        words_of(&self.text())
    }

    /// The word the caret is at the end of ("" after a space or a mark).
    pub fn last_word(&self) -> String {
        let n = self.text.iter().rev().take_while(|c| c.is_alphanumeric()).count();
        self.text[self.text.len() - n..].iter().collect()
    }
}

/// Runs of letters and digits, the same split as [`crate::typist::plan`] uses.
pub fn words_of(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(String::from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_actions_like_the_program() {
        let mut s = Screen::default();
        for c in "teh".chars() {
            s.apply(Key::Char(c), Some(c), &Action::Pass);
        }
        s.apply(Key::Close(')'), Some(')'), &Action::ReplaceThenPass { backspaces: 2, text: "he".into() });
        assert_eq!(s.text(), "the)");
        s.apply(Key::Space, Some(' '), &Action::Replace { backspaces: 0, text: " ".into() });
        s.apply(Key::Reset, Some('('), &Action::Pass);
        s.apply(Key::Char('a'), Some('a'), &Action::Pass);
        assert_eq!(s.text(), "the) (a");
        assert_eq!(s.words(), ["the", "a"]);
        assert_eq!(s.last_word(), "a");
        s.apply(Key::Backspace, None, &Action::Pass);
        assert_eq!(s.last_word(), "");
    }
}
