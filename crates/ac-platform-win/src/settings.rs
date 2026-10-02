//! Settings live in `ac-config` (shared with the settings window); this adds
//! what only Windows can do: the "start with Windows" entry, and starting
//! other programs.

use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

pub use ac_config::paths::{apps_path, journal_path, personal_path, settings_path};
pub use ac_config::settings::{load, save, Settings};

/// Opens the personal dictionary in Notepad, creating it first if needed.
pub fn open_personal() {
    let Some(path) = ac_config::personal::ensure_file() else { return };
    let file = HSTRING::from(path.display().to_string());
    unsafe {
        ShellExecuteW(None, w!("open"), w!("notepad.exe"), PCWSTR(file.as_ptr()), None, SW_SHOWNORMAL);
    }
}

/// Opens the settings window: `autocorrect-settings.exe` next to this program.
pub fn open_settings_window() {
    let program = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("autocorrect-settings.exe")))
        .filter(|path| path.exists());
    let Some(program) = program else {
        crate::log::info("autocorrect-settings.exe was not found next to autocorrect.exe".into());
        return;
    };
    let file = HSTRING::from(program.display().to_string());
    unsafe {
        ShellExecuteW(None, w!("open"), PCWSTR(file.as_ptr()), None, None, SW_SHOWNORMAL);
    }
}

const RUN_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const RUN_VALUE: PCWSTR = w!("AutoCorrect");

pub fn autostart() -> bool {
    unsafe {
        RegGetValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE, RRF_RT_REG_SZ, None, None, None).is_ok()
    }
}

pub fn set_autostart(on: bool) {
    unsafe {
        if !on {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE);
            return;
        }
        let Ok(exe) = std::env::current_exe() else { return };
        let command = HSTRING::from(format!("\"{}\"", exe.display()));
        let bytes = (command.len() + 1) * 2; // UTF-16 with terminating NUL
        let _ = RegSetKeyValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            RUN_VALUE,
            REG_SZ.0,
            Some(command.as_ptr().cast()),
            bytes as u32,
        );
    }
}
