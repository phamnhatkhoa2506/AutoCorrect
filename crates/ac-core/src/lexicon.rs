use std::collections::HashMap;

/// Word frequencies, as natural-log occurrences per billion tokens.
pub struct Lexicon {
    log_freq: HashMap<String, f64>,
}

impl Lexicon {
    /// Parses `word<TAB>per_billion` lines; `#` lines are comments.
    pub fn parse(tsv: &str) -> Self {
        let log_freq = tsv
            .lines()
            .filter(|l| !l.starts_with('#'))
            .filter_map(|l| {
                let (word, n) = l.split_once('\t')?;
                let n: f64 = n.trim().parse().ok()?;
                (n > 0.0).then(|| (word.to_string(), n.ln()))
            })
            .collect();
        Self { log_freq }
    }

    pub fn log_freq(&self, word: &str) -> Option<f64> {
        self.log_freq.get(word).copied()
    }

    pub fn len(&self) -> usize {
        self.log_freq.len()
    }

    pub fn is_empty(&self) -> bool {
        self.log_freq.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tsv() {
        let lex = Lexicon::parse("# header\nthe\t1000\nbad line\nzero\t0\n");
        assert_eq!(lex.len(), 1);
        assert!((lex.log_freq("the").unwrap() - 1000f64.ln()).abs() < 1e-9);
    }
}
