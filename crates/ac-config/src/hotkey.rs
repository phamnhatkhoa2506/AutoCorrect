//! The switch key (Vietnamese/English): its text form, and a detector fed
//! with raw key events.
//!
//! Two kinds, like Unikey: a combination with a key ("Alt+Z", "Ctrl+Space"),
//! which fires when that key goes down while exactly those modifiers are
//! held; and modifiers only ("Ctrl+Shift"), which fires when they are all
//! released again without any other key having been pressed meanwhile.

use std::fmt;

const CTRL: u8 = 1;
const SHIFT: u8 = 2;
const ALT: u8 = 4;
const WIN: u8 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hotkey {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub win: bool,
    /// Windows virtual-key code of the non-modifier key, if any.
    pub key: Option<u16>,
}

impl Hotkey {
    pub const ALT_Z: Hotkey = Hotkey { ctrl: false, shift: false, alt: true, win: false, key: Some(0x5A) };

    fn mods(&self) -> u8 {
        (u8::from(self.ctrl) * CTRL) | (u8::from(self.shift) * SHIFT) | (u8::from(self.alt) * ALT) | (u8::from(self.win) * WIN)
    }

    /// "Ctrl+Shift+Z", "alt+z", "Ctrl+Space", "Ctrl+Shift". `None` when it
    /// would be unusable: a key with no modifier (it would eat that key), or
    /// a single modifier alone (it would fire on every tap).
    pub fn parse(text: &str) -> Option<Hotkey> {
        let mut h = Hotkey { ctrl: false, shift: false, alt: false, win: false, key: None };
        for token in text.split('+').map(str::trim).filter(|t| !t.is_empty()) {
            match token.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => h.ctrl = true,
                "shift" => h.shift = true,
                "alt" => h.alt = true,
                "win" | "super" | "meta" => h.win = true,
                other => {
                    if h.key.is_some() {
                        return None;
                    }
                    h.key = Some(key_code(other)?);
                }
            }
        }
        let mods = h.mods().count_ones();
        let valid = if h.key.is_some() { mods >= 1 } else { mods >= 2 };
        valid.then_some(h)
    }
}

impl fmt::Display for Hotkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts: Vec<String> = Vec::new();
        for (on, name) in [(self.ctrl, "Ctrl"), (self.shift, "Shift"), (self.alt, "Alt"), (self.win, "Win")] {
            if on {
                parts.push(name.to_string());
            }
        }
        if let Some(key) = self.key {
            parts.push(key_name(key));
        }
        write!(f, "{}", parts.join("+"))
    }
}

fn key_code(name: &str) -> Option<u16> {
    let upper = name.to_ascii_uppercase();
    let bytes = upper.as_bytes();
    Some(match upper.as_str() {
        "SPACE" => 0x20,
        "TAB" => 0x09,
        "BACKQUOTE" | "`" => 0xC0,
        _ if bytes.len() == 1 && bytes[0].is_ascii_alphanumeric() => u16::from(bytes[0]),
        _ if upper.starts_with('F') => match upper[1..].parse::<u16>() {
            Ok(n @ 1..=12) => 0x70 + n - 1,
            _ => return None,
        },
        _ => return None,
    })
}

fn key_name(vk: u16) -> String {
    match vk {
        0x20 => "Space".into(),
        0x09 => "Tab".into(),
        0xC0 => "Backquote".into(),
        0x70..=0x7B => format!("F{}", vk - 0x70 + 1),
        _ => char::from(vk as u8).to_string(),
    }
}

/// What a key event means for the hotkey.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing.
    None,
    /// The hotkey fired; let the key through (modifiers must reach the program).
    Fire,
    /// The hotkey fired on its key: swallow that key.
    FireSwallow,
}

pub struct Detector {
    hotkey: Hotkey,
    /// Modifiers currently held, as far as the events showed.
    held: u8,
    /// Every modifier held since the first went down.
    chord: u8,
    /// A non-modifier key was pressed while modifiers were held.
    tainted: bool,
}

fn modifier(vk: u16) -> Option<u8> {
    match vk {
        0x10 | 0xA0 | 0xA1 => Some(SHIFT),
        0x11 | 0xA2 | 0xA3 => Some(CTRL),
        0x12 | 0xA4 | 0xA5 => Some(ALT),
        0x5B | 0x5C => Some(WIN),
        _ => None,
    }
}

impl Detector {
    pub fn new(hotkey: Hotkey) -> Self {
        Self { hotkey, held: 0, chord: 0, tainted: false }
    }

    pub fn hotkey(&self) -> Hotkey {
        self.hotkey
    }

    pub fn set_hotkey(&mut self, hotkey: Hotkey) {
        self.hotkey = hotkey;
        self.reset();
    }

    pub fn reset(&mut self) {
        self.held = 0;
        self.chord = 0;
        self.tainted = false;
    }

