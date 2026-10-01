//! Low-level hook callbacks. They run on the main thread and must return fast:
//! Windows silently removes a hook that exceeds `LowLevelHooksTimeout`.

use std::cell::{Cell, RefCell};
use std::time::Instant;

use ac_core::{Action, DictCorrector, Engine, Key};
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

use crate::inject::{self, Job, INJECTED_TAG};
use crate::log;

struct State {
    engine: Engine<DictCorrector>,
    foreground: HWND,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State {
        engine: Engine::new(DictCorrector::builtin()),
        foreground: HWND::default(),
    });
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
        if state.foreground != foreground {
            state.foreground = foreground;
            state.engine.on_key(Key::Reset);
        }
        let before = state.engine.current_word().to_string();
        let action = state.engine.on_key(key);
        if key == Key::Space && log::debug_enabled() {
            log::debug(format!("  word on Space: {before:?} -> {:?}", state.engine.last_decision()));
        }
        match action {
            Action::Pass => None,
            Action::Replace { backspaces, text } => Some((before, backspaces, text)),
        }
    });

    let Some((before, backspaces, text)) = outcome else {
        return false;
    };
    inject::queue(Job {
        started,
        undo: key == Key::Backspace,
        from: before,
        backspaces,
        text,
        hwnd: foreground.0 as isize,
    });
    true
}

fn with_engine(f: impl FnOnce(&mut Engine<DictCorrector>) -> Action) {
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
        VIRTUAL_KEY(v @ 0x30..=0x39) if !shift => Key::Char(v as u8 as char),
        // Enter, Tab, punctuation, arrows, Home/End, Delete, F-keys...
        _ => Key::Reset,
    };
    Some(key)
}
