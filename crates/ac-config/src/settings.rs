//! The settings file: `key=value` lines, 1/0 for switches.

use std::fs;

use crate::hotkey::Hotkey;
use crate::paths::settings_path;

/// How Vietnamese is typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMethod {
    Telex,
    Vni,
}

impl InputMethod {
    pub fn name(self) -> &'static str {
        match self {
            InputMethod::Telex => "telex",
            InputMethod::Vni => "vni",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim() {
            "telex" => Some(InputMethod::Telex),
            "vni" => Some(InputMethod::Vni),
            _ => None,
        }
    }
}

/// How readily the corrector changes what was typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strength {
    /// Fewer corrections, almost never a wrong one.
    Careful,
    Balanced,
    /// More corrections, at the cost of a few more wrong ones.
    Bold,
}

impl Strength {
    pub fn name(self) -> &'static str {
        match self {
            Strength::Careful => "careful",
            Strength::Balanced => "balanced",
            Strength::Bold => "bold",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim() {
            "careful" => Some(Strength::Careful),
            "balanced" => Some(Strength::Balanced),
            "bold" => Some(Strength::Bold),
            _ => None,
        }
    }

    /// The level `ac_core::Tuning::preset` takes: 0, 1 or 2.
    pub fn level(self) -> u8 {
        match self {
            Strength::Careful => 0,
            Strength::Balanced => 1,
            Strength::Bold => 2,
        }
    }
}

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
    /// Work around inline completion in browser address and search boxes.
    pub autocomplete_guard: bool,
    /// Give Vietnamese words typed without marks their marks (khong -> không).
    pub restore_marks: bool,
    /// Switches between Vietnamese and English.
    pub hotkey: Hotkey,
    pub strength: Strength,
    pub input: InputMethod,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            vietnamese: true,
            corrections: true,
            paused: false,
            journal: false,
            code_english: false,
            autocomplete_guard: true,
            restore_marks: true,
            hotkey: Hotkey::ALT_Z,
            strength: Strength::Balanced,
            input: InputMethod::Telex,
        }
    }
}

impl Settings {
    /// Unknown keys and unreadable values are ignored (the default stays).
    pub fn parse(text: &str) -> Self {
        let mut s = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else { continue };
            let on = value.trim() == "1";
            match key.trim() {
                "vietnamese" => s.vietnamese = on,
                "corrections" => s.corrections = on,
                "paused" => s.paused = on,
                "journal" => s.journal = on,
                "code_english" => s.code_english = on,
                "autocomplete_guard" => s.autocomplete_guard = on,
                "restore_marks" => s.restore_marks = on,
                "hotkey" => s.hotkey = Hotkey::parse(value).unwrap_or(s.hotkey),
                "strength" => s.strength = Strength::from_name(value).unwrap_or(s.strength),
                "input" => s.input = InputMethod::from_name(value).unwrap_or(s.input),
                _ => {}
            }
        }
        s
    }

    pub fn to_ini(&self) -> String {
        let flag = |b: bool| u8::from(b);
        format!(
            "vietnamese={}\ncorrections={}\npaused={}\njournal={}\ncode_english={}\nautocomplete_guard={}\nrestore_marks={}\nhotkey={}\nstrength={}\ninput={}\n",
            flag(self.vietnamese),
            flag(self.corrections),
            flag(self.paused),
            flag(self.journal),
            flag(self.code_english),
            flag(self.autocomplete_guard),
            flag(self.restore_marks),
            self.hotkey,
            self.strength.name(),
            self.input.name(),
        )
    }
}

pub fn load() -> Settings {
    settings_path()
        .and_then(|p| fs::read_to_string(p).ok())
        .map(|text| Settings::parse(&text))
        .unwrap_or_default()
}

pub fn save(settings: &Settings) {
    let Some(path) = settings_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let _ = fs::write(path, settings.to_ini());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let s = Settings {
            vietnamese: false,
            paused: true,
            journal: true,
            hotkey: Hotkey::parse("Ctrl+Shift").unwrap(),
            strength: Strength::Bold,
            ..Settings::default()
        };
        assert_eq!(Settings::parse(&s.to_ini()), s);
        assert_eq!(Settings::parse(&Settings::default().to_ini()), Settings::default());
    }

    #[test]
    fn bad_input_keeps_defaults() {
        let s = Settings::parse("hotkey=Banana\nstrength=wild\nvietnamese=0\nmystery=1\nno equals sign\n");
        assert_eq!(s.hotkey, Hotkey::ALT_Z);
        assert_eq!(s.strength, Strength::Balanced);
        assert!(!s.vietnamese);
        // Files written before the hotkey existed still load.
        assert_eq!(Settings::parse("vietnamese=1\n").hotkey, Hotkey::ALT_Z);
    }
}
