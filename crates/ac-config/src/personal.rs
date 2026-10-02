//! The personal dictionary as the settings window edits it. The engine reads
//! the same file with `ac_core::Personal`.
//!
//! One entry per line, `#` starts a comment:
//!
//! ```text
//! ignore<TAB>word            never correct this word
//! fix<TAB>typed<TAB>instead  always turn "typed" into "instead"
//! ```

use std::fs;
use std::path::PathBuf;

use crate::paths::personal_path;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entries {
    pub ignore: Vec<String>,
    pub fixes: Vec<(String, String)>,
}

const HEADER: &str = "# Từ điển cá nhân của AutoCorrect. Lưu file rồi chuyển sang cửa sổ khác: app tự nạp lại.\n#\n# ignore\ttừ              không bao giờ tự sửa từ này\n# fix\tgõ\tthành          luôn đổi chữ vừa gõ thành chữ bên phải\n#\n# Viết đúng như bạn gõ phím (khi gõ tiếng Việt thì là phím Telex), chữ thường.\n# App tự thêm dòng ignore khi bạn hoàn tác (Ctrl+Z) cùng một lần sửa hai lần.\n#\n# Ví dụ:\n# ignore\tkubectl\n# fix\tko\tkhông\n";

impl Entries {
    pub fn parse(text: &str) -> Self {
        let mut e = Self::default();
        for line in text.lines().filter(|l| !l.trim_start().starts_with('#')) {
            let fields: Vec<&str> = line.split('\t').map(str::trim).collect();
            match fields[..] {
                ["ignore", word, ..] if !word.is_empty() => {
                    let word = word.to_lowercase();
                    if !e.ignore.contains(&word) {
                        e.ignore.push(word);
                    }
                }
                ["fix", typed, instead, ..] if !typed.is_empty() && !instead.is_empty() => {
                    let typed = typed.to_lowercase();
                    e.fixes.retain(|(t, _)| *t != typed);
                    e.fixes.push((typed, instead.to_string()));
                }
                _ => {}
            }
        }
        e
    }

    /// The file text: the explanatory header, then the entries.
    pub fn to_text(&self) -> String {
        let mut out = String::from(HEADER);
        for word in &self.ignore {
            out.push_str(&format!("ignore\t{word}\n"));
        }
        for (typed, instead) in &self.fixes {
            out.push_str(&format!("fix\t{typed}\t{instead}\n"));
        }
        out
    }
}

pub fn load() -> Entries {
    personal_path()
        .and_then(|p| fs::read_to_string(p).ok())
        .map(|t| Entries::parse(&t))
        .unwrap_or_default()
}

pub fn save(entries: &Entries) {
    let Some(path) = personal_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let _ = fs::write(path, entries.to_text());
}

/// The file, created with the explanatory header if it does not exist yet.
pub fn ensure_file() -> Option<PathBuf> {
    let path = personal_path()?;
    if !path.exists() {
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(&path, HEADER);
    }
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_edits_and_keeps_order() {
        let e = Entries::parse("# c\nignore\tKubeCtl\nignore\tkubectl\nfix\tko\tkhông\nfix\tko\tkhong\nfix\tbad\njunk\n");
        assert_eq!(e.ignore, ["kubectl"]); // lowercase, no duplicates
        assert_eq!(e.fixes, [("ko".to_string(), "khong".to_string())]); // the last one wins
    }

    #[test]
    fn round_trips_through_the_file_text() {
        let e = Entries {
            ignore: vec!["kubectl".into(), "wolff".into()],
            fixes: vec![("cty".into(), "công ty".into()), ("ko".into(), "không".into())],
        };
        assert_eq!(Entries::parse(&e.to_text()), e);
        assert!(e.to_text().starts_with("# Từ điển cá nhân"));
    }
}
