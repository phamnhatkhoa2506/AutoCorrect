//! Where the config files live.

use std::path::PathBuf;

/// `%APPDATA%/AutoCorrect` on Windows, `$XDG_CONFIG_HOME/autocorrect` (or
/// `~/.config/autocorrect`) elsewhere. `AUTOCORRECT_CONFIG_DIR` overrides it
/// (tests, portable use).
pub fn config_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("AUTOCORRECT_CONFIG_DIR") {
        return Some(PathBuf::from(dir));
    }
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("AutoCorrect"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .map(|d| d.join("autocorrect"))
    }
}

fn file(name: &str) -> Option<PathBuf> {
    config_dir().map(|d| d.join(name))
}

pub fn settings_path() -> Option<PathBuf> {
    file("settings.ini")
}

/// Words never to correct and replacements (see `ac_core::Personal`).
pub fn personal_path() -> Option<PathBuf> {
    file("personal.tsv")
}

/// Per-program overrides (see [`crate::apps`]).
pub fn apps_path() -> Option<PathBuf> {
    file("apps.tsv")
}

/// Corrections and undos, when the user turned the journal on.
pub fn journal_path() -> Option<PathBuf> {
    file("journal.tsv")
}
