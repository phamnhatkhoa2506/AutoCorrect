//! Simulated typing (RESEARCH.md, section 3).
//!
//!     cargo run -p ac-sim --release -- [--sentences N] [--env normal|code|both] [--seed S]
//!         [--rate R] [--notice P] [--undo P] [--wrap P]
//!         [--strength 0|1|2] [--no-delayed] [--code-english]
//!         [--show N] [--export out.jsonl]
//!
//! Every sentence is a held-out one (never used to build the word tables),
//! typed key by key in Vietnamese mode through the app's engine. English
//! sentences are typed in Vietnamese mode too, as when switching languages
//! mid-text. Slip rates are guesses until calibrated on real mistakes:
//! compare versions with these numbers, do not read them as real-world
//! accuracy.

use std::fs;
use std::io::{BufWriter, Write};
use std::path::Path;

use ac_core::{Kn, Lexicon, SmartCorrector, HELD_OUT_SENTENCES};
use ac_sim::sim::{AppSettings, Env, Outcome, Sim, Tally, Typed, WordResult};
use ac_sim::typist::{plan, Profile, Slip, Token};

const SETS: [(&str, &[&str]); 4] = [
    ("vi-news", &["vie_news_2022_1M", "vie-vn_web_2015_1M"]),
    ("vi-dialogue", &["vie_subtitles"]),
    ("vi-social", &["vie_social"]),
    ("en-news", &["eng_news_2023_1M"]),
];

/// Built as the app builds it (`hook.rs`, `corrector()`): keep the two in step.
fn app_corrector() -> SmartCorrector {
    let vi = Lexicon::parse(include_str!("../../../data/vi_syllables.tsv"));
    let en = Lexicon::parse(include_str!("../../../data/en_words.tsv"));
    let vi_kn = Kn::from_bytes(include_bytes!("../../../data/vi_kn.bin"), vi.len());
    let en_kn = Kn::from_bytes(include_bytes!("../../../data/en_kn.bin"), en.len());
    SmartCorrector::new(vi, en)
        .with_kn(vi_kn, en_kn)
        .with_misspellings(include_str!("../../../data/en_misspellings.tsv"))
}

/// `limit` sentences spread over the held-out tail of a corpus (as `ac-bench` does).
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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{}", include_str!("main.rs").lines().take_while(|l| l.starts_with("//!")).map(|l| l.trim_start_matches("//!")).collect::<Vec<_>>().join("\n"));
        return;
    }
    let has = |name: &str| args.iter().any(|a| a == name);
    let value = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let flag = |name: &str, default: f64| value(name).and_then(|v| v.parse().ok()).unwrap_or(default);

    let sentences = flag("--sentences", 300.0) as usize;
    let seed = flag("--seed", 7.0) as u64;
    let show = flag("--show", 0.0) as usize;
    let d = Profile::default();
    let profile =
        Profile { rate: flag("--rate", d.rate), notice: flag("--notice", d.notice), undo: flag("--undo", d.undo), wrap: flag("--wrap", d.wrap), ..d };
    let settings = AppSettings {
        delayed: !has("--no-delayed"),
        code_english: has("--code-english"),
        strength: flag("--strength", 1.0) as u8,
        ..AppSettings::default()
    };
    let envs = match value("--env").as_deref() {
        Some("normal") => vec![Env::Normal],
        Some("code") => vec![Env::Code],
        _ => vec![Env::Normal, Env::Code],
    };
    let mut export = value("--export").map(|p| BufWriter::new(fs::File::create(p).expect("export file")));
    println!("{settings:?}");
    println!("typist: slip rate {}, sees own slip {}, Ctrl+Z on a wrong change {}, word in brackets/quotes {}", profile.rate, profile.notice, profile.undo, profile.wrap);

    let raw = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/raw");
    let corpora: Vec<(&str, Vec<Vec<Token>>, u32)> = SETS
        .iter()
        .map(|(name, dirs)| {
            let mut plans = Vec::new();
            let mut skipped = 0;
            for dir in *dirs {
                for sentence in held_out(&raw, dir, sentences / dirs.len() + 1) {
                    match plan(&sentence) {
                        Some(p) if p.iter().filter(|t| matches!(t, Token::Word { .. })).count() >= 3 => plans.push(p),
                        _ => skipped += 1,
                    }
                }
            }
            (*name, plans, skipped)
        })
        .collect();

    for env in envs {
        for (set, plans, skipped) in &corpora {
            let mut sim = Sim::new(app_corrector(), env, settings, profile.clone(), seed);
            let mut tally = Tally { skipped: *skipped, ..Tally::default() };
            let mut examples = Vec::new();
            for plan in plans {
                tally.sentences += 1;
                let Typed::Words(words) = sim.type_sentence(plan) else {
                    tally.misaligned += 1;
                    continue;
                };
                let meant: Vec<&str> = words.iter().map(|w| w.intended.as_str()).collect();
                for (i, w) in words.iter().enumerate() {
                    tally.add(w);
                    if let Some(out) = export.as_mut() {
                        writeln!(out, "{}", json_line(env, set, &meant, i, w)).expect("export write");
                    }
                    if matches!(w.outcome, Outcome::Broken | Outcome::Wrong) && examples.len() < show {
                        examples.push(example(&meant, i, w));
                    }
                }
            }
            report(env, set, &tally, &examples);
        }
    }
}

