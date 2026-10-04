//! The typist: the keys typed for a sentence, and the slips made on the way.
//!
//! The slip kinds and the rates in [`Profile::default`] are guesses, not yet
//! calibrated on real mistakes (RESEARCH.md, section 4).

use ac_core::Key;
use ac_telex::{canonical, to_keys, Method};
use unicode_normalization::UnicodeNormalization;

use crate::Rng;

/// A piece of a sentence as it is typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// A word: the text meant and the keys that type it without a slip.
    /// `vietnamese`: a Vietnamese syllable (its keys may end with a tone key).
    Word { text: String, keys: String, vietnamese: bool },
    Space,
    /// Any other character typed on its own.
    Mark(char),
}

/// What the platform layer makes of a character that is not part of a word,
/// on a US layout (`hook.rs`, the key table): keep the two in step.
pub fn mark_key(c: char) -> Key {
    match c {
        ',' | '.' | ';' | ':' | '?' | '!' => Key::Punct(c),
        ')' | ']' | '}' | '"' => Key::Close(c),
        _ => Key::Reset,
    }
}

/// The tokens of a sentence, or `None` when something in it cannot be typed
/// on a US keyboard with Telex (another script, a symbol, a mixed-case
/// Vietnamese word...).
pub fn plan(sentence: &str) -> Option<Vec<Token>> {
    let mut out = Vec::new();
    let mut run = String::new();
    for c in sentence.nfc() {
        let c = match c {
            '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{AB}' | '\u{BB}' => '"',
            '\u{2018}' | '\u{2019}' => '\'',
            '\u{2013}' | '\u{2014}' => '-',
            '\u{A0}' | '\t' => ' ',
            c => c,
        };
        if c.is_alphanumeric() {
            run.push(c);
            continue;
        }
        if !run.is_empty() {
            out.push(word(&std::mem::take(&mut run))?);
        }
        if c.is_whitespace() {
            if out.last().is_some_and(|t| *t != Token::Space) {
                out.push(Token::Space);
            }
        } else if c.is_ascii_graphic() {
            out.push(Token::Mark(c));
        } else {
            return None;
        }
    }
    if !run.is_empty() {
        out.push(word(&run)?);
    }
    if out.last() == Some(&Token::Space) {
        out.pop();
    }
    Some(out)
}

fn word(token: &str) -> Option<Token> {
    if let Some(canon) = canonical(&token.to_lowercase()) {
        let text = with_case_of(token, &canon)?;
        let keys = to_keys(&text);
        // Typed without a slip, the engine must show exactly this text.
        return (Method::Telex.compose(&keys).text == text).then_some(Token::Word { text, keys, vietnamese: true });
    }
    // Not a Vietnamese syllable: typed letter by letter (English, names, numbers).
    token
        .chars()
        .all(|c| c.is_ascii_alphanumeric())
        .then(|| Token::Word { text: token.to_string(), keys: token.to_string(), vietnamese: false })
}

/// `canon` (lower case) with the capitals of `token`: "việt", "Việt" or "VIỆT".
fn with_case_of(token: &str, canon: &str) -> Option<String> {
    let chars: Vec<char> = token.chars().collect();
    let upper = chars.iter().filter(|c| c.is_uppercase()).count();
    if upper == 0 {
        Some(canon.to_string())
    } else if upper == 1 && chars[0].is_uppercase() {
        let mut rest = canon.chars();
        rest.next().map(|first| first.to_uppercase().chain(rest).collect())
    } else if upper == chars.len() {
        Some(canon.to_uppercase())
    } else {
        None
    }
}

/// A kind of slip, as made on the keys of one word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Slip {
    /// A key next to the right one.
    Neighbour,
    /// A key left out.
    Omit,
    /// A key typed twice.
    Double,
    /// A neighbouring key typed as well ("saoi").
    Extra,
    /// Two keys in the wrong order.
    Swap,
    /// Another tone key.
    ToneWrong,
    /// The tone key left out.
    ToneMissing,
    /// Half of a mark left out: aa, ee, oo, dd, aw, ow, uw.
    MarkHalf,
    /// Shift held a key too long: "ĐIểm".
    CapsHeld,
    /// The spaces between a run of words left out: "quanheej" for "quan hệ". Made by
    /// the simulation on several words at once ([`Profile::join`]), not by [`Profile::slip`].
    SpaceMissing,
}

