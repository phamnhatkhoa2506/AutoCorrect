//! Low-level hook callbacks. They run on the main thread and must return fast:
//! Windows silently removes a hook that exceeds `LowLevelHooksTimeout`.

use std::cell::{Cell, RefCell};
use std::time::Instant;

use ac_core::{Action, Bigrams, Decision, Engine, Key, Lexicon, SmartCorrector};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyState, VIRTUAL_KEY, VK_BACK, VK_CAPITAL, VK_CONTROL, VK_LCONTROL,
    VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_PACKET, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN,
    VK_SHIFT, VK_SPACE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetForegroundWindow, HC_ACTION, KBDLLHOOKSTRUCT, LLKHF_ALTDOWN,
    LLKHF_INJECTED, WM_KEYDOWN, WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_RBUTTONDOWN, WM_SYSKEYDOWN,
};

use crate::focus;
use crate::inject::{self, Job, JobKind, INJECTED_TAG};
use crate::log;
use crate::policy::{classify, AppKind};
use crate::settings::Settings;
use crate::tray;

struct State {
    engine: Engine<SmartCorrector>,
    foreground: HWND,
    settings: Settings,
    app: AppKind,
}

impl State {
    /// Pushes the settings and the foreground app's policy into the engine.
    fn apply(&mut self) {
        let s = self.settings;
        self.engine.set_vietnamese(s.vietnamese);
        // English corrections would mangle commands and code, unless the user
        // asked for them there (chat panels of an IDE are plain text).
        let english = self.app == AppKind::Normal || (self.app == AppKind::Code && s.code_english);
        self.engine.set_corrections(s.corrections, s.corrections && english);
    }

    /// Keys must pass through untouched and nothing may be remembered.
    fn hands_off(&self) -> bool {
        self.settings.paused || self.app == AppKind::Off || focus::password()
    }
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State {
        engine: Engine::new(corrector()),
        foreground: HWND::default(),
        settings: Settings::default(),
        app: AppKind::Normal,
    });
}

fn corrector() -> SmartCorrector {
    let vi = Lexicon::parse(include_str!("../../../data/vi_syllables.tsv"));
    let en = Lexicon::parse(include_str!("../../../data/en_words.tsv"));
    // Word pairs are built for exactly these lexicons (empty if they differ).
    let vi_pairs = Bigrams::from_bytes(include_bytes!("../../../data/vi_bigrams.bin"), vi.len());
    let en_pairs = Bigrams::from_bytes(include_bytes!("../../../data/en_bigrams.bin"), en.len());
    SmartCorrector::new(vi, en)
        .with_bigrams(vi_pairs, en_pairs)
        .with_misspellings(include_str!("../../../data/en_misspellings.tsv"))
}

/// Builds the engine (parses the lexicons) on the hook thread and applies
/// the saved settings.
pub fn init(settings: Settings) {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.settings = settings;
        state.apply();
    });
}

pub fn settings() -> Settings {
    STATE.with(|cell| cell.borrow().settings)
}

/// Changes the settings (tray menu, Alt+Z). Saving and redrawing the icon
/// happen later on the tray window, never inside the hook.
pub fn update(change: impl FnOnce(&mut Settings)) {
    let s = STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        change(&mut state.settings);
        state.apply();
        state.settings
    });
    tray::changed();
    log::info(format!(
        "mode: {}, corrections {}{}",
        if s.vietnamese { "Vietnamese (Telex)" } else { "English" },
        if s.corrections { "on" } else { "off" },
        if s.paused { ", PAUSED" } else { "" },
    ));
}

/// The foreground program changed (from the WinEvent hook).
pub fn set_app(kind: AppKind, name: &str) {
    STATE.with(|cell| {
        let Ok(mut state) = cell.try_borrow_mut() else { return };
        if state.app != kind {
            state.app = kind;
            state.apply();
        }
    });
    if log::debug_enabled() {
        log::debug(format!("foreground: {name} -> {kind:?}"));
    }
}

