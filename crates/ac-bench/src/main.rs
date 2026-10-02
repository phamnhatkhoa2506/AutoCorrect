//! Measures the corrector on held-out sentences with artificial typos.
//!
//!     cargo run -p ac-bench --release -- [--sentences N] [--rate R] [--seed S] [--show N]
//!         [--floor F --margin M --ambiguity A --known K --rare P --weight W --language L]
//!         [--far-floor F --far-ambiguity A --no-list]
//!         [--bare R --restore-margin M --restore-ambiguity A --restore-english E]
//!     cargo run -p ac-bench --release -- --journal [file]   (report on your own journal)
//!
//! Every word of real sentences (never seen in training: the last
//! `HELD_OUT_SENTENCES` lines of each corpus) is typed either correctly or,
//! with probability R, with a typo made by a generator that is independent
//! of the corrector's slip model. Reported per language, with and without
//! the previous word as context:
//!
//! * false corrections: correct words the corrector changed (per 1000)
//! * typos fixed right / fixed wrong / missed, for one and for two slips
//!
//! `--show N` prints N examples of false and of wrong corrections (with context).
//!
//! The typo generator is a stand-in for real mistakes: use the numbers to
//! compare versions, not as a promise of real-world accuracy.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use ac_core::{Bigrams, Corrector, Lexicon, SmartCorrector, Tuning, HELD_OUT_SENTENCES};
use ac_telex::syllable::split_tone;
use ac_telex::{canonical, to_keys};
use unicode_normalization::UnicodeNormalization;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Language {
    Vietnamese,
    English,
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn chance(&mut self, p: f64) -> bool {
        (self.next() % 1_000_000) as f64 / 1_000_000.0 < p
    }
}

/// One word of a sentence: its text, the keys that type it, and the word
/// typed just before it (None at a phrase start).
struct Word {
    text: String,
    keys: String,
    prev: Option<String>,
    /// The words before this one, up to three, oldest first (`prev` is the last).
    history: Vec<String>,
    /// Written with a capital first letter (names, sentence starts).
    capital: bool,
}

/// A Vietnamese word typed without any marks: "không" -> "khong".
fn bare(text: &str) -> String {
    text.chars()
        .map(|c| match split_tone(c).0 {
            'ă' | 'â' => 'a',
            'ê' => 'e',
            'ô' | 'ơ' => 'o',
            'ư' => 'u',
            'đ' => 'd',
            other => other,
        })
        .collect()
}

/// The keys as the user types them: capital first letter when the word has one.
fn typed_as(keys: &str, capital: bool) -> String {
    let mut chars = keys.chars();
    match chars.next() {
        Some(first) if capital => first.to_uppercase().chain(chars).collect(),
        _ => keys.to_string(),
    }
}

fn words(sentence: &str, language: Language) -> Vec<Word> {
    let mut out: Vec<Word> = Vec::new();
    let mut prev: Option<String> = None;
    let mut run = String::new();
    let mut history: Vec<String> = Vec::new();
    let finish = |run: &mut String, prev: &mut Option<String>, history: &mut Vec<String>, out: &mut Vec<Word>| {
        if run.is_empty() {
            return;
        }
        let token: String = run.nfc().collect();
        run.clear();
        let capital = token.chars().next().is_some_and(char::is_uppercase);
        let text = match language {
            Language::Vietnamese => canonical(&token),
            Language::English => Some(token.to_lowercase()).filter(|t| t.chars().all(|c| c.is_ascii_lowercase())),
        };
        match text {
            Some(text) => {
                let keys = if language == Language::Vietnamese { to_keys(&text) } else { text.clone() };
                out.push(Word { text: text.clone(), keys, prev: prev.take(), history: history.clone(), capital });
                history.push(text.clone());
                if history.len() > 3 {
                    history.remove(0);
                }
                *prev = Some(text);
            }
            None => {
                *prev = None;
                history.clear();
            }
        }
    };
    for c in sentence.chars() {
        if c.is_alphabetic() {
            run.push(c);
        } else {
            finish(&mut run, &mut prev, &mut history, &mut out);
            if !c.is_whitespace() {
                prev = None;
                history.clear();
            }
        }
    }
    finish(&mut run, &mut prev, &mut history, &mut out);
    out
}