fn report(env: Env, set: &str, t: &Tally, examples: &[String]) {
    let words: u32 = t.outcomes.values().sum();
    let right = t.count(Outcome::Kept) + t.count(Outcome::Broken);
    let wrong = t.count(Outcome::Fixed) + t.count(Outcome::Missed) + t.count(Outcome::Wrong);
    let pct = |a: u32, b: u32| 100.0 * f64::from(a) / f64::from(b.max(1));
    println!(
        "\n== {} / {set}: {} sentences ({} not typeable, {} misaligned), {words} words ==",
        env.name(),
        t.sentences,
        t.skipped,
        t.misaligned
    );
    println!(
        "  typed right {right:>6}: changed by the app {} ({:.2} per 1000)",
        t.count(Outcome::Broken),
        10.0 * pct(t.count(Outcome::Broken), right)
    );
    println!(
        "  typed wrong {wrong:>6}: fixed {:.1}%, missed {:.1}%, wrong fix {:.1}%",
        pct(t.count(Outcome::Fixed), wrong),
        pct(t.count(Outcome::Missed), wrong),
        pct(t.count(Outcome::Wrong), wrong)
    );
    println!("    {:<14}{:>6}{:>9}{:>9}{:>9}", "slip", "words", "fixed", "missed", "wrong");
    for (slip, counts) in &t.by_slip {
        let n: u32 = counts.values().sum();
        let c = |o: Outcome| counts.get(&o).copied().unwrap_or(0);
        println!(
            "    {:<14}{n:>6}{:>8.1}%{:>8.1}%{:>8.1}%",
            slip.map_or("(Telex)", Slip::name),
            pct(c(Outcome::Fixed), n),
            pct(c(Outcome::Missed), n),
            pct(c(Outcome::Wrong), n)
        );
    }
    println!(
        "  slips the typist fixed before ending the word: {}; words put back with Ctrl+Z (counted above as the app left them): {}",
        t.typist_fixed, t.undone
    );
    for e in examples {
        println!("    {e}");
    }
}

/// "broken: ... tôi đang [đi -> đó] ...".
fn example(meant: &[&str], i: usize, w: &WordResult) -> String {
    let left = meant[i.saturating_sub(3)..i].join(" ");
    let right = meant.get(i + 1).copied().unwrap_or("");
    let wrote = w.app_wrote.as_deref().unwrap_or(&w.shown);
    format!("{}: {left} [{} (keys {}) -> {wrote}{}] {right}", w.outcome.name(), w.intended, w.keys, if w.undone { ", undone" } else { "" })
}

/// One word as a JSON line for training. `left` and `right` are the words meant
/// around it in the sentence (punctuation in between is not marked).
fn json_line(env: Env, set: &str, meant: &[&str], i: usize, w: &WordResult) -> String {
    let list = |words: &[&str]| words.iter().map(|w| json_str(w)).collect::<Vec<_>>().join(",");
    format!(
        "{{\"env\":{},\"set\":{},\"left\":[{}],\"intended\":{},\"keys\":{},\"slip\":{},\"typist_fixed\":{},\"baseline\":{},\"shown\":{},\"app_wrote\":{},\"outcome\":{},\"undone\":{},\"right\":[{}]}}",
        json_str(env.name()),
        json_str(set),
        list(&meant[i.saturating_sub(3)..i]),
        json_str(&w.intended),
        json_str(&w.keys),
        w.slip.map_or_else(|| "null".to_string(), |s| json_str(s.name())),
        w.typist_fixed,
        json_str(&w.baseline),
        json_str(&w.shown),
        w.app_wrote.as_deref().map_or_else(|| "null".to_string(), json_str),
        json_str(w.outcome.name()),
        w.undone,
        list(&meant[(i + 1).min(meant.len())..(i + 4).min(meant.len())]),
    )
}

fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
