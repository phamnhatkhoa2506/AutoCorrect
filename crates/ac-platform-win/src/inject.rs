//! Text replacement via SendInput, on a dedicated thread.
//!
//! Calling SendInput from inside the low-level hook makes it wait on that same
//! hook (measured 20-50 ms), so the hook only queues a job and returns.

use std::sync::mpsc::{self, Sender};
use std::sync::OnceLock;
use std::thread;
use std::time::Instant;

use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, VIRTUAL_KEY, VK_BACK, VK_SPACE,
};

use crate::log::{self, Event};

/// Marker in `dwExtraInfo` so the hook can recognise (and skip) our own keys.
pub const INJECTED_TAG: usize = 0x4143_5350; // "ACSP"

pub struct Job {
    pub started: Instant,
    pub undo: bool,
    pub from: String,
    pub backspaces: usize,
    pub text: String,
    pub hwnd: isize,
}

static TX: OnceLock<Sender<Job>> = OnceLock::new();

pub fn start() {
    let (tx, rx) = mpsc::channel::<Job>();
    let _ = TX.set(tx);
    thread::spawn(move || {
        for job in rx {
            let inputs = build_inputs(job.backspaces, &job.text);
            let send_start = Instant::now();
            let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
            log::send(Event {
                latency: job.started.elapsed(),
                send_time: send_start.elapsed(),
                undo: job.undo,
                from: job.from,
                backspaces: job.backspaces,
                text: job.text,
                expected_inputs: inputs.len(),
                sent_inputs: sent,
                hwnd: job.hwnd,
            });
        }
    });
}

pub fn queue(job: Job) {
    if let Some(tx) = TX.get() {
        let _ = tx.send(job);
    }
}

/// `backspaces` Backspaces followed by `text`, as key down/up pairs.
fn build_inputs(backspaces: usize, text: &str) -> Vec<INPUT> {
    let mut inputs = Vec::with_capacity((backspaces + text.len()) * 2);
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
