//! Measures the corrector on held-out sentences with artificial typos.
//!
//!     cargo run -p ac-bench --release -- [--sentences N] [--rate R] [--seed S] [--show N]
//!         [--floor F --margin M --ambiguity A --known K --rare P --weight W --language L]
//!         [--far-floor F --far-ambiguity A --no-list]
//!         [--bare R --restore-margin M --restore-ambiguity A --restore-english E]
//!     cargo run -p ac-bench --release -- --golden [file]    (real and reported cases, bench/golden.tsv)
//!     cargo run -p ac-bench --release -- --from-journal [journal] [out]   (journal -> bench/journal_cases.tsv)
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
    /// The word that follows it in the sentence (diagnostics only).
    next: Option<String>,
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
                out.push(Word { text: text.clone(), keys, prev: prev.take(), history: history.clone(), next: None, capital });
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
    for i in 1..out.len() {
        if out[i].prev.as_deref() == Some(out[i - 1].text.as_str()) {
            out[i - 1].next = Some(out[i].text.clone());
        }
    }
    out
}

/// Why typos are missed or fixed wrongly: is the right word even a
/// candidate, ranked first, blocked by the caution thresholds, or is the
/// typo a real word? Also the accuracy of simply taking the top candidate
/// (no thresholds), with and without the word that follows as an oracle.
fn diagnose(corrector: &SmartCorrector, cases: &[Case]) {
    #[derive(Default)]
    struct Group {
        n: u32,
        right: u32,
        // Not right: (typo is a real word) x (right word: absent, first, lower).
        bad: [[u32; 3]; 2],
        capital: u32,
        top1: u32,
        top1_with_next: u32,
        with_next: u32,
        top1_on_next: u32,
    }
    let mut groups = [Group::default(), Group::default(), Group::default()];
    for case in cases {
        let Some(typed) = &case.typed else { continue };
        let g = &mut groups[if case.bare { 2 } else if is_two_slips(&case.word.keys, typed) { 1 } else { 0 }];
        g.n += 1;
        let history: Vec<&str> = case.word.history.iter().map(String::as_str).collect();
        let typed_keys = typed_as(typed, case.word.capital);
        let outcome = corrector.correct_in(&typed_keys, &history).map(|f| f.to_lowercase());
        let fixed_right = outcome.as_deref() == Some(case.word.text.as_str());
        if fixed_right {
            g.right += 1;
        }
        let ranking = if case.bare {
            if case.word.capital {
                g.capital += 1;
                continue;
            }
            corrector.rank_bare(&typed_keys, &history)
        } else {
            corrector.rank_in(&typed_keys, &history)
        };
        let Some(ranking) = ranking else {
            if !fixed_right {
                g.bad[0][0] += 1;
            }
            continue;
        };
        let position = ranking.candidates.iter().position(|(w, _)| *w == case.word.text);
        if position == Some(0) {
            g.top1 += 1;
        }
        if !fixed_right {
            let real = usize::from(ranking.typed > f64::NEG_INFINITY);
            let class = match position {
                None => 0,
                Some(0) => 1,
                Some(_) => 2,
            };
            g.bad[real][class] += 1;
        }
        if let Some(next) = &case.word.next {
            g.with_next += 1;
            if position == Some(0) {
                g.top1_on_next += 1;
            }
            let best = ranking
                .candidates
                .iter()
                .map(|(w, s)| (w, s + corrector.right_context_bonus(w, next)))
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(w, _)| w.clone());
            if best.as_deref() == Some(case.word.text.as_str()) {
                g.top1_with_next += 1;
            }
        }
    }
    let pct = |part: u32, whole: u32| if whole == 0 { 0.0 } else { 100.0 * f64::from(part) / f64::from(whole) };
    println!("  why typos are not fixed (Vietnamese; share of all typos in the group):");
    for (name, g) in ["one slip", "two slips", "no marks"].iter().zip(&groups) {
        let b = &g.bad;
        println!("    {name} ({} typos): fixed right {:.1}%", g.n, pct(g.right, g.n));
        println!(
            "      typo is a NEW word : right word not a candidate {:.1}%, ranked first but blocked {:.1}%, ranked lower {:.1}%",
            pct(b[0][0], g.n), pct(b[0][1], g.n), pct(b[0][2], g.n)
        );
        println!(
            "      typo is a real word: right word not a candidate {:.1}%, ranked first but blocked {:.1}%, ranked lower {:.1}%",
            pct(b[1][0], g.n), pct(b[1][1], g.n), pct(b[1][2], g.n)
        );
        if g.capital > 0 {
            println!("      capitalised (left alone on purpose): {:.1}%", pct(g.capital, g.n));
        }
        println!(
            "      top candidate is right (no thresholds): {:.1}%  |  where the next word is known: {:.1}% -> {:.1}% with it",
            pct(g.top1, g.n), pct(g.top1_on_next, g.with_next), pct(g.top1_with_next, g.with_next)
        );
    }
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