impl Slip {
    pub const ALL: [Slip; 10] = [
        Slip::Neighbour,
        Slip::Omit,
        Slip::Double,
        Slip::Extra,
        Slip::Swap,
        Slip::ToneWrong,
        Slip::ToneMissing,
        Slip::MarkHalf,
        Slip::CapsHeld,
        Slip::SpaceMissing,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Slip::Neighbour => "neighbour",
            Slip::Omit => "omit",
            Slip::Double => "double",
            Slip::Extra => "extra",
            Slip::Swap => "swap",
            Slip::ToneWrong => "tone-wrong",
            Slip::ToneMissing => "tone-missing",
            Slip::MarkHalf => "mark-half",
            Slip::CapsHeld => "caps-held",
            Slip::SpaceMissing => "space-missing",
        }
    }

    /// The keys with this slip, or `None` if it does not apply to them.
    fn apply(self, keys: &[char], vietnamese: bool, rng: &mut Rng) -> Option<Vec<char>> {
        const TONES: &[char] = &['s', 'f', 'r', 'x', 'j'];
        let mut k = keys.to_vec();
        let n = k.len();
        let tone_last = vietnamese && k.last().is_some_and(|c| TONES.contains(c));
        match self {
            Slip::Neighbour => {
                let i = rng.below(n);
                let near = neighbours(k[i]);
                k[i] = *pick(&near, rng)?;
            }
            Slip::Omit if n >= 3 => {
                k.remove(rng.below(n));
            }
            Slip::Double => {
                let i = rng.below(n);
                k.insert(i, k[i]);
            }
            Slip::Extra => {
                let i = rng.below(n);
                let c = *pick(&neighbours(k[i]), rng)?;
                k.insert(i + rng.below(2), c);
            }
            Slip::Swap if n >= 2 => {
                let i = rng.below(n - 1);
                k.swap(i, i + 1);
            }
            Slip::ToneWrong if tone_last => {
                let others: Vec<char> = TONES.iter().copied().filter(|t| Some(t) != k.last()).collect();
                k[n - 1] = *pick(&others, rng)?;
            }
            Slip::ToneMissing if tone_last => {
                k.pop();
            }
            Slip::MarkHalf => {
                let halves: Vec<usize> = (1..n)
                    .filter(|&i| {
                        let (a, b) = (k[i - 1].to_ascii_lowercase(), k[i]);
                        (a == b && "aeod".contains(b)) || (b == 'w' && "aou".contains(a))
                    })
                    .collect();
                k.remove(*pick(&halves, rng)?);
            }
            Slip::CapsHeld if n >= 2 && k[0].is_ascii_uppercase() && k[1].is_ascii_lowercase() => {
                k[1] = k[1].to_ascii_uppercase();
            }
            _ => return None,
        }
        (k != keys).then_some(k)
    }
}

fn pick<'a, T>(items: &'a [T], rng: &mut Rng) -> Option<&'a T> {
    (!items.is_empty()).then(|| &items[rng.below(items.len())])
}

/// QWERTY neighbours: same row left and right, and the keys diagonally above
/// and below (the case of `c` is kept).
fn neighbours(c: char) -> Vec<char> {
    const ROWS: [&[u8]; 3] = [b"qwertyuiop", b"asdfghjkl", b"zxcvbnm"];
    let lower = c.to_ascii_lowercase() as u8;
    let mut out = Vec::new();
    for (r, row) in ROWS.iter().enumerate() {
        let Some(i) = row.iter().position(|&k| k == lower) else { continue };
        if i > 0 {
            out.push(row[i - 1]);
        }
        if i + 1 < row.len() {
            out.push(row[i + 1]);
        }
        for other in [r.wrapping_sub(1), r + 1] {
            if let Some(row) = ROWS.get(other) {
                out.extend(row.get(i).copied());
                out.extend(row.get(i + 1).copied());
            }
        }
    }
    out.into_iter().map(|b| if c.is_ascii_uppercase() { b.to_ascii_uppercase() } else { b } as char).collect()
}

