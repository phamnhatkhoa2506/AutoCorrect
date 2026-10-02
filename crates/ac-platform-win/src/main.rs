//! AutoCorrect: Telex input + typo correction in every Windows app, living in
//! the notification area.
//!
//! Flags: `--debug` opens a console with a key-by-key log, `--en` starts in
//! English, `--normal-priority` disables the hook thread's priority boost.
// No console window unless --debug asks for one.
#![windows_subsystem = "windows"]

mod focus;
mod hook;
mod inject;
mod log;
mod policy;
mod settings;
mod tray;

use std::sync::atomic::{AtomicU32, Ordering};

use windows::core::{w, Result, BOOL};
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, LPARAM, TRUE, WPARAM};
use windows::Win32::System::Console::{AllocConsole, AttachConsole, SetConsoleCtrlHandler, ATTACH_PARENT_PROCESS};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{
    CreateMutexW, GetCurrentThread, GetCurrentThreadId, SetThreadPriority,
    THREAD_PRIORITY_TIME_CRITICAL,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MessageBoxW, PostThreadMessageW, SetWindowsHookExW,
    TranslateMessage, UnhookWindowsHookEx, MB_ICONINFORMATION, MB_OK, MSG, WH_KEYBOARD_LL,
    WH_MOUSE_LL, WM_QUIT,
};

static MAIN_THREAD: AtomicU32 = AtomicU32::new(0);

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

        let hinstance = GetModuleHandleW(None)?.into();
        let keyboard = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook::keyboard_proc), Some(hinstance), 0)?;
        let mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(hook::mouse_proc), Some(hinstance), 0)?;
        tray::create()?;
        focus::start();
        if debug {
            let _ = SetConsoleCtrlHandler(Some(on_console_close), true);
        }
        log::info("AutoCorrect running. Alt+Z or the tray icon switches Vietnamese/English.".into());
        hook::update(|_| {}); // print the mode

        // Hooks, WinEvents and the tray all run on this thread: keep pumping.
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        tray::remove();
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
