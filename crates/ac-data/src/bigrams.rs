//! Counts word pairs in the Leipzig sentence files and writes the compact
//! tables `ac-core` embeds (`data/vi_bigrams.bin`, `data/en_bigrams.bin`).
//!
//! The last [`HELD_OUT_SENTENCES`] lines of every file are skipped: the
//! benchmark (`ac-bench`) evaluates on exactly those.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use ac_core::{Bigrams, Lexicon, HELD_OUT_SENTENCES};
use ac_telex::canonical;
use unicode_normalization::UnicodeNormalization;

/// Pairs seen fewer times are dropped (they are mostly noise, and the table
/// is embedded in the program).
pub const MIN_PAIR_COUNT: u32 = 6;

pub struct Corpus {
    /// Folder name under `data/raw`.
    pub dir: &'static str,
}

/// How a token becomes a lexicon word.
#[derive(Clone, Copy)]
pub enum Language {
    Vietnamese,
    English,
}

impl Language {
    fn word(self, token: &str) -> Option<String> {
        match self {
            Language::Vietnamese => canonical(&token.nfc().collect::<String>()),
            Language::English => {
                let lower = token.to_lowercase();
                lower.chars().all(|c| c.is_ascii_lowercase()).then_some(lower)
            }
        }
    }
}

/// `*-sentences.txt` inside a corpus folder.
pub fn sentence_file(raw: &Path, dir: &str) -> Option<PathBuf> {
    fs::read_dir(raw.join(dir)).ok()?.flatten().map(|e| e.path()).find(|p| p.to_string_lossy().ends_with("-sentences.txt"))
}

/// The training sentences of a corpus (everything but the held-out tail).
pub fn training_sentences(path: &Path) -> std::io::Result<Vec<String>> {
    let lines: Vec<String> = BufReader::new(File::open(path)?).lines().collect::<Result<_, _>>()?;
    let keep = lines.len().saturating_sub(HELD_OUT_SENTENCES);
    Ok(lines.into_iter().take(keep).map(|l| l.split('\t').nth(1).unwrap_or("").to_string()).collect())
}

/// A word of a sentence as its lexicon id (`None`: outside the lexicon), or
/// a break in the chain (digits, punctuation) after which a new phrase starts.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Token {
    Word(Option<u32>),
    Break,
}

pub fn tokens(sentence: &str, language: Language, lexicon: &Lexicon) -> Vec<Token> {
    let mut out = Vec::new();
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut Vec<Token>| {
        if !word.is_empty() {
            let id = language.word(word).and_then(|w| lexicon.id(&w));
            out.push(Token::Word(id));
            word.clear();
        }
    };
    for c in sentence.chars() {
        if c.is_alphabetic() {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            if !c.is_whitespace() {
                out.push(Token::Break);
            }
        }
    }
    flush(&mut word, &mut out);
    out
}

/// Builds the table for one language from the given corpora.
pub fn build(raw: &Path, corpora: &[Corpus], language: Language, lexicon: &Lexicon) -> std::io::Result<Vec<u8>> {
    let mut pairs: HashMap<u64, u32> = HashMap::new();
    let mut totals = vec![0u32; lexicon.len()];
    let mut sentences = 0usize;
    for corpus in corpora {
        let Some(path) = sentence_file(raw, corpus.dir) else {
            eprintln!("skipping {}: no sentence file", corpus.dir);
            continue;
        };
        for sentence in training_sentences(&path)? {
            sentences += 1;
            let tokens = tokens(&sentence, language, lexicon);
            for w in tokens.windows(2) {
                let (Token::Word(Some(prev)), Token::Word(next)) = (w[0], w[1]) else { continue };
                totals[prev as usize] += 1; // every successor counts, known or not
                if let Some(next) = next {
                    *pairs.entry((u64::from(prev) << 32) | u64::from(next)).or_insert(0) += 1;
                }
            }
        }
    }
    let kept: Vec<(u32, u32, f64)> = pairs
        .into_iter()
        .filter(|&(_, n)| n >= MIN_PAIR_COUNT)
        .map(|(key, n)| {
            let (prev, next) = ((key >> 32) as u32, key as u32);
            (prev, next, f64::from(n) / f64::from(totals[prev as usize]))
        })
        .collect();
    eprintln!("{sentences} sentences -> {} word pairs seen at least {MIN_PAIR_COUNT} times", kept.len());
    Ok(Bigrams::encode(lexicon.len(), &kept))
}