pub unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let msg = wparam.0 as u32;
        let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
        // Skip our own injected keys, otherwise we would correct our corrections.
        if is_down && kb.dwExtraInfo != INJECTED_TAG && on_key_down(kb) {
            return LRESULT(1); // swallow
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

pub unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32
        && matches!(wparam.0 as u32, WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN)
    {
        // A click may move the caret: the buffered word no longer matches the screen.
        with_engine(|e| e.on_key(Key::Reset));
    }
    CallNextHookEx(None, code, wparam, lparam)
}

/// Returns true if the key must be swallowed.
unsafe fn on_key_down(kb: &KBDLLHOOKSTRUCT) -> bool {
    let started = Instant::now();
    // Alt+Z switches Vietnamese/English, like Unikey's default.
    if kb.vkCode == u32::from(b'Z') && kb.flags.0 & LLKHF_ALTDOWN.0 != 0 {
        update(|s| s.vietnamese = !s.vietnamese);
        return true;
    }
    let decoded = decode(kb);
    if log::debug_enabled() {
        log::debug(format!(
            "vk=0x{:02X} scan=0x{:04X} injected={} shift={} caps={} -> {:?}",
            kb.vkCode,
            kb.scanCode,
            kb.flags.0 & LLKHF_INJECTED.0 != 0,
            is_down(VK_SHIFT),
            CAPS_LOCK.with(Cell::get),
            decoded,
        ));
    }
    let Some(key) = decoded else {
        return false;
    };

    let foreground = GetForegroundWindow();
    let outcome = STATE.with(|cell| {
        let Ok(mut state) = cell.try_borrow_mut() else {
            return None;
        };
        if state.hands_off() {
            // Password field, paused, or a hands-off app: keep nothing.
            state.engine.on_key(Key::Reset);
            return None;
        }
        if state.foreground != foreground {
            // Ask the window itself: WinEvents can arrive late or out of
            // order (a taskbar click reports explorer.exe after the app).
            state.foreground = foreground;
            let kind = classify(&focus::process_name(foreground.0 as isize));
            if state.app != kind {
                state.app = kind;
                state.apply();
            }
            state.engine.on_key(Key::Reset);
            if state.hands_off() {
                return None;
            }
        }
        // Not yet known whether this is a password field: follow, do not act.
        state.engine.set_observing(focus::pending());
        let before = state.engine.current_word().to_string();
        let keys = state.engine.current_keys().to_string();
        let context = state.engine.context().map(str::to_string);
        let pending = state.engine.last_correction().map(|(k, f)| (k.to_string(), f.to_string()));
        let action = state.engine.on_key(key);
        if state.settings.journal && matches!(action, Action::Replace { .. } | Action::ReplaceThenPass { .. }) {
            let entry = match key {
                Key::Space | Key::Punct(_) => state.engine.last_correction().map(|(k, f)| ("FIX", k.to_string(), f.to_string())),
                Key::Undo => pending.map(|(k, f)| ("UNDO", k, f)),
                _ => None,
            };
            if let Some((kind, keys, fix)) = entry {
                log::journal(format!("{kind}\t{keys}\t{fix}\t{}", context.as_deref().unwrap_or("")));
            }
        }
        if matches!(key, Key::Space | Key::Punct(_)) && log::debug_enabled() {
            let decision = state.engine.last_decision();
            let mut line = format!("  word on Space: {before:?} (keys {keys:?}, after {context:?}) -> {decision:?}");
            if decision == Decision::NoCandidate {
                if let Some(r) = state.engine.corrector().rank(&keys, context.as_deref()) {
                    let top: Vec<String> =
                        r.candidates.iter().take(3).map(|(w, s)| format!("{w} {s:.1}")).collect();
                    line += &format!("  (typed {:.1}; top: {})", r.typed, top.join(", "));
                }
            }
            log::debug(line);
        }
        match action {
            Action::Pass => None,
            Action::Replace { backspaces, text } => Some((before, backspaces, text, true)),
            // Punctuation: our edit first, then the key itself.
            Action::ReplaceThenPass { backspaces, text } => Some((before, backspaces, text, false)),
        }
    });

    let Some((before, backspaces, text, swallow)) = outcome else {
        return false;
    };
    let kind = match key {
        Key::Undo => JobKind::Undo,
        Key::Space | Key::Punct(_) => JobKind::Fix,
        _ => JobKind::Compose,
    };
    inject::run(Job {
        started,
        kind,
        from: before,
        backspaces,
        text,
        hwnd: foreground.0 as isize,
        held_ctrl: (key == Key::Undo).then(|| if is_down(VK_LCONTROL) { VK_LCONTROL } else { VK_RCONTROL }),
    });
    swallow
}

