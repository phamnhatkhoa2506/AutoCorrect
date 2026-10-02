//! Builds the frequency lexicons in `data/` from the raw corpora in `data/raw/`.
//!
//!     cargo run -p ac-data --release -- [--cc-by-only]
//!
//! Sources (see data/ATTRIBUTION.md):
//! - Leipzig Corpora Collection word lists (`data/raw/*/…-words.txt`), CC BY
//! - hermitdave/FrequencyWords `vi_50k.txt` / `en_50k.txt`, CC BY-SA 4.0
//!
//! `--cc-by-only` skips FrequencyWords so the output carries no share-alike terms.

mod bigrams;
mod trigrams;

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use ac_telex::canonical;
use unicode_normalization::UnicodeNormalization;

/// Words seen in a single source must reach this frequency to be kept, which
/// drops one-off typos and junk tokens.
const SINGLE_SOURCE_MIN_PER_BILLION: f64 = 200.0;
/// Floor for every word: below it the list is mostly misspellings ("đưởng").
/// Typos above it remain; the corrector's noisy-channel scoring outweighs
/// them with their frequent neighbours.
const MIN_PER_BILLION: f64 = 100.0;

struct Source {
    name: String,
    counts: HashMap<String, u64>,
}

fn main() -> std::io::Result<()> {
    if let Some(at) = std::env::args().position(|a| a == "--trigrams") {
        // Only the triple tables: `--trigrams [min count]`.
        let min = std::env::args().nth(at + 1).and_then(|v| v.parse().ok()).unwrap_or(3);
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        return write_trigrams(&root, &root.join("data/raw"), min);
    }
    if std::env::args().any(|a| a == "--bigrams") {
        // Only the pair tables.
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        return write_bigrams(&root, &root.join("data/raw"));
    }
    let cc_by_only = std::env::args().any(|a| a == "--cc-by-only");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let raw = root.join("data/raw");

    let mut vi_sources: Vec<Source> = leipzig_word_files(&raw)?
        .into_iter()
        .map(|path| read_source(&path, Format::Leipzig))
        .collect::<Result<_, _>>()?;
    let mut en_sources = Vec::new();
    if !cc_by_only {
        vi_sources.push(read_source(&raw.join("vi_50k.txt"), Format::FrequencyWords)?);
        en_sources.push(read_source(&raw.join("en_50k.txt"), Format::FrequencyWords)?);
    }

    let vi = combine(&vi_sources, canonical);
    let en = combine(&en_sources, |w| {
        let w = w.to_lowercase();
        let ascii = w.chars().all(|c| c.is_ascii_lowercase());
        (ascii && (w.len() > 1 || w == "a" || w == "i")).then_some(w)
    });

    let license = if cc_by_only { "CC BY 4.0" } else { "CC BY-SA 4.0" };
    write_lexicon(&root.join("data/vi_syllables.tsv"), &vi, &vi_sources, license)?;
    if !en_sources.is_empty() {
        write_lexicon(&root.join("data/en_words.tsv"), &en, &en_sources, license)?;
    }
    write_bigrams(&root, &raw)
}

/// Word-pair tables for the corpora that are present in `data/raw`.
fn write_bigrams(root: &Path, raw: &Path) -> std::io::Result<()> {
    use ac_core::Lexicon;
    use bigrams::{Corpus, Language};

    let jobs = [
        ("vi", Language::Vietnamese, vec![Corpus { dir: "vie_news_2022_1M" }, Corpus { dir: "vie-vn_web_2015_1M" }, Corpus { dir: "vie_subtitles" }]),
        ("en", Language::English, vec![Corpus { dir: "eng_news_2023_1M" }]),
    ];
    for (code, language, corpora) in jobs {
        let tsv = root.join(match code { "vi" => "data/vi_syllables.tsv", _ => "data/en_words.tsv" });
        let lexicon = Lexicon::parse(&fs::read_to_string(tsv)?);
        eprintln!("{code}: counting word pairs...");
        let table = bigrams::build(raw, &corpora, language, &lexicon)?;
        let out = root.join(format!("data/{code}_bigrams.bin"));
        fs::write(&out, &table)?;
        eprintln!("wrote {} ({:.1} MB)", out.display(), table.len() as f64 / 1e6);
    }
    Ok(())
}