/// The golden set: real and reported cases in `bench/golden.tsv` (see its
/// header). "must" cases may never fail; "goal" cases are tracked, and are
/// expected to fail until the model improves. Returns whether every "must"
/// case passed.
fn golden(corrector: &mut SmartCorrector, path: &Path) -> bool {
    use std::collections::BTreeMap;
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    // group -> [must passed, must total, goal passed, goal total]
    let mut groups: BTreeMap<String, [u32; 4]> = BTreeMap::new();
    let mut failures: Vec<String> = Vec::new();
    for line in text.lines().skip(1).filter(|l| !l.trim().is_empty() && !l.starts_with('#')) {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 7 {
            eprintln!("skipped (too few columns): {line}");
            continue;
        }
        let (id, group, mode, context, typed, expected, level) = (f[0], f[1], f[2], f[3], f[4], f[5], f[6]);
        match mode {
            "en" => {
                corrector.set_languages(false, true);
                corrector.set_restore_marks(false);
            }
            "code" => {
                corrector.set_languages(true, false);
                corrector.set_restore_marks(false);
            }
            _ => {
                corrector.set_languages(true, true);
                corrector.set_restore_marks(true);
            }
        }
        let history: Vec<&str> = context.split_whitespace().collect();
        let got = corrector.correct_in(typed, &history);
        let want = (expected != "=").then(|| expected.to_string());
        let ok = got == want;
        let tally = groups.entry(group.to_string()).or_default();
        let at = if level == "goal" { 2 } else { 0 };
        tally[at + 1] += 1;
        tally[at] += u32::from(ok);
        if !ok {
            let show = |v: &Option<String>| v.as_deref().map_or("(unchanged)".to_string(), |s| format!("{s:?}"));
            failures.push(format!(
                "  [{level}] {id} {group}: {typed:?} after {context:?} -> {}, expected {}",
                show(&got),
                show(&want)
            ));
        }
    }
    println!("{:<10} {:>12} {:>12}", "group", "must", "goal");
    let mut total = [0u32; 4];
    for (group, t) in &groups {
        println!("{group:<10} {:>7}/{:<4} {:>7}/{:<4}", t[0], t[1], t[2], t[3]);
        for i in 0..4 {
            total[i] += t[i];
        }
    }
    println!("{:<10} {:>7}/{:<4} {:>7}/{:<4}", "TOTAL", total[0], total[1], total[2], total[3]);
    if !failures.is_empty() {
        println!("\nnot passing:");
        for line in &failures {
            println!("{line}");
        }
    }
    total[0] == total[1]
}