/// QWERTY neighbours, written independently of the corrector's table: same
/// row left/right and the keys diagonally above and below.
fn neighbours(c: u8) -> Vec<u8> {
    const ROWS: [&[u8]; 3] = [b"qwertyuiop", b"asdfghjkl", b"zxcvbnm"];
    let mut out = Vec::new();
    for (r, row) in ROWS.iter().enumerate() {
        let Some(i) = row.iter().position(|&k| k == c) else { continue };
        if i > 0 {
            out.push(row[i - 1]);
        }
        if i + 1 < row.len() {
            out.push(row[i + 1]);
        }
        for other in [r.wrapping_sub(1), r + 1] {
            if let Some(row) = ROWS.get(other) {
                out.extend(row.get(i).copied());
                out.extend(row.get(i + 1).copied());
            }
        }
    }
    out
}

/// One typo of a kind that does not depend on the language.
fn slip(keys: &[u8], rng: &mut Rng) -> Vec<u8> {
    let mut k = keys.to_vec();
    let i = rng.below(k.len());
    match rng.below(100) {
        0..=34 if k.len() > 1 => {
            let i = i.min(k.len() - 2);
            k.swap(i, i + 1);
        }
        35..=59 => {
            let near = neighbours(k[i]);
            if !near.is_empty() {
                k[i] = near[rng.below(near.len())];
            }
        }
        60..=79 if k.len() > 2 => {
            k.remove(i);
        }
        80..=92 => k.insert(i, k[i]),
        _ => {
            let near = neighbours(k[i]);
            if !near.is_empty() {
                k.insert(i, near[rng.below(near.len())]);
            }
        }
    }
    k
}

/// Telex-specific slips: a wrong or missing tone key, a half-typed mark.
fn telex_slip(keys: &[u8], rng: &mut Rng) -> Vec<u8> {
    const TONES: &[u8] = b"sfrxj";
    let mut k = keys.to_vec();
    if let Some(&last) = k.last().filter(|c| TONES.contains(c)) {
        match rng.below(3) {
            0 => {
                let others: Vec<u8> = TONES.iter().copied().filter(|&t| t != last).collect();
                let n = k.len();
                k[n - 1] = others[rng.below(others.len())];
            }
            1 => {
                k.pop();
            }
            _ => {}
        }
    }
    // A doubled mark key (aa, ee, oo, dd): drop one half.
    let pairs: Vec<usize> = (1..k.len()).filter(|&i| k[i] == k[i - 1] && b"aeod".contains(&k[i])).collect();
    if k == keys && !pairs.is_empty() {
        k.remove(pairs[rng.below(pairs.len())]);
    }
    k
}

fn typo(keys: &str, language: Language, double: bool, rng: &mut Rng) -> Option<String> {
    let original = keys.as_bytes();
    for _ in 0..8 {
        let mut k = original.to_vec();
        for _ in 0..if double { 2 } else { 1 } {
            k = if language == Language::Vietnamese && rng.chance(0.3) { telex_slip(&k, rng) } else { slip(&k, rng) };
        }
        if k != original && k.len() >= 3 {
            return String::from_utf8(k).ok();
        }
    }
    None
}

/// Typed keys differ from the right ones by more than one slip.
fn is_two_slips(right: &str, typed: &str) -> bool {
    let (a, b) = (right.as_bytes(), typed.as_bytes());
    if a.len().abs_diff(b.len()) > 1 {
        return true;
    }
    if a.len() != b.len() {
        return false; // one key more or less
    }
    let diff: Vec<usize> = (0..a.len()).filter(|&i| a[i] != b[i]).collect();
    let one_swap = diff.len() == 2 && diff[1] == diff[0] + 1 && a[diff[0]] == b[diff[1]] && a[diff[1]] == b[diff[0]];
    diff.len() > 1 && !one_swap
}

