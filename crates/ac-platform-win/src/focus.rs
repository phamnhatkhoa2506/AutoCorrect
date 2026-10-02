//! Tracks which program is in front and whether a password field has focus.
//!
//! WinEvent callbacks arrive on the main thread (it pumps messages). Asking
//! UI Automation about the focused element can take tens of milliseconds, so
//! that runs on a worker; until it answers, the focus counts as a password
//! field, so no key of a password can be composed or kept.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::OnceLock;
use std::thread;

use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HWND};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, SetWinEventHook, HWINEVENTHOOK,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, EVENT_OBJECT_FOCUS, EVENT_SYSTEM_FOREGROUND,
    WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
};

use crate::hook;
use crate::policy::classify;

/// A password field has focus, or the check of a new focus is still running.
static BLOCKED: AtomicBool = AtomicBool::new(false);
static CHECK: OnceLock<Sender<()>> = OnceLock::new();

pub fn blocked() -> bool {
    BLOCKED.load(Ordering::Relaxed)
}

/// Installs the WinEvent hooks and starts the password checker.
pub fn start() {
    let (tx, rx) = mpsc::channel::<()>();
    let _ = CHECK.set(tx);
    thread::spawn(move || unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let automation: Option<IUIAutomation> =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok();
        while rx.recv().is_ok() {
            while rx.try_recv().is_ok() {} // only the latest focus matters
            let password = automation
                .as_ref()
                .and_then(|a| a.GetFocusedElement().ok())
                .and_then(|e| e.CurrentIsPassword().ok())
                .is_some_and(|b| b.as_bool());
            BLOCKED.store(password, Ordering::Relaxed);
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

unsafe extern "system" fn on_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    _object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    if event == EVENT_SYSTEM_FOREGROUND {
        on_foreground(hwnd);
    }
    recheck_focus();
}

fn recheck_focus() {
    BLOCKED.store(true, Ordering::Relaxed);
    if let Some(tx) = CHECK.get() {
        let _ = tx.send(());
    }
}

fn on_foreground(hwnd: HWND) {
    let name = process_name(hwnd.0 as isize);
    hook::set_app(classify(&name), &name);
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
