//! What the app does in each foreground program.

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

/// Classifies a process by its executable name (case-insensitive).
pub fn classify(process: &str) -> AppKind {
    let name = process.to_lowercase();
    if OFF.contains(&name.as_str()) {
        AppKind::Off
    } else if CODE.contains(&name.as_str()) {
        AppKind::Code
    } else {
        AppKind::Normal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_known_apps() {
        assert_eq!(classify("WindowsTerminal.exe"), AppKind::Code);
        assert_eq!(classify("Antigravity IDE.exe"), AppKind::Code);
        assert_eq!(classify("mintty.exe"), AppKind::Code); // Git Bash
        assert_eq!(classify("mstsc.exe"), AppKind::Off);
        assert_eq!(classify("msedge.exe"), AppKind::Normal);
        assert_eq!(classify("Zalo.exe"), AppKind::Normal);
    }
}