/// Word-triple tables for the corpora that are present in `data/raw`.
fn write_trigrams(root: &Path, raw: &Path, min_count: u32) -> std::io::Result<()> {
    use ac_core::Lexicon;
    use bigrams::{Corpus, Language};

    let jobs = [
        ("vi", Language::Vietnamese, vec![Corpus { dir: "vie_news_2022_1M" }, Corpus { dir: "vie-vn_web_2015_1M" }, Corpus { dir: "vie_subtitles" }]),
        ("en", Language::English, vec![Corpus { dir: "eng_news_2023_1M" }]),
    ];
    for (code, language, corpora) in jobs {
        let tsv = root.join(match code {
            "vi" => "data/vi_syllables.tsv",
            _ => "data/en_words.tsv",
        });
        let lexicon = Lexicon::parse(&fs::read_to_string(tsv)?);
        eprintln!("{code}: counting word triples...");
        let table = trigrams::build(raw, &corpora, language, &lexicon, min_count)?;
        let out = root.join(format!("data/{code}_trigrams.bin"));
        fs::write(&out, &table)?;
        eprintln!("wrote {} ({:.1} MB)", out.display(), table.len() as f64 / 1e6);
    }
    Ok(())
}

fn leipzig_word_files(raw: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for dir in fs::read_dir(raw)? {
        let dir = dir?.path();
        if !dir.is_dir() {
            continue;
        }
        for f in fs::read_dir(&dir)? {
            let f = f?.path();
            if f.to_string_lossy().ends_with("-words.txt") {
                files.push(f);
            }
        }
    }
    files.sort();
    Ok(files)
}

enum Format {
    /// `id \t word \t count`
    Leipzig,
    /// `word count`
    FrequencyWords,
}

fn read_source(path: &Path, format: Format) -> std::io::Result<Source> {
    let text = fs::read_to_string(path)?;
    let mut counts = HashMap::new();
    for line in text.lines() {
        let fields: Vec<&str> = match format {
            Format::Leipzig => line.split('\t').skip(1).collect(),
            Format::FrequencyWords => line.split_whitespace().collect(),
        };
        let [word, count] = fields[..] else { continue };
        let Ok(count) = count.trim().parse::<u64>() else { continue };
        *counts.entry(word.nfc().collect::<String>()).or_insert(0) += count;
    }
    let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
    eprintln!("read {name}: {} entries", counts.len());
    Ok(Source { name, counts })
}

/// Averages each source's relative frequencies (so a big corpus does not
/// drown a small one) and returns counts per billion tokens.
fn combine(sources: &[Source], normalize: impl Fn(&str) -> Option<String>) -> Vec<(String, f64)> {
    let mut sum: HashMap<String, (f64, usize)> = HashMap::new();
    for source in sources {
        let mut kept: HashMap<String, u64> = HashMap::new();
        for (word, &count) in &source.counts {
            if let Some(key) = normalize(word) {
                *kept.entry(key).or_insert(0) += count;
            }
        }
        let total: u64 = kept.values().sum();
        for (word, count) in kept {
            let e = sum.entry(word).or_insert((0.0, 0));
            e.0 += count as f64 / total as f64;
            e.1 += 1;
        }
    }
    let n = sources.len().max(1) as f64;
    let mut out: Vec<(String, f64)> = sum
        .into_iter()
        .map(|(w, (p, seen))| (w, p / n * 1e9, seen))
        .filter(|&(_, per_billion, seen)| {
            per_billion >= MIN_PER_BILLION && (seen >= 2 || per_billion >= SINGLE_SOURCE_MIN_PER_BILLION)
        })
        .map(|(w, per_billion, _)| (w, per_billion))
        .collect();
    out.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    out
}

fn write_lexicon(path: &Path, words: &[(String, f64)], sources: &[Source], license: &str) -> std::io::Result<()> {
    let mut f = std::io::BufWriter::new(fs::File::create(path)?);
    writeln!(f, "# word<TAB>occurrences per billion tokens, averaged over sources")?;
    writeln!(f, "# license: {license}; sources and attribution: data/ATTRIBUTION.md")?;
    for s in sources {
        writeln!(f, "# source: {}", s.name)?;
    }
    for (word, per_billion) in words {
        writeln!(f, "{word}\t{}", per_billion.round() as u64)?;
    }
    eprintln!("wrote {} ({} words)", path.display(), words.len());
    Ok(())
}
