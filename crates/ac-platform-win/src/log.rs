//! Console logging on a separate thread: printing can block (e.g. console
//! QuickEdit selection), and the hook thread must never block.

use std::sync::mpsc::{self, Sender};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

use crate::focus::process_name;
use crate::inject::JobKind;

pub struct Event {
    pub kind: JobKind,
    pub from: String,
    pub backspaces: usize,
    pub text: String,
    /// Hook entry -> SendInput returned.
    pub latency: Duration,
    /// Part of `latency` spent inside SendInput.
    pub send_time: Duration,
    pub expected_inputs: usize,
    pub sent_inputs: u32,
    pub hwnd: isize,
}

enum Msg {
    Event(String),
    Correction(Event),
    Debug(String),
    Info(String),
    Journal(String),
    Personal(String),
}

static TX: OnceLock<Sender<Msg>> = OnceLock::new();
static DEBUG: AtomicBool = AtomicBool::new(false);

pub fn start(debug: bool) {
    DEBUG.store(debug, Ordering::Relaxed);
    let (tx, rx) = mpsc::channel::<Msg>();
    let _ = TX.set(tx);
    thread::spawn(move || {
        let mut latencies_us: Vec<u128> = Vec::new();
        for msg in rx {
            let e = match msg {
                Msg::Correction(e) => e,
                Msg::Event(line) => {
                    append_event(&line);
                    if debug_enabled() {
                        println!("  ! {line}");
                    }
                    continue;
                }
                Msg::Debug(line) => {
                    println!("  · {line}");
                    continue;
                }
                Msg::Info(line) => {
                    println!("{line}");
                    continue;
                }
                Msg::Journal(line) => {
                    append_journal(&line);
                    continue;
                }
                Msg::Personal(line) => {
                    append_personal(&line);
                    continue;
                }
            };
            latencies_us.push(e.latency.as_micros());
            latencies_us.sort_unstable();
            let pct = |p: f64| latencies_us[((latencies_us.len() - 1) as f64 * p) as usize];

            // Telex composition happens on most Vietnamese words: debug only.
            let kind = match e.kind {
                JobKind::Compose if !debug_enabled() => continue,
                JobKind::Compose => "TELEX",
                JobKind::Fix => "FIX  ",
                JobKind::Undo => "UNDO ",
            };
            let blocked = if (e.sent_inputs as usize) < e.expected_inputs {
                format!("  !! only {}/{} inputs accepted", e.sent_inputs, e.expected_inputs)
            } else {
                String::new()
            };
            let edit = format!("-{} +{:?}", e.backspaces, e.text);
            println!(
                "{kind} {:>12} {:<14} {:>6} us, SendInput {:>6} us  (p50 {} us, p99 {} us)  [{}]{blocked}",
                e.from,
                edit,
                e.latency.as_micros(),
                e.send_time.as_micros(),
                pct(0.50),
                pct(0.99),
                process_name(e.hwnd),
            );
        }
    });
}

pub fn send(event: Event) {
    if let Some(tx) = TX.get() {
        let _ = tx.send(Msg::Correction(event));
    }
}

pub fn debug_enabled() -> bool {
    DEBUG.load(Ordering::Relaxed)
}

/// One line for `events.log`: what the app decided about the keyboard (hands off in a
/// password field, typing not composed, the mode switched, hooks reinstalled...).
/// Always on and cheap, and never holds anything that was typed: when typing
/// "stops working for a while" this says why.
pub fn event(line: String) {
    if let Some(tx) = TX.get() {
        let _ = tx.send(Msg::Event(line));
    }
}

fn append_event(line: &str) {
    use std::io::Write;
    let Some(dir) = ac_config::paths::config_dir() else { return };
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("events.log");
    // One older generation, so the file never grows without bound.
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 256_000) {
        let _ = std::fs::rename(&path, dir.join("events.old.log"));
    }
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(
            file,
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}  {line}",
            t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond
        );
    }
}

/// One line for the personal dictionary (a word learned from an undo).
pub fn personal(line: String) {
    if let Some(tx) = TX.get() {
        let _ = tx.send(Msg::Personal(line));
    }
}

fn append_personal(line: &str) {
    use std::io::Write;
    let Some(path) = crate::settings::personal_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{line}");
    }
}

/// One line for the local journal (setting `journal`): written off-thread.
pub fn journal(line: String) {
    if let Some(tx) = TX.get() {
        let _ = tx.send(Msg::Journal(line));
    }
}

/// `unix_seconds<TAB>FIX|UNDO|EDIT|NEAR<TAB>...` (see `bench/README.md`).
/// Stops growing at 8 MB: the file holds what was typed.
fn append_journal(line: &str) {
    use std::io::Write;
    let Some(path) = crate::settings::journal_path() else { return };
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 8_000_000) {
        return;
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{secs}\t{line}");
    }
}

/// Always printed (mode changes...).
pub fn info(line: String) {
    if let Some(tx) = TX.get() {
        let _ = tx.send(Msg::Info(line));
    }
}

/// Prints every decoded key: local console only, opt-in via `--debug`.
pub fn debug(line: String) {
    if let Some(tx) = TX.get() {
        let _ = tx.send(Msg::Debug(line));
    }
}
