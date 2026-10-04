//! Builds the interpolated Kneser-Ney tables (`data/vi_kn.bin`, `data/en_kn.bin`)
//! from the same training sentences as the pair and triple tables.
//!
//! With D the absolute discount of each order:
//!   P3(w|ab) = max(c(abw) - D3, 0) / c(ab.) + g3(ab) * P2(w|b)
//!   P2(w|b)  = max(N(.bw) - D2, 0) / N(.b.) + g2(b) * P1(w)
//!   P1(w)    = N(.w) / N(..)
//! where N counts the distinct words that precede, not occurrences.

use std::collections::HashMap;
use std::path::Path;

use ac_core::{Kn, Lexicon};

use crate::bigrams::{sentence_file, tokens, training_sentences, Corpus, Language, Token};

/// Smallest counts kept in the tables (like the pair and triple tables).
const MIN_PAIR: u32 = 6;
const MIN_TRIPLE: u32 = 3;

fn discount(counts: impl Iterator<Item = u32>) -> f64 {
    let (mut n1, mut n2) = (0f64, 0f64);
    for c in counts {
        if c == 1 {
            n1 += 1.0;
        } else if c == 2 {
            n2 += 1.0;
        }
    }
    if n1 + 2.0 * n2 == 0.0 {
        0.75
    } else {
        (n1 / (n1 + 2.0 * n2)).clamp(0.1, 0.95)
    }
}

pub fn build(raw: &Path, corpora: &[Corpus], language: Language, lexicon: &Lexicon) -> std::io::Result<Vec<u8>> {
    let vocab = lexicon.len();
    let mut c3: HashMap<u64, u32> = HashMap::new();
    let mut c2: HashMap<u32, u32> = HashMap::new();
    // The small casual-speech corpus is repeated this many times (AC_SOCIAL_WEIGHT) so that
    // its few thousand sentences count against millions from other registers.
    let social_weight: usize = std::env::var("AC_SOCIAL_WEIGHT").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
    for corpus in corpora {
        let Some(path) = sentence_file(raw, corpus.dir) else { continue };
        let repeat = if corpus.dir == "vie_social_train" { social_weight } else { 1 };
        for sentence in training_sentences(&path)?.into_iter().flat_map(|s| std::iter::repeat(s).take(repeat)) {
            let toks = tokens(&sentence, language, lexicon);
            for w in toks.windows(2) {
                if let (Token::Word(Some(b)), Token::Word(Some(w))) = (w[0], w[1]) {
                    *c2.entry((b << 16) | w).or_insert(0) += 1;
                }
            }
            for w in toks.windows(3) {
                if let (Token::Word(Some(a)), Token::Word(Some(b)), Token::Word(Some(w))) = (w[0], w[1], w[2]) {
                    *c3.entry((u64::from(a) << 32) | (u64::from(b) << 16) | u64::from(w)).or_insert(0) += 1;
                }
            }
        }
    }
    eprintln!("{} triple types, {} pair types", c3.len(), c2.len());

    // Continuation counts: how many distinct words precede each pair / word.
    let mut n2: HashMap<u32, u32> = HashMap::new(); // (b, w) -> distinct a
    let mut ctx3: HashMap<u32, (u32, u32)> = HashMap::new(); // (a, b) -> (total, types)
    for (&key, &c) in &c3 {
        let (a, b, w) = ((key >> 32) as u32, ((key >> 16) & 0xFFFF) as u32, (key & 0xFFFF) as u32);
        *n2.entry((b << 16) | w).or_insert(0) += 1;
        let e = ctx3.entry((a << 16) | b).or_insert((0, 0));
        e.0 += c;
        e.1 += 1;
    }
    let mut n1 = vec![0f64; vocab]; // word -> distinct previous words (pair types)
    for &key in c2.keys() {
        n1[(key & 0xFFFF) as usize] += 1.0;
    }
    let total_n1: f64 = n1.iter().sum::<f64>().max(1.0);
    let p1: Vec<f64> = n1.iter().map(|&n| n.max(0.5) / total_n1).collect();

    let d3 = discount(c3.values().copied());
    let d2 = discount(n2.values().copied());
    eprintln!("discounts: D3 {d3:.3}, D2 {d2:.3}");

    // Continuation totals per previous word b.
    let mut sum2 = vec![0f64; vocab];
    let mut types2 = vec![0f64; vocab];
    for (&key, &n) in &n2 {
        let b = (key >> 16) as usize;
        sum2[b] += f64::from(n);
        types2[b] += 1.0;
    }
    let g2: Vec<f64> = (0..vocab).map(|b| if sum2[b] > 0.0 { d2 * types2[b] / sum2[b] } else { 1.0 }).collect();
    let p2 = |b: u32, w: u32| -> f64 {
        let sum = sum2[b as usize];
        if sum == 0.0 {
            return p1[w as usize];
        }
        let n = f64::from(n2.get(&((b << 16) | w)).copied().unwrap_or(0));
        ((n - d2).max(0.0) / sum) + g2[b as usize] * p1[w as usize]
    };

    let mut pairs: Vec<(u32, u32, f64)> = c2
        .iter()
        .filter(|&(_, &c)| c >= MIN_PAIR)
        .map(|(&key, _)| (key >> 16, key & 0xFFFF, p2(key >> 16, key & 0xFFFF)))
        .collect();
    let mut triples: Vec<(u32, u32, u32, f64)> = Vec::new();
    let mut kept_ctx: HashMap<u32, ()> = HashMap::new();
    for (&key, &c) in &c3 {
        if c < MIN_TRIPLE {
            continue;
        }
        let (a, b, w) = ((key >> 32) as u32, ((key >> 16) & 0xFFFF) as u32, (key & 0xFFFF) as u32);
        let (total, types) = ctx3[&((a << 16) | b)];
        let g3 = d3 * f64::from(types) / f64::from(total);
        let p = (f64::from(c) - d3).max(0.0) / f64::from(total) + g3 * p2(b, w);
        triples.push((a, b, w, p));
        kept_ctx.insert((a << 16) | b, ());
    }
    let mut gammas: Vec<(u32, u32, f64)> = kept_ctx
        .keys()
        .map(|&ctx| {
            let (total, types) = ctx3[&ctx];
            (ctx >> 16, ctx & 0xFFFF, d3 * f64::from(types) / f64::from(total))
        })
        .collect();
    eprintln!("kept {} pairs, {} triples, {} backoff weights", pairs.len(), triples.len(), gammas.len());
    let uni: Vec<(f64, f64)> = (0..vocab).map(|w| (p1[w], g2[w])).collect();
    Ok(Kn::encode(vocab, &uni, &mut pairs, &mut triples, &mut gammas))
}
