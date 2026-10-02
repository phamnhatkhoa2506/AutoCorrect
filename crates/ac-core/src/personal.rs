//! The user's own dictionary: words never to correct, and replacements.
//!
//! Text format, one entry per line (`#` starts a comment):
//!
//! ```text
//! ignore<TAB>word            never correct this word
//! fix<TAB>typed<TAB>instead  always turn "typed" into "instead"
//! ```
//!
//! Words are what is typed (Telex keys while typing Vietnamese), lowercase.

use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Default)]
pub struct Personal {
    ignore: HashSet<String>,
    fixes: HashMap<String, String>,
}

impl Personal {
    pub fn parse(text: &str) -> Self {
        let mut p = Self::default();
        for line in text.lines().filter(|l| !l.trim_start().starts_with('#')) {
            let fields: Vec<&str> = line.split('\t').map(str::trim).collect();
            match fields[..] {
                ["ignore", word, ..] if !word.is_empty() => {
                    p.ignore.insert(word.to_lowercase());
                }
                ["fix", typed, instead, ..] if !typed.is_empty() && !instead.is_empty() => {
                    p.fixes.insert(typed.to_lowercase(), instead.to_string());
                }
                _ => {}
            }
        }
        p
    }

    /// `word` is lowercase.
    pub fn ignores(&self, word: &str) -> bool {
        self.ignore.contains(word)
    }

    /// `word` is lowercase.
    pub fn fix(&self, word: &str) -> Option<&str> {
        self.fixes.get(word).map(String::as_str)
    }

    pub fn add_ignore(&mut self, word: &str) {
        self.ignore.insert(word.to_lowercase());
    }

    pub fn len(&self) -> usize {
        self.ignore.len() + self.fixes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_both_kinds_and_skips_the_rest() {
        let p = Personal::parse(
            "# comment\nignore\tKubeCtl\nfix\tko\tkhông\nfix\tbad\nignore\t\njunk\tx\n  # indented comment\n",
        );
        assert!(p.ignores("kubectl"));
        assert_eq!(p.fix("ko"), Some("không"));
        assert_eq!(p.fix("bad"), None); // no replacement given
        assert_eq!(p.len(), 2);
    }

    #[test]
    fn can_learn_a_word() {
        let mut p = Personal::default();
        assert!(p.is_empty());
        p.add_ignore("Wolff");
        assert!(p.ignores("wolff"));
    }
}
