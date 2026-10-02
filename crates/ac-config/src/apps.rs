//! Which programs get which behaviour, and the user's changes to that.
//!
//! `apps.tsv`, one entry per line (`#` starts a comment):
//!
//! ```text
//! code<TAB>name.exe      terminal or editor: Vietnamese corrections only
//! off<TAB>name.exe       hands off: keys pass through untouched
//! normal<TAB>name.exe    ordinary program (overrides the built-in lists)
//! guard<TAB>name.exe     work around inline completion in its text boxes
//! noguard<TAB>name.exe   do not (overrides the built-in list)
//! ```

use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppKind {
    /// Chat, documents, browsers: Telex and corrections in both languages.
    Normal,
    /// Terminals and code editors: Telex, but only Vietnamese corrections,
    /// since commands and code look like (misspelled) English.
    Code,
    /// Remote desktops and password managers: keys pass through untouched.
    Off,
}

impl AppKind {
    fn word(self) -> &'static str {
        match self {
            AppKind::Normal => "normal",
            AppKind::Code => "code",
            AppKind::Off => "off",
        }
    }
}

const CODE: &[&str] = &[
    // Terminals
    "windowsterminal.exe", "openconsole.exe", "conhost.exe", "cmd.exe", "powershell.exe",
    "pwsh.exe", "mintty.exe", "alacritty.exe", "wezterm-gui.exe", "hyper.exe", "tabby.exe",
    "putty.exe", "conemu64.exe", "warp.exe",
    // Editors and IDEs
    "code.exe", "code - insiders.exe", "cursor.exe", "windsurf.exe", "antigravity.exe",
    "antigravity ide.exe", "zed.exe", "sublime_text.exe", "notepad++.exe", "devenv.exe",
    "idea64.exe", "pycharm64.exe", "webstorm64.exe", "clion64.exe", "rider64.exe",
    "goland64.exe", "rustrover64.exe", "datagrip64.exe", "studio64.exe",
];

const OFF: &[&str] = &[
    "mstsc.exe", "vmconnect.exe", "keepass.exe", "keepassxc.exe", "1password.exe",
    "bitwarden.exe",
];

/// Programs whose address and search boxes complete text inline: the part
/// they add is selected, so the first Backspace deletes that instead of the
/// letter just typed ("to" + suggestion "ols": Backspace leaves "to").
const AUTOCOMPLETE: &[&str] = &[
    "msedge.exe", "chrome.exe", "firefox.exe", "brave.exe", "opera.exe", "vivaldi.exe",
    "coccoc.exe", "browser.exe", "searchhost.exe", "searchapp.exe", "searchui.exe",
    "startmenuexperiencehost.exe",
    // The end-to-end test window (see crates/ac-e2e), to exercise the guard.
    "ac-e2e.exe",
];

pub fn builtin(kind: AppKind) -> &'static [&'static str] {
    match kind {
        AppKind::Code => CODE,
        AppKind::Off => OFF,
        AppKind::Normal => &[],
    }
}

pub fn builtin_guard() -> &'static [&'static str] {
    AUTOCOMPLETE
}

/// The user's overrides on top of the built-in lists. Names are lowercase.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Apps {
    kinds: BTreeMap<String, AppKind>,
    guards: BTreeMap<String, bool>,
}

impl Apps {
    pub fn parse(text: &str) -> Self {
        let mut apps = Self::default();
        for line in text.lines().filter(|l| !l.trim_start().starts_with('#')) {
            let fields: Vec<&str> = line.split('\t').map(str::trim).collect();
            let [what, name, ..] = fields[..] else { continue };
            if name.is_empty() {
                continue;
            }
            let name = name.to_lowercase();
            match what {
                "code" => apps.set_kind(&name, Some(AppKind::Code)),
                "off" => apps.set_kind(&name, Some(AppKind::Off)),
                "normal" => apps.set_kind(&name, Some(AppKind::Normal)),
                "guard" => apps.set_guard(&name, Some(true)),
                "noguard" => apps.set_guard(&name, Some(false)),
                _ => {}
            }
        }
        apps
    }

