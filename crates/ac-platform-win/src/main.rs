//! Phase 0 spike: global keyboard hook -> ac-core engine -> SendInput.
//!
//! Run, then type `teh ` or `dunhf ` (Vietnamese IME off) in any app.
//! Press Backspace right after a correction to undo it. Ctrl+C here to quit.

mod hook;
mod inject;
mod log;

use windows::core::Result;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{
    GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_TIME_CRITICAL,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, MSG,
    WH_KEYBOARD_LL, WH_MOUSE_LL,
};

fn main() -> Result<()> {
    let has_flag = |flag: &str| std::env::args().any(|a| a == flag);
    log::start(has_flag("--debug"));
    inject::start();
    hook::init();

    unsafe {
        // Every injected key makes Windows call this thread's hook; a high
        // priority keeps each of those round trips from waiting for the CPU.
        // `--normal-priority` exists to A/B the effect on latency.
        if !has_flag("--normal-priority") {
            SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL)?;
        }

        let hinstance = GetModuleHandleW(None)?.into();
        let keyboard = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook::keyboard_proc), Some(hinstance), 0)?;
        let mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(hook::mouse_proc), Some(hinstance), 0)?;

        println!("ac-spike running. Type `teh `, `dunhf `, `mooir ` anywhere. Ctrl+C to quit.");
        println!("Turn the Vietnamese IME (Unikey/EVKey) OFF for this spike.\n");

        // Low-level hooks are called on this thread, so it must keep pumping.
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let _ = UnhookWindowsHookEx(keyboard);
        let _ = UnhookWindowsHookEx(mouse);
    }
    Ok(())
}