/// How a typist types: how often and how they slip, and how they react.
#[derive(Debug, Clone)]
pub struct Profile {
    /// Chance that a word gets a slip.
    pub rate: f64,
    /// Relative weight of each slip kind.
    pub weights: Vec<(Slip, f64)>,
    /// Chance that a slip is seen and fixed with Backspace before the word ends.
    pub notice: f64,
    /// Chance of pressing Ctrl+Z right after seeing the app change a word wrongly.
    pub undo: f64,
    /// Chance that a word stands in brackets or quotes, right against it: "(vì)".
    pub wrap: f64,
    /// Chance, per sentence, that the space between two Vietnamese words is left
    /// out ("quanheej" for "quan hệ"); at most once per sentence.
    pub join: f64,
    /// The longest run of words typed without spaces.
    pub join_max: usize,
}

impl Default for Profile {
    /// Guesses, not measurements (see the module documentation).
    fn default() -> Self {
        Self {
            rate: 0.08,
            weights: vec![
                (Slip::Neighbour, 25.0),
                (Slip::Omit, 15.0),
                (Slip::Double, 8.0),
                (Slip::Extra, 12.0),
                (Slip::Swap, 15.0),
                (Slip::ToneWrong, 8.0),
                (Slip::ToneMissing, 8.0),
                (Slip::MarkHalf, 7.0),
                (Slip::CapsHeld, 2.0),
            ],
            notice: 0.3,
            undo: 0.7,
            wrap: 0.02,
            join: 0.15,
            join_max: 5,
        }
    }
}

impl Profile {
    /// One slip on `keys`, drawn by weight among the kinds that apply to them.
    pub fn slip(&self, keys: &str, vietnamese: bool, rng: &mut Rng) -> Option<(Slip, String)> {
        let k: Vec<char> = keys.chars().collect();
        let total: f64 = self.weights.iter().map(|(_, w)| w).sum();
        if k.len() < 2 || total <= 0.0 {
            return None;
        }
        for _ in 0..8 {
            let mut x = (rng.next_u64() % 1_000_000) as f64 / 1_000_000.0 * total;
            let kind = self.weights.iter().find(|(_, w)| {
                x -= w;
                x < 0.0
            });
            let Some(&(kind, _)) = kind else { continue };
            if let Some(slipped) = kind.apply(&k, vietnamese, rng) {
                return Some((kind, slipped.into_iter().collect()));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(text: &str, keys: &str, vietnamese: bool) -> Token {
        Token::Word { text: text.into(), keys: keys.into(), vietnamese }
    }

    #[test]
    fn plans_words_spaces_and_marks() {
        let plan = plan("Vì sao (nó) nói “that”, 2 lần?").unwrap();
        assert_eq!(
            plan,
            [
                word("Vì", "Vif", true),
                Token::Space,
                word("sao", "sao", true),
                Token::Space,
                Token::Mark('('),
                word("nó", "nos", true),
                Token::Mark(')'),
                Token::Space,
                word("nói", "nois", true),
                Token::Space,
                Token::Mark('"'),
                word("that", "that", false),
                Token::Mark('"'),
                Token::Mark(','),
                Token::Space,
                word("2", "2", false),
                Token::Space,
                word("lần", "laanf", true),
                Token::Mark('?'),
            ]
        );
        assert_eq!(super::plan("中文"), None);
    }

    #[test]
    fn slips_apply_only_where_they_can() {
        let mut rng = Rng::new(1);
        let keys: Vec<char> = "thichs".chars().collect();
        assert_eq!(Slip::ToneMissing.apply(&keys, true, &mut rng), Some("thich".chars().collect()));
        assert_eq!(Slip::ToneMissing.apply(&keys, false, &mut rng), None);
        let keys: Vec<char> = "Ddieemr".chars().collect();
        assert_eq!(Slip::CapsHeld.apply(&keys, true, &mut rng), Some("DDieemr".chars().collect()));
        let half = Slip::MarkHalf.apply(&keys, true, &mut rng).unwrap();
        assert!(half == "Dieemr".chars().collect::<Vec<_>>() || half == "Ddiemr".chars().collect::<Vec<_>>());
        for _ in 0..50 {
            let near = Slip::Neighbour.apply(&keys, true, &mut rng).unwrap();
            assert_eq!(near.len(), keys.len());
        }
    }
}