    /// Feeds one key event (`down`: key down, otherwise key up).
    pub fn on_event(&mut self, vk: u16, down: bool) -> Outcome {
        // Dummy keys of remapping tools (PowerToys, AutoHotkey) mean nothing.
        if vk == 0xFF {
            return Outcome::None;
        }
        if let Some(bit) = modifier(vk) {
            if down {
                if self.held == 0 {
                    self.chord = 0;
                    self.tainted = false;
                }
                self.held |= bit;
                self.chord |= bit;
                return Outcome::None;
            }
            self.held &= !bit;
            if self.held != 0 {
                return Outcome::None;
            }
            let fire = self.hotkey.key.is_none() && !self.tainted && self.chord == self.hotkey.mods();
            self.chord = 0;
            self.tainted = false;
            return if fire { Outcome::Fire } else { Outcome::None };
        }
        if !down {
            return Outcome::None;
        }
        if self.held != 0 {
            self.tainted = true;
        }
        if self.hotkey.key == Some(vk) && self.held == self.hotkey.mods() && self.held != 0 {
            return Outcome::FireSwallow;
        }
        Outcome::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hk(text: &str) -> Hotkey {
        Hotkey::parse(text).unwrap_or_else(|| panic!("{text} should parse"))
    }

    const CTRL_L: u16 = 0xA2;
    const SHIFT_L: u16 = 0xA0;
    const ALT_L: u16 = 0xA4;
    const Z: u16 = 0x5A;
    const A: u16 = 0x41;
    const SPACE: u16 = 0x20;

    #[test]
    fn parses_and_prints() {
        assert_eq!(hk("alt+z"), Hotkey::ALT_Z);
        assert_eq!(hk("Alt+Z").to_string(), "Alt+Z");
        assert_eq!(hk("shift + ctrl + space").to_string(), "Ctrl+Shift+Space");
        assert_eq!(hk("Ctrl+Shift").to_string(), "Ctrl+Shift");
        assert_eq!(hk("Win+F5").to_string(), "Win+F5");
        // Modifiers always print as Ctrl, Shift, Alt, Win.
        assert_eq!(hk("Alt+Shift").to_string(), "Shift+Alt");
        // Round trip.
        for text in ["Ctrl+Alt+J", "Shift+Alt", "Ctrl+Backquote", "Ctrl+Shift+9"] {
            assert_eq!(hk(text).to_string(), text);
        }
    }

    #[test]
    fn rejects_unusable_hotkeys() {
        for text in ["", "Z", "Ctrl", "Shift", "Ctrl+Z+X", "Ctrl+Banana", "F13", "Ctrl+F0"] {
            assert_eq!(Hotkey::parse(text), None, "{text}");
        }
    }

    fn press(d: &mut Detector, events: &[(u16, bool)]) -> Vec<Outcome> {
        events.iter().map(|&(vk, down)| d.on_event(vk, down)).collect()
    }

    #[test]
    fn key_combination_fires_on_its_key() {
        let mut d = Detector::new(hk("Alt+Z"));
        let out = press(&mut d, &[(ALT_L, true), (Z, true), (Z, false), (ALT_L, false)]);
        assert_eq!(out, [Outcome::None, Outcome::FireSwallow, Outcome::None, Outcome::None]);
        // The key alone, or with other modifiers too, does not.
        assert_eq!(press(&mut d, &[(Z, true), (Z, false)]), [Outcome::None, Outcome::None]);
        let out = press(&mut d, &[(ALT_L, true), (SHIFT_L, true), (Z, true)]);
        assert_eq!(out[2], Outcome::None);
    }

    #[test]
    fn key_combination_with_ctrl_space() {
        let mut d = Detector::new(hk("Ctrl+Space"));
        assert_eq!(press(&mut d, &[(CTRL_L, true), (SPACE, true)])[1], Outcome::FireSwallow);
    }

    #[test]
    fn modifiers_only_fire_on_release() {
        let mut d = Detector::new(hk("Ctrl+Shift"));
        let out = press(&mut d, &[(CTRL_L, true), (SHIFT_L, true), (SHIFT_L, false), (CTRL_L, false)]);
        assert_eq!(out, [Outcome::None, Outcome::None, Outcome::None, Outcome::Fire]);
        // Either release order, and again afterwards.
        let out = press(&mut d, &[(SHIFT_L, true), (CTRL_L, true), (CTRL_L, false), (SHIFT_L, false)]);
        assert_eq!(out[3], Outcome::Fire);
    }

    #[test]
    fn modifiers_only_ignore_a_chord_with_another_key() {
        let mut d = Detector::new(hk("Ctrl+Shift"));
        // Ctrl+Shift+A (a shortcut): not a switch.
        let out = press(&mut d, &[(CTRL_L, true), (SHIFT_L, true), (A, true), (A, false), (SHIFT_L, false), (CTRL_L, false)]);
        assert!(out.iter().all(|o| *o == Outcome::None));
        // Ctrl alone, or Ctrl+Shift+Alt, is not Ctrl+Shift either.
        assert!(press(&mut d, &[(CTRL_L, true), (CTRL_L, false)]).iter().all(|o| *o == Outcome::None));
        let out = press(&mut d, &[(CTRL_L, true), (SHIFT_L, true), (ALT_L, true), (ALT_L, false), (SHIFT_L, false), (CTRL_L, false)]);
        assert!(out.iter().all(|o| *o == Outcome::None));
        // And it works again right after.
        let out = press(&mut d, &[(CTRL_L, true), (SHIFT_L, true), (SHIFT_L, false), (CTRL_L, false)]);
        assert_eq!(out[3], Outcome::Fire);
    }

    #[test]
    fn remapper_dummy_keys_are_ignored() {
        let mut d = Detector::new(hk("Ctrl+Shift"));
        let out = press(&mut d, &[(CTRL_L, true), (0xFF, true), (SHIFT_L, true), (0xFF, false), (SHIFT_L, false), (CTRL_L, false)]);
        assert_eq!(out[5], Outcome::Fire);
    }

    #[test]
    fn changing_the_hotkey_forgets_held_keys() {
        let mut d = Detector::new(hk("Ctrl+Shift"));
        press(&mut d, &[(CTRL_L, true)]);
        d.set_hotkey(hk("Alt+Z"));
        assert_eq!(press(&mut d, &[(ALT_L, true), (Z, true)])[1], Outcome::FireSwallow);
    }
}
