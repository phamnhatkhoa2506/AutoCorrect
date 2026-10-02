//! Text replacement via SendInput, called from inside the keyboard hook.
//!
//! This runs *before* the hook returns, on purpose: the keys the user types
//! next only reach the program after the hook returns, so they always queue
//! behind the replacement. Handing the job to another thread (as an earlier
//! version did) let those keys overtake it whenever that thread was late,
//! for instance while the machine was busy right after startup, which
//! duplicated or misplaced letters. The price is that SendInput takes
//! 15-40 ms here, a delay that keys typed meanwhile wait out in the queue.

use std::time::Instant;

use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, VIRTUAL_KEY, VK_BACK, VK_SPACE,
};

use crate::log::{self, Event};

/// Narrow no-break space: a printable character that replaces a selection and
/// that no suggestion list reacts to (the trick other Vietnamese input
/// methods use against inline completion in address bars).
const EMPTY_CHAR: u16 = 0x202F;

/// Marker in `dwExtraInfo` so the hook can recognise (and skip) our own keys.
pub const INJECTED_TAG: usize = 0x4143_5350; // "ACSP"

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    /// Telex composition while typing (â, tones...).
    Compose,
    /// Typo correction at the end of a word.
    Fix,
    /// Backspace reverting a correction.
    Undo,
}

pub struct Job {
    pub started: Instant,
    pub kind: JobKind,
    pub from: String,
    pub backspaces: usize,
    pub text: String,
    pub hwnd: isize,
    /// The program completes text inline: see [`EMPTY_CHAR`].
    pub guard: bool,
    /// A Control key the user is holding (Ctrl+Z): released while the keys
    /// go out, otherwise every Backspace would act as Ctrl+Backspace.
    pub held_ctrl: Option<VIRTUAL_KEY>,
}

/// Sends the replacement now and reports it to the log thread.
pub fn run(job: Job) {
    let mut inputs = build_inputs(job.backspaces, &job.text, job.guard);
    if let Some(ctrl) = job.held_ctrl {
        inputs.insert(0, key_event(ctrl, true));
        inputs.push(key_event(ctrl, false));
    }
    let send_start = Instant::now();
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    log::send(Event {
        latency: job.started.elapsed(),
        send_time: send_start.elapsed(),
        kind: job.kind,
        from: job.from,
        backspaces: job.backspaces,
        text: job.text,
        expected_inputs: inputs.len(),
        sent_inputs: sent,
        hwnd: job.hwnd,
    });
}

/// `backspaces` Backspaces followed by `text`, as key down/up pairs.
fn build_inputs(backspaces: usize, text: &str, guard: bool) -> Vec<INPUT> {
    let mut inputs = Vec::with_capacity((backspaces + text.len() + 1) * 2);
    let backspaces = if guard && backspaces > 0 {
        // Typing replaces a selected inline suggestion; the extra Backspace
        // then removes this character instead of a real one. Without any
        // suggestion it is simply typed and deleted again.
        push_key(&mut inputs, VIRTUAL_KEY(0), EMPTY_CHAR, KEYEVENTF_UNICODE);
        backspaces + 1
    } else {
        backspaces
    };
    for _ in 0..backspaces {
        push_key(&mut inputs, VK_BACK, 0, KEYBD_EVENT_FLAGS(0));
    }
    for unit in text.encode_utf16() {
        if unit == u16::from(b' ') {
            // A real Space key is the most compatible choice for terminals.
            push_key(&mut inputs, VK_SPACE, 0, KEYBD_EVENT_FLAGS(0));
        } else {
            push_key(&mut inputs, VIRTUAL_KEY(0), unit, KEYEVENTF_UNICODE);
        }
    }
    inputs
}

/// A key no program reacts to (virtual key 0xFF), tagged as ours. Sent after
/// a hotkey swallowed its key: Alt pressed and released with nothing in
/// between would open the program's menu bar.
pub fn dummy_tap() {
    let inputs = [key_event(VIRTUAL_KEY(0xFF), false), key_event(VIRTUAL_KEY(0xFF), true)];
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

/// A single key event (down or up), tagged as ours.
fn key_event(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                time: 0,
                dwExtraInfo: INJECTED_TAG,
            },
        },
    }
}

fn push_key(inputs: &mut Vec<INPUT>, vk: VIRTUAL_KEY, scan: u16, flags: KEYBD_EVENT_FLAGS) {
    for up in [false, true] {
        let flags = if up { flags | KEYEVENTF_KEYUP } else { flags };
        inputs.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: scan,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: INJECTED_TAG,
                },
            },
        });
    }
}
