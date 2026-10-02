//! Settings window of AutoCorrect. A separate process: it only reads and
//! writes the config files (`ac-config`); the running app notices the change
//! the next time the foreground window changes.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use ac_config::apps::AppKind;
use ac_config::{personal, settings, Apps, Hotkey, InputMethod, Settings, Strength};
use serde::{Deserialize, Serialize};
use std::process::Command;

/// What the window can change; the on/off and pause switches belong to the
/// tray, so they are left as the app wrote them.
#[derive(Serialize, Deserialize)]
struct Options {
    corrections: bool,
    journal: bool,
    code_english: bool,
    autocomplete_guard: bool,
    restore_marks: bool,
    hotkey: String,
    strength: String,
    input: String,
}

#[derive(Serialize, Deserialize)]
struct AppRule {
    name: String,
    /// "normal", "code" or "off".
    kind: String,
}

#[derive(Serialize, Deserialize)]
struct GuardRule {
    name: String,
    guard: bool,
}

#[derive(Serialize, Deserialize)]
struct AppsDto {
    kinds: Vec<AppRule>,
    guards: Vec<GuardRule>,
}

#[derive(Serialize, Deserialize)]
struct Fix {
    typed: String,
    instead: String,
}

#[derive(Serialize, Deserialize)]
struct Dictionary {
    ignore: Vec<String>,
    fixes: Vec<Fix>,
}

#[derive(Serialize)]
struct State {
    options: Options,
    apps: AppsDto,
    dictionary: Dictionary,
    autostart: bool,
    config_dir: String,
}

fn options_of(s: &Settings) -> Options {
    Options {
        corrections: s.corrections,
        journal: s.journal,
        code_english: s.code_english,
        autocomplete_guard: s.autocomplete_guard,
        restore_marks: s.restore_marks,
        hotkey: s.hotkey.to_string(),
        strength: s.strength.name().to_string(),
        input: s.input.name().to_string(),
    }
}

fn kind_name(kind: AppKind) -> &'static str {
    match kind {
        AppKind::Normal => "normal",
        AppKind::Code => "code",
        AppKind::Off => "off",
    }
}

fn kind_of(name: &str) -> AppKind {
    match name {
        "code" => AppKind::Code,
        "off" => AppKind::Off,
        _ => AppKind::Normal,
    }
}

fn load_apps() -> Apps {
    let text = ac_config::paths::apps_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default();
    Apps::parse(&text)
}

trait Hidden {
    fn hidden(&mut self) -> &mut Self;
}

impl Hidden for Command {
    #[cfg(windows)]
    fn hidden(&mut self) -> &mut Self {
        use std::os::windows::process::CommandExt;
        self.creation_flags(0x0800_0000) // CREATE_NO_WINDOW
    }

    #[cfg(not(windows))]
    fn hidden(&mut self) -> &mut Self {
        self
    }
}

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

/// Windows only: whether the "start with Windows" entry exists.
fn run_key_has_entry() -> bool {
    Command::new("reg")
        .args(["query", RUN_KEY, "/v", "AutoCorrect"])
        .hidden()
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[tauri::command]
fn get_state() -> State {
    let apps = load_apps();
    let entries = personal::load();
    State {
        options: options_of(&settings::load()),
        apps: AppsDto {
            kinds: apps.kinds().map(|(n, k)| AppRule { name: n.to_string(), kind: kind_name(k).to_string() }).collect(),
            guards: apps.guards().map(|(n, g)| GuardRule { name: n.to_string(), guard: g }).collect(),
        },
        dictionary: Dictionary {
            ignore: entries.ignore,
            fixes: entries.fixes.into_iter().map(|(typed, instead)| Fix { typed, instead }).collect(),
        },
        autostart: run_key_has_entry(),
        config_dir: ac_config::paths::config_dir().map(|p| p.display().to_string()).unwrap_or_default(),
    }
}

/// Checks a hotkey typed in the window; returns it in the canonical form.
#[tauri::command]
fn check_hotkey(text: String) -> Result<String, String> {
    Hotkey::parse(&text)
        .map(|h| h.to_string())
        .ok_or_else(|| "Phím tắt không dùng được: cần Ctrl/Alt/Shift/Win cùng một phím, hoặc ít nhất hai phím bổ trợ (ví dụ Ctrl+Shift).".to_string())
}

#[tauri::command]
fn save_options(options: Options) -> Result<(), String> {
    // Keep what the tray owns (on/off, paused) as the app last wrote it.
    let mut s = settings::load();
    s.corrections = options.corrections;
    s.journal = options.journal;
    s.code_english = options.code_english;
    s.autocomplete_guard = options.autocomplete_guard;
    s.restore_marks = options.restore_marks;
    s.hotkey = Hotkey::parse(&options.hotkey).ok_or("Phím tắt không hợp lệ")?;
    s.strength = Strength::from_name(&options.strength).unwrap_or(s.strength);
    s.input = InputMethod::from_name(&options.input).unwrap_or(s.input);
    settings::save(&s);
    Ok(())
}

#[tauri::command]
fn save_apps(apps: AppsDto) {
    let mut out = Apps::default();
    for rule in &apps.kinds {
        let name = rule.name.trim();
        if !name.is_empty() {
            out.set_kind(name, Some(kind_of(&rule.kind)));
        }
    }
    for rule in &apps.guards {
        let name = rule.name.trim();
        if !name.is_empty() {
            out.set_guard(name, Some(rule.guard));
        }
    }
    if let Some(path) = ac_config::paths::apps_path() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, out.to_text());
    }
}

#[tauri::command]
fn save_dictionary(dictionary: Dictionary) {
    let clean = |s: &str| s.trim().to_lowercase();
    let entries = personal::Entries {
        ignore: dictionary.ignore.iter().map(|w| clean(w)).filter(|w| !w.is_empty()).collect(),
        fixes: dictionary
            .fixes
            .iter()
            .map(|f| (clean(&f.typed), f.instead.trim().to_string()))
            .filter(|(t, i)| !t.is_empty() && !i.is_empty())
            .collect(),
    };
    personal::save(&entries);
}

#[tauri::command]
fn set_autostart(on: bool) {
    if !on {
        let _ = Command::new("reg").args(["delete", RUN_KEY, "/v", "AutoCorrect", "/f"]).hidden().output();
        return;
    }
    let exe = std::env::current_exe()
        .ok()
        .and_then(|me| me.parent().map(|dir| dir.join("autocorrect.exe")))
        .filter(|p| p.exists());
    if let Some(exe) = exe {
        let command = format!("\"{}\"", exe.display());
        let _ = Command::new("reg")
            .args(["add", RUN_KEY, "/v", "AutoCorrect", "/t", "REG_SZ", "/d", &command, "/f"])
            .hidden()
            .output();
    }
}

#[tauri::command]
fn open_config_dir() {
    if let Some(dir) = ac_config::paths::config_dir() {
        let _ = std::fs::create_dir_all(&dir);
        let _ = Command::new("explorer").arg(dir).spawn();
    }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_state,
            check_hotkey,
            save_options,
            save_apps,
            save_dictionary,
            set_autostart,
            open_config_dir
        ])
        .run(tauri::generate_context!())
        .expect("error while running the settings window");
}
