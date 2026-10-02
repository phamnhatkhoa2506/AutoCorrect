//! Tracks which program is in front and whether a password field has focus.
//!
//! WinEvent callbacks arrive on the main thread (it pumps messages). Asking
//! UI Automation about the focused element can take tens of milliseconds, so
//! that runs on a worker. Until it answers, keys are only observed for a
//! short grace period (not composed, not corrected, so a password is never
//! altered), but no longer: in a big page (Edge) or right after startup the
//! answer can take seconds. A known password field is hands-off entirely.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::OnceLock;
use std::thread;
use std::time::Instant;

use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HWND};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, SetWinEventHook, HWINEVENTHOOK,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, EVENT_OBJECT_FOCUS, EVENT_SYSTEM_FOREGROUND,
    WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
};

use crate::{hook, log};

/// The last answer: a password field has focus.
static PASSWORD: AtomicBool = AtomicBool::new(false);
/// When the running check began (milliseconds since start, plus one); 0 when
/// none is running.
static PENDING: AtomicU64 = AtomicU64::new(0);
/// Bumped for every focus change, so a stale answer is dropped.
static GENERATION: AtomicU32 = AtomicU32::new(0);
static CHECK: OnceLock<Sender<()>> = OnceLock::new();

/// How long an unanswered check keeps keys observe-only.
const GRACE_MS: u64 = 200;

fn now_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64 + 1
}

/// A password field has focus: keys must pass through untouched and nothing
/// may be remembered.
pub fn password() -> bool {
    PASSWORD.load(Ordering::Relaxed)
}

/// The check of a very recent focus change has not answered yet: keys must
/// not be changed (no Telex, no corrections), but are still followed so
/// that words typed right after switching windows are not lost.
pub fn pending() -> bool {
    let since = PENDING.load(Ordering::Relaxed);
    since != 0 && now_ms().saturating_sub(since) < GRACE_MS
}

/// Installs the WinEvent hooks and starts the password checker.
pub fn start() {
    let (tx, rx) = mpsc::channel::<()>();
    let _ = CHECK.set(tx);
    thread::spawn(move || unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let automation: Option<IUIAutomation> =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok();
        // Fields seen masked, so "show password" (which turns the field into
        // plain text) does not unlock them.
        let mut seen_masked: HashSet<String> = HashSet::new();
        let mut was_password = false;
        while rx.recv().is_ok() {
            while rx.try_recv().is_ok() {} // only the latest focus matters
            let generation = GENERATION.load(Ordering::Relaxed);
            let element = automation.as_ref().and_then(|a| a.GetFocusedElement().ok());
            let password = element.is_some_and(|e| is_password_field(&e, &mut seen_masked));
            if password != was_password && log::debug_enabled() {
                log::debug(format!("focus: {}", if password { "password field (hands off)" } else { "normal field" }));
            }
            was_password = password;
            if GENERATION.load(Ordering::Relaxed) == generation {
                PASSWORD.store(password, Ordering::Relaxed);
                PENDING.store(0, Ordering::Relaxed);
            }
        }
    });

    unsafe {
        let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
        for event in [EVENT_SYSTEM_FOREGROUND, EVENT_OBJECT_FOCUS] {
            SetWinEventHook(event, event, None, Some(on_event), 0, 0, flags);
        }
    }
    on_foreground(unsafe { GetForegroundWindow() });
    recheck_focus();
}

/// Masked now, masked earlier (same process, id and name), or labelled as a
/// password ("Password", "mật khẩu", id "pwd"...).
unsafe fn is_password_field(e: &IUIAutomationElement, seen_masked: &mut HashSet<String>) -> bool {
    let id = e.CurrentAutomationId().map(|s| s.to_string()).unwrap_or_default();
    let name = e.CurrentName().map(|s| s.to_string()).unwrap_or_default();
    let pid = e.CurrentProcessId().unwrap_or_default();
    // Unnamed fields cannot be told apart: never remember those.
    let key = (!id.is_empty() || !name.is_empty()).then(|| format!("{pid}|{id}|{name}"));

    let masked = e.CurrentIsPassword().is_ok_and(|b| b.as_bool());
    if masked {
        if let Some(key) = &key {
            seen_masked.insert(key.clone());
        }
        return true;
    }
    key.is_some_and(|k| seen_masked.contains(&k)) || looks_like_password(&id) || looks_like_password(&name)
}

fn looks_like_password(text: &str) -> bool {
    const WORDS: &[&str] =
        &["password", "passwd", "pwd", "passcode", "passphrase", "mật khẩu", "mat khau", "matkhau"];
    let text = text.to_lowercase();
    WORDS.iter().any(|w| text.contains(w))
}

unsafe extern "system" fn on_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    _hwnd: HWND,
    _object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    if event == EVENT_SYSTEM_FOREGROUND {
        // The event's window is not always the one left in front (taskbar
        // clicks report explorer.exe): ask for the actual foreground.
        on_foreground(GetForegroundWindow());
    }
    recheck_focus();
}

fn recheck_focus() {
    GENERATION.fetch_add(1, Ordering::Relaxed);
    PENDING.store(now_ms(), Ordering::Relaxed);
    if let Some(tx) = CHECK.get() {
        let _ = tx.send(());
    }
}

fn on_foreground(hwnd: HWND) {
    let name = process_name(hwnd.0 as isize);
    hook::set_app(&name);
}

/// Executable name of the process owning a window ("Notepad.exe").
pub fn process_name(hwnd: isize) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_labels() {
        for label in ["Password", "Nhập mật khẩu", "Mat khau", "login-pwd", "confirmPassword"] {
            assert!(looks_like_password(label), "{label}");
        }
        for label in ["Username", "Passport number", "Search", ""] {
            assert!(!looks_like_password(label), "{label}");
        }
    }
}
