//! Console logging on a separate thread: printing can block (e.g. console
//! QuickEdit selection), and the hook thread must never block.

use std::sync::mpsc::{self, Sender};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HWND};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

pub struct Event {
    pub undo: bool,
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
    Correction(Event),
    Debug(String),
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
                Msg::Debug(line) => {
                    println!("  · {line}");
                    continue;
                }
            };
            latencies_us.push(e.latency.as_micros());
            latencies_us.sort_unstable();
            let pct = |p: f64| latencies_us[((latencies_us.len() - 1) as f64 * p) as usize];

            let kind = if e.undo { "UNDO" } else { "FIX " };
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

/// Prints every decoded key: local console only, opt-in via `--debug`.
pub fn debug(line: String) {
    if let Some(tx) = TX.get() {
        let _ = tx.send(Msg::Debug(line));
    }
}

fn process_name(hwnd: isize) -> String {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(HWND(hwnd as _), Some(&mut pid));
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return format!("pid {pid}");
        };
        let mut buf = [0u16; 260];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(handle);
        if ok.is_err() {
            return format!("pid {pid}");
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        path.rsplit('\\').next().unwrap_or(&path).to_string()
    }
}
