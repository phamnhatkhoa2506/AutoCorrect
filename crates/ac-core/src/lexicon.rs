use std::collections::HashMap;

/// Word frequencies, as natural-log occurrences per billion tokens. Words get
/// consecutive ids in file order; [`crate::Bigrams`] refer to them.
pub struct Lexicon {
    index: HashMap<String, usize>,
    words: Vec<(String, f64)>,
}

impl Lexicon {
    /// Parses `word<TAB>per_billion` lines; `#` lines are comments.
    pub fn parse(tsv: &str) -> Self {
        let mut index = HashMap::new();
        let mut words = Vec::new();
        for line in tsv.lines().filter(|l| !l.starts_with('#')) {
            let Some((word, n)) = line.split_once('\t') else { continue };
            let Ok(n) = n.trim().parse::<f64>() else { continue };
            if n > 0.0 && !index.contains_key(word) {
                index.insert(word.to_string(), words.len());
                words.push((word.to_string(), n.ln()));
            }
        }
        Self { index, words }
    }

    pub fn log_freq(&self, word: &str) -> Option<f64> {
        self.index.get(word).map(|&i| self.words[i].1)
    }

    /// Position of `word` in the lexicon (its bigram id).
    pub fn id(&self, word: &str) -> Option<u32> {
        self.index.get(word).map(|&i| i as u32)
    }

    /// All words with their log frequency, indexed by id.
    pub fn words(&self) -> &[(String, f64)] {
        &self.words
    }

    pub fn len(&self) -> usize {
        self.words.len()
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tsv() {
        let lex = Lexicon::parse("# header\nthe\t1000\nbad line\nzero\t0\nof\t500\nthe\t7\n");
        assert_eq!(lex.len(), 2);
        assert!((lex.log_freq("the").unwrap() - 1000f64.ln()).abs() < 1e-9);
        assert_eq!((lex.id("the"), lex.id("of"), lex.id("zero")), (Some(0), Some(1), None));
        assert_eq!(lex.words()[1].0, "of");
    }
}
