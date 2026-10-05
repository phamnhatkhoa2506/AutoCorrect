//! AutoCorrect: Telex input + typo correction in every Windows app, living in
//! the notification area.
//!
//! Flags: `--debug` opens a console with a key-by-key log, `--en` starts in
//! English, `--normal-priority` disables the hook thread's priority boost.
// No console window unless --debug asks for one.
#![windows_subsystem = "windows"]

use ac_platform_win::{focus, hook, log, settings, tray};

use std::sync::atomic::{AtomicU32, Ordering};

use windows::core::{w, Result, BOOL};
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, LPARAM, TRUE, WPARAM};
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::System::Console::{AllocConsole, AttachConsole, SetConsoleCtrlHandler, ATTACH_PARENT_PROCESS};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{
    CreateMutexW, GetCurrentThread, GetCurrentThreadId, SetThreadPriority,
    THREAD_PRIORITY_TIME_CRITICAL,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MessageBoxW, PostThreadMessageW, SetTimer, SetWindowsHookExW,
    TranslateMessage, UnhookWindowsHookEx, HHOOK, MB_ICONINFORMATION, MB_OK, MSG, WH_KEYBOARD_LL,
    WH_MOUSE_LL, WM_QUIT,
};
use windows::Win32::Foundation::HWND;
use std::cell::Cell;

static MAIN_THREAD: AtomicU32 = AtomicU32::new(0);

/// How often the watchdog looks, and how far behind the system's last input
/// the hooks may be before they are considered gone.
const WATCHDOG_MS: u32 = 5000;
const SILENT_MS: u32 = 3000;

thread_local! {
    static HOOKS: Cell<(HHOOK, HHOOK)> = const { Cell::new((HHOOK(std::ptr::null_mut()), HHOOK(std::ptr::null_mut()))) };
    static REINSTALLED: Cell<u32> = const { Cell::new(0) };
}

unsafe fn install() -> Result<()> {
    let hinstance = GetModuleHandleW(None)?.into();
    let keyboard = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook::keyboard_proc), Some(hinstance), 0)?;
    let mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(hook::mouse_proc), Some(hinstance), 0)?;
    HOOKS.with(|h| h.set((keyboard, mouse)));
    hook::mark_seen();
    Ok(())
}

/// Input happened that neither hook saw: Windows dropped them (a callback
/// took too long, e.g. after the machine slept). Put them back.
unsafe extern "system" fn watchdog(_: HWND, _: u32, _: usize, _: u32) {
    let mut last_input = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    if !GetLastInputInfo(&mut last_input).as_bool() {
        return;
    }
    let behind = last_input.dwTime.wrapping_sub(hook::last_seen()) as i32;
    let now = GetTickCount();
    if behind <= SILENT_MS as i32 || now.wrapping_sub(REINSTALLED.with(Cell::get)) < 2 * WATCHDOG_MS {
        return;
    }
    REINSTALLED.with(|r| r.set(now));
    let (keyboard, mouse) = HOOKS.with(Cell::get);
    let _ = UnhookWindowsHookEx(keyboard);
    let _ = UnhookWindowsHookEx(mouse);
    match install() {
        Ok(()) => {
            log::info("hooks were silent while you typed: installed them again".into());
            log::event("keyboard hooks were silent while you typed: installed them again".into());
        }
        Err(e) => {
            log::info(format!("could not install the hooks again: {e}"));
            log::event(format!("could not install the keyboard hooks again: {e}"));
        }
    }
}

fn main() -> Result<()> {
    let has_flag = |flag: &str| std::env::args().any(|a| a == flag);
    let debug = has_flag("--debug");

    unsafe {
        if debug && AttachConsole(ATTACH_PARENT_PROCESS).is_err() {
            let _ = AllocConsole();
        }
        // Two copies would fight over every key.
        let _instance = CreateMutexW(None, true, w!("Local\\AutoCorrect.SingleInstance"))?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            MessageBoxW(
                None,
                w!("AutoCorrect đang chạy rồi (xem icon ở khay hệ thống)."),
                w!("AutoCorrect"),
                MB_OK | MB_ICONINFORMATION,
            );
            return Ok(());
        }
        MAIN_THREAD.store(GetCurrentThreadId(), Ordering::Relaxed);
    }

    log::start(debug);
    let mut saved = settings::load();
    if has_flag("--en") {
        saved.vietnamese = false;
    }
    hook::init(saved);

    unsafe {
        // Every injected key makes Windows call this thread's hook; a high
        // priority keeps each of those round trips from waiting for the CPU.
        if !has_flag("--normal-priority") {
            SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL)?;
        }

        install()?;
        SetTimer(None, 0, WATCHDOG_MS, Some(watchdog));
        tray::create()?;
        focus::start();
        if debug {
            let _ = SetConsoleCtrlHandler(Some(on_console_close), true);
        }
        log::info("AutoCorrect running. The hotkey or the tray icon switches Vietnamese/English.".into());
        hook::update(|_| {}); // print the mode

        // Hooks, WinEvents and the tray all run on this thread: keep pumping.
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // Journal lines still waiting for the words after them are written now. The log
        // thread writes them off-thread: give it a moment before the process ends.
        hook::flush_journal();
        std::thread::sleep(std::time::Duration::from_millis(200));
        tray::remove();
        let (keyboard, mouse) = HOOKS.with(Cell::get);
        let _ = UnhookWindowsHookEx(keyboard);
        let _ = UnhookWindowsHookEx(mouse);
    }
    Ok(())
}

/// Ctrl+C / closing the debug console: quit cleanly so the tray icon goes away.
unsafe extern "system" fn on_console_close(_event: u32) -> BOOL {
    let _ = PostThreadMessageW(MAIN_THREAD.load(Ordering::Relaxed), WM_QUIT, WPARAM(0), LPARAM(0));
    TRUE
}
