use std::collections::HashMap;

/// Proposes a replacement for a finished word, or `None` to leave it alone.
pub trait Corrector {
    /// `word` is the raw keys typed (Telex keys in Vietnamese mode).
    fn correct(&self, word: &str) -> Option<String>;

    /// Like [`Corrector::correct`], knowing the word typed just before.
    fn correct_after(&self, word: &str, _prev: Option<&str>) -> Option<String> {
        self.correct(word)
    }

    /// Which languages corrections may produce (input mode, per-app policy).
    fn set_languages(&mut self, _vietnamese: bool, _english: bool) {}
}

/// Phase 0 corrector: exact lookup in a typo -> fix table, preserving case.
pub struct DictCorrector {
    table: HashMap<String, String>,
}

impl DictCorrector {
    pub fn new<'a>(pairs: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        let table = pairs
            .into_iter()
            .map(|(typo, fix)| (typo.to_lowercase(), fix.to_string()))
            .collect();
        Self { table }
    }

    /// Small built-in list used by the spike to exercise the pipeline.
    pub fn builtin() -> Self {
        Self::new([
            // English
            ("teh", "the"),
            ("hlelo", "hello"),
            ("adn", "and"),
            ("taht", "that"),
            ("wiht", "with"),
            ("jsut", "just"),
            ("waht", "what"),
            ("thier", "their"),
            ("recieve", "receive"),
            ("becuase", "because"),
            ("definately", "definitely"),
            ("seperate", "separate"),
            ("occured", "occurred"),
            ("untill", "until"),
            // Vietnamese raw Telex (IME off or mistyped)
            ("dunhf", "dùng"),
            ("dungf", "dùng"),
            ("nhnah", "nhanh"),
            ("tieengs", "tiếng"),
            ("vieetj", "việt"),
            ("dduowcj", "được"),
            ("khoong", "không"),
        ])
    }
}

impl Corrector for DictCorrector {
    fn correct(&self, word: &str) -> Option<String> {
        let fix = self.table.get(&word.to_lowercase())?;
        Some(match_case(word, fix))
    }
}

/// Applies the casing pattern of `original` (lower / Capitalized / UPPER) to `fix`.
pub(crate) fn match_case(original: &str, fix: &str) -> String {
    let mut letters = original.chars().filter(|c| c.is_alphabetic());
    let first_upper = letters.next().is_some_and(char::is_uppercase);
    let rest: Vec<char> = letters.collect();

    if first_upper && !rest.is_empty() && rest.iter().all(|c| c.is_uppercase()) {
        fix.to_uppercase()
    } else if first_upper {
        let mut out = fix.chars();
        match out.next() {
            Some(f) => f.to_uppercase().chain(out).collect(),
            None => String::new(),
        }
    } else {
        fix.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corrects_known_typo() {
        let c = DictCorrector::builtin();
        assert_eq!(c.correct("teh").as_deref(), Some("the"));
        assert_eq!(c.correct("dunhf").as_deref(), Some("dùng"));
    }

    #[test]
    fn leaves_unknown_word() {
        assert_eq!(DictCorrector::builtin().correct("hello"), None);
    }

    #[test]
    fn preserves_case() {
        let c = DictCorrector::builtin();
        assert_eq!(c.correct("Teh").as_deref(), Some("The"));
        assert_eq!(c.correct("TEH").as_deref(), Some("THE"));
        assert_eq!(c.correct("Dunhf").as_deref(), Some("Dùng"));
    }
}
