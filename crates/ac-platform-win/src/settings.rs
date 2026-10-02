//! User settings (%APPDATA%\AutoCorrect\settings.ini) and the "start with
//! Windows" registry entry.

use std::fs;
use std::path::PathBuf;

use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    /// Telex input on (otherwise keys are typed as is).
    pub vietnamese: bool,
    /// Fix typos on Space.
    pub corrections: bool,
    /// Everything off: keys pass through untouched.
    pub paused: bool,
    /// Append every correction and undo to the local journal file.
    pub journal: bool,
    /// Correct English in terminals and IDEs too (off: only Vietnamese there,
    /// so commands and code are left alone).
    pub code_english: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { vietnamese: true, corrections: true, paused: false, journal: false, code_english: false }
    }
}

fn path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|dir| PathBuf::from(dir).join("AutoCorrect").join("settings.ini"))
}

/// Where corrections are journaled when the setting is on.
pub fn journal_path() -> Option<PathBuf> {
    path().map(|p| p.with_file_name("journal.tsv"))
}

pub fn load() -> Settings {
    let mut s = Settings::default();
    let Some(text) = path().and_then(|p| fs::read_to_string(p).ok()) else {
        return s;
    };
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else { continue };
        let on = value.trim() == "1";
        match key.trim() {
            "vietnamese" => s.vietnamese = on,
            "corrections" => s.corrections = on,
            "paused" => s.paused = on,
            "journal" => s.journal = on,
            "code_english" => s.code_english = on,
            _ => {}
        }
    }
    s
}

pub fn save(s: &Settings) {
    let Some(path) = path() else { return };
    let flag = |b: bool| if b { 1 } else { 0 };
    let text = format!(
        "vietnamese={}\ncorrections={}\npaused={}\njournal={}\ncode_english={}\n",
        flag(s.vietnamese),
        flag(s.corrections),
        flag(s.paused),
        flag(s.journal),
        flag(s.code_english)
    );
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let _ = fs::write(path, text);
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