#[derive(Default, Clone, Copy)]
struct Typos {
    n: u32,
    right: u32,
    wrong: u32,
    /// Missed because the typo is itself a real word (or valid syllable):
    /// nothing to fix without understanding the sentence.
    missed_real: u32,
}

impl Typos {
    fn row(&self) -> String {
        let pct = |part: u32| if self.n == 0 { 0.0 } else { 100.0 * f64::from(part) / f64::from(self.n) };
        format!(
            "{:>6} typos: {:>5.1}% right {:>5.1}% wrong {:>5.1}% missed ({:.1}% of all are real words)",
            self.n,
            pct(self.right),
            pct(self.wrong),
            pct(self.n - self.right - self.wrong),
            pct(self.missed_real)
        )
    }
}

struct Case<'a> {
    word: &'a Word,
    /// The keys actually typed when this word got a typo.
    typed: Option<String>,
    /// `typed` is the word without its marks, not a slip.
    bare: bool,
}

/// (correct words, false corrections), typos with one slip, with two, and
/// words typed without their marks.
fn run(corrector: &SmartCorrector, cases: &[Case], show: usize) -> ((u32, u32), Typos, Typos, Typos) {
    let mut shown = [0usize; 2];
    let (mut clean, mut false_fix) = (0, 0);
    let (mut one, mut two, mut stripped) = (Typos::default(), Typos::default(), Typos::default());
    for case in cases {
        let prev = case.word.prev.as_deref();
        let history: Vec<&str> = case.word.history.iter().map(String::as_str).collect();
        match &case.typed {
            None => {
                clean += 1;
                let keys = typed_as(&case.word.keys, case.word.capital);
                if let Some(fix) = corrector.correct_in(&keys, &history).filter(|fix| fix.to_lowercase() != case.word.text) {
                    false_fix += 1;
                    if shown[0] < show {
                        shown[0] += 1;
                        println!("    false: {:?} after {:?} -> {fix:?}", case.word.text, prev);
                    }
                }
            }
            Some(typed) => {
                let tally = if case.bare {
                    &mut stripped
                } else if is_two_slips(&case.word.keys, typed) {
                    &mut two
                } else {
                    &mut one
                };
                tally.n += 1;
                let typed_keys = typed_as(typed, case.word.capital);
                match corrector.correct_in(&typed_keys, &history) {
                    Some(fix) if fix.to_lowercase() == case.word.text => tally.right += 1,
                    Some(fix) => {
                        tally.wrong += 1;
                        if shown[1] < show {
                            shown[1] += 1;
                            println!("    wrong: {:?} typed {typed:?} after {:?} -> {fix:?}", case.word.text, prev);
                        }
                    }
                    None => {
                        if corrector.rank(&typed_keys, prev).is_some_and(|r| r.typed > f64::NEG_INFINITY) {
                            tally.missed_real += 1;
                        }
                    }
                }
            }
        }
    }
    ((clean, false_fix), one, two, stripped)
}

fn held_out(raw: &Path, dir: &str, limit: usize) -> Vec<String> {
    let Some(path) = fs::read_dir(raw.join(dir))
        .ok()
        .and_then(|d| d.flatten().map(|e| e.path()).find(|p| p.to_string_lossy().ends_with("-sentences.txt")))
    else {
        eprintln!("missing corpus {dir}: skipped");
        return Vec::new();
    };
    let text = fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    let tail = &lines[lines.len().saturating_sub(HELD_OUT_SENTENCES)..];
    let stride = (tail.len() / limit.max(1)).max(1);
    tail.iter().step_by(stride).take(limit).filter_map(|l| l.split('\t').nth(1)).map(String::from).collect()
}