/// Turns the journal into golden cases (`--from-journal [journal] [out]`):
/// a correction you kept is a case that must keep working; one you undid is
/// a case expected to be left alone (a goal until the model stops making it).
/// Lines from before the mode column existed are guessed from the fix.
fn journal_cases(journal: &Path, out: &Path) {
    use std::collections::BTreeSet;
    let text = fs::read_to_string(journal).unwrap_or_default();
    // (keys, fix, context, mode, app)
    type Entry = (String, String, String, String, String);
    let mut entries: Vec<(bool, Entry)> = Vec::new(); // (undone, entry)
    let mut undone_pairs: BTreeSet<(String, String)> = BTreeSet::new();
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        let [_, kind, keys, fix, rest @ ..] = &f[..] else { continue };
        let (kind, keys, fix) = (*kind, *keys, *fix);
        let context = rest.first().copied().unwrap_or("").to_string();
        let mode = rest.get(1).copied().filter(|m| !m.is_empty()).map_or_else(|| if fix.is_ascii() { "en" } else { "vi" }, |m| m).to_string();
        let app = rest.get(2).copied().unwrap_or("Normal").to_string();
        if kind == "UNDO" {
            undone_pairs.insert((keys.to_string(), fix.to_string()));
        }
        if kind == "FIX" || kind == "UNDO" {
            entries.push((kind == "UNDO", (keys.to_string(), fix.to_string(), context, mode, app)));
        }
    }
    let mut seen: BTreeSet<Entry> = BTreeSet::new();
    let mut rows = vec!["id\tgroup\tmode\tcontext\ttyped\texpected\tlevel\tsource\tnote".to_string()];
    for (_, entry) in &entries {
        if !seen.insert(entry.clone()) {
            continue;
        }
        let (keys, fix, context, mode, app) = entry;
        let rejected = undone_pairs.contains(&(keys.clone(), fix.clone()));
        let mode = if app == "Code" && mode == "vi" { "code" } else { mode.as_str() };
        let (group, expected, level) = if rejected { ("journal-undone", "=", "goal") } else { ("journal-kept", fix.as_str(), "must") };
        rows.push(format!("j{:03}\t{group}\t{mode}\t{context}\t{keys}\t{expected}\t{level}\tjournal\t{app}", rows.len()));
    }
    fs::write(out, rows.join("
") + "
").expect("write cases");
    println!("{} cases from {} journal lines -> {}", rows.len() - 1, entries.len(), out.display());
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--journal") {
        let default = std::env::var("APPDATA").map(|d| PathBuf::from(d).join("AutoCorrect").join("journal.tsv")).unwrap_or_default();
        journal_report(&args.get(i + 1).map(PathBuf::from).unwrap_or(default));
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--from-journal") {
        let appdata = std::env::var("APPDATA").map(|d| PathBuf::from(d).join("AutoCorrect")).unwrap_or_default();
        let journal = args.get(i + 1).filter(|a| !a.starts_with("--")).map(PathBuf::from).unwrap_or(appdata.join("journal.tsv"));
        let out = args.get(i + 2).filter(|a| !a.starts_with("--")).map(PathBuf::from).unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/journal_cases.tsv"));
        journal_cases(&journal, &out);
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
        trigram_weight: flag("--tri", d.trigram_weight),
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
    let triples = |vocab: usize, bin: &str| {
        let bytes: &'static [u8] = Box::leak(fs::read(root.join(bin)).unwrap_or_default().into_boxed_slice());
        ac_core::Trigrams::from_bytes(bytes, vocab)
    };
    let (vi_tri, en_tri) = (triples(vi_lex.len(), "data/vi_trigrams.bin"), triples(en_lex.len(), "data/en_trigrams.bin"));
    println!("trigrams: vi {} triples, en {} triples", vi_tri.len(), en_tri.len());
    let mut corrector = SmartCorrector::new(vi_lex, en_lex)
        .with_bigrams(vi_bi, en_bi)
        .with_trigrams(vi_tri, en_tri)
        .with_misspellings(&misspellings);
    corrector.set_tuning(tuning);
    if let Some(i) = args.iter().position(|a| a == "--golden") {
        let path = args.get(i + 1).filter(|a| !a.starts_with("--")).map(PathBuf::from).unwrap_or_else(|| root.join("bench/golden.tsv"));
        std::process::exit(if golden(&mut corrector, &path) { 0 } else { 1 });
    }
    println!("{tuning:?}");

    let started = Instant::now();
    let sets: [(&str, Language, Vec<&str>); 3] = [
        ("Vietnamese", Language::Vietnamese, vec!["vie_news_2022_1M", "vie-vn_web_2015_1M"]),
        ("English", Language::English, vec!["eng_news_2023_1M"]),
        ("Vietnamese dialogue (subtitles)", Language::Vietnamese, vec!["vie_subtitles"]),
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
            let other = &pools[if sets[i].1 == Language::Vietnamese { 1 } else { 0 }];
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
        if name.starts_with("Vietnamese") && args.iter().any(|a| a == "--diagnose") {
            corrector.set_context(true);
            diagnose(&corrector, &cases);
        }
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
