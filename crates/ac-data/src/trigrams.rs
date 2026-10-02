//! Counts word triples in the Leipzig sentence files and writes the compact
//! tables `ac-core` can embed (`data/vi_trigrams.bin`, `data/en_trigrams.bin`).
//! Like the pair tables, the held-out sentences are left out.

use std::collections::HashMap;
use std::path::Path;

use ac_core::{Lexicon, Trigrams};

use crate::bigrams::{sentence_file, tokens, training_sentences, Corpus, Language, Token};

/// Builds the table for one language, keeping triples seen at least `min_count` times.
pub fn build(raw: &Path, corpora: &[Corpus], language: Language, lexicon: &Lexicon, min_count: u32) -> std::io::Result<Vec<u8>> {
    let mut triples: HashMap<u64, u32> = HashMap::new();
    let mut totals: HashMap<u32, u32> = HashMap::new();
    let mut sentences = 0usize;
    for corpus in corpora {
        let Some(path) = sentence_file(raw, corpus.dir) else {
            eprintln!("skipping {}: no sentence file", corpus.dir);
            continue;
        };
        for sentence in training_sentences(&path)? {
            sentences += 1;
            let tokens = tokens(&sentence, language, lexicon);
            for w in tokens.windows(3) {
                let (Token::Word(Some(a)), Token::Word(Some(b)), Token::Word(next)) = (w[0], w[1], w[2]) else { continue };
                *totals.entry((a << 16) | b).or_insert(0) += 1; // every successor counts, known or not
                if let Some(next) = next {
                    *triples.entry((u64::from(a) << 32) | (u64::from(b) << 16) | u64::from(next)).or_insert(0) += 1;
                }
            }
        }
    }
    let seen = triples.len();
    let kept: Vec<(u32, u32, u32, f64)> = triples
        .into_iter()
        .filter(|&(_, n)| n >= min_count)
        .map(|(key, n)| {
            let (a, b, next) = ((key >> 32) as u32, ((key >> 16) & 0xFFFF) as u32, (key & 0xFFFF) as u32);
            (a, b, next, f64::from(n) / f64::from(totals[&((a << 16) | b)]))
        })
        .collect();
    eprintln!("{sentences} sentences -> {seen} triples, {} seen at least {min_count} times", kept.len());
    Ok(Trigrams::encode(lexicon.len(), &kept))
}