fn load(root: &Path, tsv: &str, bin: &str) -> (Lexicon, Bigrams) {
    let lexicon = Lexicon::parse(&fs::read_to_string(root.join(tsv)).expect("lexicon"));
    let bytes: &'static [u8] = Box::leak(fs::read(root.join(bin)).unwrap_or_default().into_boxed_slice());
    let bigrams = Bigrams::from_bytes(bytes, lexicon.len());
    (lexicon, bigrams)
}

/// Summarises the journal the app writes when "Ghi nhật ký sửa lỗi" is on:
/// how often corrections are undone, and which ones. A correction that is
/// undone is the best signal there is that the model was wrong.
fn journal_report(path: &Path) {
    let text = fs::read_to_string(path).unwrap_or_default();
    // (keys, fix) -> (times fixed, times undone)
    let mut pairs: HashMap<(String, String), (u32, u32)> = HashMap::new();
    let (mut fixes, mut undos) = (0u32, 0u32);
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        let [_, kind, keys, fix, ..] = f[..] else { continue };
        let e = pairs.entry((keys.to_string(), fix.to_string())).or_default();
        match kind {
            "FIX" => {
                fixes += 1;
                e.0 += 1;
            }
            "UNDO" => {
                undos += 1;
                e.1 += 1;
            }
            _ => {}
        }
    }
    println!("{}: {fixes} corrections, {undos} undone ({:.1}%)", path.display(), 100.0 * f64::from(undos) / f64::from(fixes.max(1)));
    let mut by_undos: Vec<_> = pairs.iter().filter(|(_, c)| c.1 > 0).collect();
    by_undos.sort_by_key(|(_, c)| std::cmp::Reverse((c.1, c.0)));
    println!("most undone (typed -> fix: undone/fixed):");
    for ((keys, fix), (made, undone)) in by_undos.into_iter().take(20) {
        println!("  {keys} -> {fix}: {undone}/{made}");
    }
    let mut kept: Vec<_> = pairs.iter().filter(|(_, c)| c.1 == 0).collect();
    kept.sort_by_key(|(_, c)| std::cmp::Reverse(c.0));
    println!("most frequent corrections never undone:");
    for ((keys, fix), (made, _)) in kept.into_iter().take(20) {
        println!("  {keys} -> {fix}: {made}");
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--journal") {
        let default = std::env::var("APPDATA").map(|d| PathBuf::from(d).join("AutoCorrect").join("journal.tsv")).unwrap_or_default();
        journal_report(&args.get(i + 1).map(PathBuf::from).unwrap_or(default));
        return;
    }
    let flag = |name: &str, default: f64| {
        args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok()).unwrap_or(default)
    };
    let sentences = flag("--sentences", 800.0) as usize;
    let rate = flag("--rate", 0.15);
    let seed = flag("--seed", 7.0) as u64;
    let show = flag("--show", 0.0) as usize;
    let bare_rate = flag("--bare", 0.05);
    let d = Tuning::preset(flag("--preset", 1.0) as u8);
    let tuning = Tuning {
        known_word: flag("--known", d.known_word),
        known_syllable: flag("--known-syllable", d.known_syllable),
        rare_typed_penalty: flag("--rare", d.rare_typed_penalty),
        floor: flag("--floor", d.floor),
        margin: flag("--margin", d.margin),
        ambiguity: flag("--ambiguity", d.ambiguity),
        far_floor: flag("--far-floor", d.far_floor),
        far_ambiguity: flag("--far-ambiguity", d.far_ambiguity),
        bigram_weight: flag("--weight", d.bigram_weight),
        language_penalty: flag("--language", d.language_penalty),
        phrase_decay: flag("--phrase", d.phrase_decay),
        restore_margin: flag("--restore-margin", d.restore_margin),
        restore_ambiguity: flag("--restore-ambiguity", d.restore_ambiguity),
        restore_english: flag("--restore-english", d.restore_english),
    };

    let root: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let raw = root.join("data/raw");
    let (vi_lex, vi_bi) = load(&root, "data/vi_syllables.tsv", "data/vi_bigrams.bin");
    let (en_lex, en_bi) = load(&root, "data/en_words.tsv", "data/en_bigrams.bin");
    println!("bigrams: vi {} pairs, en {} pairs", vi_bi.len(), en_bi.len());
    let misspellings = if args.iter().any(|a| a == "--no-list") {
        String::new()
    } else {
        fs::read_to_string(root.join("data/en_misspellings.tsv")).unwrap_or_default()
    };
    let mut corrector = SmartCorrector::new(vi_lex, en_lex).with_bigrams(vi_bi, en_bi).with_misspellings(&misspellings);
    corrector.set_tuning(tuning);
    println!("{tuning:?}");

    let started = Instant::now();
    let sets: [(&str, Language, Vec<&str>); 2] = [
        ("Vietnamese", Language::Vietnamese, vec!["vie_news_2022_1M", "vie-vn_web_2015_1M"]),
        ("English", Language::English, vec!["eng_news_2023_1M"]),
    ];
    let mut alls: Vec<Vec<Word>> = sets
        .iter()
        .map(|(_, language, dirs)| {
            let mut all: Vec<Word> = Vec::new();
            for dir in dirs {
                for sentence in held_out(&raw, dir, sentences / dirs.len() + 1) {
                    all.extend(words(&sentence, *language));
                }
            }
            // Words under 3 keys are never corrected, so they say nothing here.
            all.retain(|w| w.keys.len() >= 3);
            all
        })
        .collect();
    // Code-switching: with this probability the word before a word is one of
    // the other language ("tôi dùng laptop mới"), as in real mixed typing.
    let mixed = flag("--mixed", 0.0);
    if mixed > 0.0 {
        let pools: Vec<Vec<String>> = alls.iter().map(|all| all.iter().map(|w| w.text.clone()).collect()).collect();
        let mut rng = Rng(seed.wrapping_mul(0x2545_F491_4F6C_DD1D) | 1);
        for (i, all) in alls.iter_mut().enumerate() {
            let other = &pools[1 - i];
            for word in all.iter_mut().filter(|w| w.prev.is_some()) {
                if rng.chance(mixed) {
                    let foreign = other[(rng.next() as usize) % other.len()].clone();
                    word.prev = Some(foreign.clone());
                    if let Some(last) = word.history.last_mut() {
                        *last = foreign;
                    }
                }
            }
        }
    }
    for ((name, language, _), all) in sets.into_iter().zip(alls) {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let cases: Vec<Case> = all
            .iter()
            .map(|word| {
                let double = rng.chance(0.2);
                // Some words are typed without marks instead of with a slip.
                let stripped = language == Language::Vietnamese && !word.text.is_ascii() && rng.chance(bare_rate);
                if stripped {
                    return Case { word, typed: Some(bare(&word.text)), bare: true };
                }
                let typed = rng.chance(rate).then(|| typo(&word.keys, language, double, &mut rng)).flatten();
                Case { word, typed, bare: false }
            })
            .collect();
        println!("\n== {name}: {} words, {} with typos ==", all.len(), cases.iter().filter(|c| c.typed.is_some()).count());
        for (label, context) in [("no context  ", false), ("with context", true)] {
            corrector.set_context(context);
            let ((clean, false_fix), one, two, stripped) = run(&corrector, &cases, if context { show } else { 0 });
            println!(
                "{label}: {clean} correct words, {false_fix} changed ({:.2} per 1000)",
                1000.0 * f64::from(false_fix) / f64::from(clean.max(1))
            );
            println!("              one slip : {}", one.row());
            println!("              two slips: {}", two.row());
            if stripped.n > 0 {
                println!("              no marks : {}", stripped.row());
            }
        }
    }
    println!("\n({:.0}s)", started.elapsed().as_secs_f64());
}