    pub fn to_text(&self) -> String {
        let mut out = String::from(
            "# Ứng dụng theo nhóm (ghi đè danh sách có sẵn). Tên chương trình viết thường, ví dụ code.exe.\n# code = terminal/IDE, off = tắt hẳn, normal = bình thường, guard/noguard = chống lỗi gợi ý.\n",
        );
        for (name, kind) in &self.kinds {
            out.push_str(&format!("{}\t{name}\n", kind.word()));
        }
        for (name, on) in &self.guards {
            out.push_str(&format!("{}\t{name}\n", if *on { "guard" } else { "noguard" }));
        }
        out
    }

    pub fn set_kind(&mut self, name: &str, kind: Option<AppKind>) {
        match kind {
            Some(kind) => self.kinds.insert(name.to_lowercase(), kind),
            None => self.kinds.remove(&name.to_lowercase()),
        };
    }

    pub fn set_guard(&mut self, name: &str, guard: Option<bool>) {
        match guard {
            Some(on) => self.guards.insert(name.to_lowercase(), on),
            None => self.guards.remove(&name.to_lowercase()),
        };
    }

    /// The user's own entries, for the settings window.
    pub fn kinds(&self) -> impl Iterator<Item = (&str, AppKind)> {
        self.kinds.iter().map(|(n, k)| (n.as_str(), *k))
    }

    pub fn guards(&self) -> impl Iterator<Item = (&str, bool)> {
        self.guards.iter().map(|(n, g)| (n.as_str(), *g))
    }

    /// What to do in the program with this executable name (case-insensitive).
    pub fn classify(&self, process: &str) -> AppKind {
        let name = process.to_lowercase();
        if let Some(kind) = self.kinds.get(&name) {
            return *kind;
        }
        if OFF.contains(&name.as_str()) {
            AppKind::Off
        } else if CODE.contains(&name.as_str()) {
            AppKind::Code
        } else {
            AppKind::Normal
        }
    }

    /// Whether replacements in this program need the inline-completion guard.
    pub fn guard(&self, process: &str) -> bool {
        let name = process.to_lowercase();
        self.guards.get(&name).copied().unwrap_or_else(|| AUTOCOMPLETE.contains(&name.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_lists() {
        let a = Apps::default();
        assert_eq!(a.classify("WindowsTerminal.exe"), AppKind::Code);
        assert_eq!(a.classify("Antigravity IDE.exe"), AppKind::Code);
        assert_eq!(a.classify("mintty.exe"), AppKind::Code); // Git Bash
        assert_eq!(a.classify("mstsc.exe"), AppKind::Off);
        assert_eq!(a.classify("msedge.exe"), AppKind::Normal);
        assert_eq!(a.classify("Zalo.exe"), AppKind::Normal);
        assert!(a.guard("msedge.exe") && a.guard("Chrome.exe") && a.guard("SearchHost.exe"));
        assert!(!a.guard("notepad.exe") && !a.guard("Antigravity IDE.exe"));
    }

    #[test]
    fn the_user_overrides_the_built_in_lists() {
        let a = Apps::parse(
            "# comment\ncode\tZalo.exe\nnormal\tcode.exe\noff\tMyVault.exe\nnoguard\tmsedge.exe\nguard\tmyapp.exe\njunk\tx\ncode\t\n",
        );
        assert_eq!(a.classify("zalo.exe"), AppKind::Code);
        assert_eq!(a.classify("Code.exe"), AppKind::Normal); // built in as Code
        assert_eq!(a.classify("myvault.exe"), AppKind::Off);
        assert!(!a.guard("msedge.exe"));
        assert!(a.guard("MyApp.exe"));
        assert_eq!(a.classify("windowsterminal.exe"), AppKind::Code); // untouched
    }

    #[test]
    fn round_trips_and_edits() {
        let mut a = Apps::default();
        a.set_kind("Foo.exe", Some(AppKind::Off));
        a.set_guard("bar.exe", Some(true));
        assert_eq!(Apps::parse(&a.to_text()), a);
        a.set_kind("foo.exe", None);
        assert_eq!(a.classify("foo.exe"), AppKind::Normal);
        assert_eq!(a.kinds().count(), 0);
        assert_eq!(a.guards().collect::<Vec<_>>(), [("bar.exe", true)]);
    }
}