fn with_engine(f: impl FnOnce(&mut Engine<SmartCorrector>) -> Action) {
    STATE.with(|cell| {
        if let Ok(mut state) = cell.try_borrow_mut() {
            f(&mut state.engine);
        }
    });
}

fn is_down(vk: VIRTUAL_KEY) -> bool {
    unsafe { GetAsyncKeyState(vk.0 as i32) < 0 }
}

thread_local! {
    /// Caps Lock state tracked from the key stream. `GetKeyState` would read
    /// this thread's own key-state table, which never sees focused input.
    static CAPS_LOCK: Cell<bool> = Cell::new(unsafe { GetKeyState(VK_CAPITAL.0 as i32) } & 1 != 0);
}

/// Maps a raw key to an engine key. `None` = ignore (pure modifiers).
unsafe fn decode(kb: &KBDLLHOOKSTRUCT) -> Option<Key> {
    let vk = VIRTUAL_KEY(kb.vkCode as u16);
    // 0xFF is a reserved code that types nothing: key remappers (PowerToys,
    // AutoHotkey) inject it around their output to keep Alt from opening
    // menus. Treating it as a reset dropped remapped letters from the word.
    if vk.0 == 0xFF {
        return None;
    }
    if vk == VK_CAPITAL {
        CAPS_LOCK.with(|c| c.set(!c.get()));
    }
    if matches!(
        vk,
        VK_SHIFT | VK_LSHIFT | VK_RSHIFT | VK_CONTROL | VK_LCONTROL | VK_RCONTROL | VK_MENU
            | VK_LMENU | VK_RMENU | VK_LWIN | VK_RWIN | VK_CAPITAL
    ) {
        return None;
    }

    let alt = kb.flags.0 & LLKHF_ALTDOWN.0 != 0;
    if !alt && is_down(VK_CONTROL) && vk.0 == 0x41 {
        return Some(Key::SelectAll);
    }
    if !alt && !is_down(VK_SHIFT) && !is_down(VK_LWIN) && !is_down(VK_RWIN) && is_down(VK_CONTROL) && vk.0 == 0x5A {
        return Some(Key::Undo);
    }
    if alt || is_down(VK_CONTROL) || is_down(VK_LWIN) || is_down(VK_RWIN) {
        return Some(Key::Reset); // shortcut: text may change unpredictably
    }

    let shift = is_down(VK_SHIFT);
    let caps = CAPS_LOCK.with(Cell::get);
    let key = match vk {
        VK_BACK => Key::Backspace,
        VK_SPACE => Key::Space,
        // Unicode chars injected by other tools (e.g. a Vietnamese IME).
        VK_PACKET => char::from_u32(kb.scanCode).map_or(Key::Reset, Key::Char),
        VIRTUAL_KEY(v @ 0x41..=0x5A) => {
            let c = v as u8 as char;
            Key::Char(if shift ^ caps { c } else { c.to_ascii_lowercase() })
        }
        // Punctuation that ends a word, on a US layout: , . ; : ? and !
        VIRTUAL_KEY(0xBC) if !shift => Key::Punct(','),
        VIRTUAL_KEY(0xBE) if !shift => Key::Punct('.'),
        VIRTUAL_KEY(0xBA) => Key::Punct(if shift { ':' } else { ';' }),
        VIRTUAL_KEY(0xBF) if shift => Key::Punct('?'),
        VIRTUAL_KEY(0x31) if shift => Key::Punct('!'),
        VIRTUAL_KEY(v @ 0x30..=0x39) if !shift => Key::Char(v as u8 as char),
        // Enter, Tab, punctuation, arrows, Home/End, Delete, F-keys...
        _ => Key::Reset,
    };
    Some(key)
}
