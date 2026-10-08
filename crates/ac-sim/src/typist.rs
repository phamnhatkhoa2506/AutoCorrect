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
    /// The same, with at least one word that is not a Vietnamese syllable in the run ("mìnhtrain"): English words,
    /// names and numbers do not get the syllable cutter, so this is measured apart.
    SpaceMissingMixed,
    /// The hand slides: two or three keys in a row, each next to the one before, put in at one place.
    Slide,
    /// A finger presses two keys at once: two keys next to one key, put on either side of it.
    Multi,
    /// A key held or bouncing: one key typed three or four times in a row ("nguuu").
    Held,
}

impl Slip {
    pub const ALL: [Slip; 14] = [
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
        Slip::SpaceMissingMixed,
        Slip::Slide,
        Slip::Multi,
        Slip::Held,
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
            Slip::SpaceMissingMixed => "space-missing-mixed",
            Slip::Slide => "slide",
            Slip::Multi => "multi-press",
            Slip::Held => "held-key",
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
                // A key next to the right one on the writer's keyboard, or one of the writer's own habits for it.
                let i = rng.below(n);
                k[i] = crate::keyboard::laptop().mistake_for(k[i], rng)?;
            }
            Slip::Slide => {
                let at = rng.below(n + 1);
                let mut pool: Vec<char> = Vec::new();
                for side in [at.checked_sub(1).and_then(|i| k.get(i)), k.get(at)].into_iter().flatten() {
                    pool.extend(neighbours(*side));
                }
                let mut block: Vec<char> = Vec::new();
                for _ in 0..2 + rng.below(2) {
                    let mut options = pool.clone();
                    if let Some(&previous) = block.last() {
                        options.extend(neighbours(previous));
                    }
                    block.push(*pick(&options, rng)?);
                }
                k.splice(at..at, block);
            }
            Slip::Multi => {
                let i = rng.below(n);
                let near = neighbours(k[i]);
                let (a, b) = (*pick(&near, rng)?, *pick(&near, rng)?);
                match rng.below(3) {
                    0 => drop(k.splice(i..i, [a, b])),
                    1 => drop(k.splice(i + 1..i + 1, [a, b])),
                    _ => {
                        k.insert(i + 1, b);
                        k.insert(i, a);
                    }
                }
            }
            Slip::Held => {
                let i = rng.below(n);
                let copies = 2 + rng.below(2);
                for _ in 0..copies {
                    k.insert(i, k[i]);
                }
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

/// The letters and digits on the keys next to the key of `c` on the writer's keyboard (the case of `c` is kept).
fn neighbours(c: char) -> Vec<char> {
    crate::keyboard::laptop().neighbours(c)
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
    /// Chance, per sentence, that the spaces inside a run of words are left out ("quanheej" for
    /// "quan hệ", "đichơi" for "đi chơi"). The words may be Vietnamese, English, names or numbers.
    pub join: f64,
    /// The longest run of words typed without spaces (a run has 2 to `join_max` words). It is a cap on the data, never
    /// an input of the problem: the length of a run is drawn from a long-tailed law (see `join_continue`).
    pub join_max: usize,
    /// A run of k words becomes one of k + 1 with this chance (until `join_max`): two words are the most common, long
    /// runs are rare but there, up to a whole sentence typed without a space.
    pub join_continue: f64,
    /// After a run, the chance of another one in the same sentence, and how many at most.
    pub join_more: f64,
    pub join_runs: usize,
    /// Chance that a run also gets a slip of its own among its keys ("quanhej" with a swapped pair, a missing tone...).
    pub join_slip: f64,
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
                // The writer's own kinds (AUGMENT_RULES.md, B; the weights are guesses until calibrated).
                (Slip::Slide, 3.0),
                (Slip::Multi, 3.0),
                (Slip::Held, 4.0),
            ],
            notice: 0.3,
            undo: 0.7,
            wrap: 0.02,
            join: 0.15,
            join_max: 12,
            join_continue: 0.55,
            join_more: 0.3,
            join_runs: 3,
            join_slip: 0.25,
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

    /// Where `slipped` is `keys` with a block of `len` keys put in: the place, or `None`.
    fn block_at(keys: &[char], slipped: &[char], len: usize) -> Vec<usize> {
        (0..=keys.len()).filter(|&i| slipped[..i] == keys[..i] && slipped[i + len..] == keys[i..]).collect()
    }

    #[test]
    fn the_writers_kinds_of_slip_stay_on_the_keyboard() {
        let mut rng = Rng::new(3);
        let keys: Vec<char> = "thuowngf".chars().collect();
        for _ in 0..300 {
            // a key held: one key three or four times in a row
            let held = Slip::Held.apply(&keys, true, &mut rng).unwrap();
            assert!((keys.len() + 2..=keys.len() + 3).contains(&held.len()));
            assert!(!block_at(&keys, &held, held.len() - keys.len()).is_empty());

            // a slide: two or three keys put in at one place, each next to a key beside that place or to the one before
            let slid = Slip::Slide.apply(&keys, true, &mut rng).unwrap();
            let len = slid.len() - keys.len();
            assert!((2..=3).contains(&len));
            let ok = block_at(&keys, &slid, len).into_iter().any(|i| {
                let mut previous: Option<char> = None;
                slid[i..i + len].iter().all(|&c| {
                    let mut allowed = Vec::new();
                    for side in [i.checked_sub(1).and_then(|j| keys.get(j)), keys.get(i)].into_iter().flatten() {
                        allowed.extend(neighbours(*side));
                    }
                    allowed.extend(previous.map(neighbours).unwrap_or_default());
                    previous = Some(c);
                    allowed.contains(&c)
                })
            });
            assert!(ok, "{keys:?} -> {slid:?}");

            // two keys at once: both next to the key they are put around
            let both = Slip::Multi.apply(&keys, true, &mut rng).unwrap();
            assert_eq!(both.len(), keys.len() + 2);
            let ok = (0..both.len()).any(|p| {
                (p + 1..both.len()).any(|q| {
                    // the two inserted keys at p and q; the other keys, in order, must be the ones typed
                    let rest: Vec<(usize, char)> = both.iter().copied().enumerate().filter(|(x, _)| *x != p && *x != q).collect();
                    rest.iter().map(|(_, c)| *c).eq(keys.iter().copied())
                        && rest.iter().any(|&(at, key)| {
                            let near = neighbours(key);
                            [p, q].iter().all(|&x| near.contains(&both[x]) && x.abs_diff(at) <= 2 && x != at)
                        })
                })
            });
            assert!(ok, "{keys:?} -> {both:?}");
        }
    }

    #[test]
    fn a_neighbour_slip_uses_the_writers_habits_for_t() {
        let mut rng = Rng::new(11);
        let keys: Vec<char> = "tt".chars().collect();
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..400 {
            let slipped = Slip::Neighbour.apply(&keys, false, &mut rng).unwrap();
            seen.extend(slipped.into_iter().filter(|c| *c != 't'));
        }
        // the geometry (r y 5 6 f g) and the writer's own list (e u h)
        for c in "ry56fgeuh".chars() {
            assert!(seen.contains(&c), "{c} missing from {seen:?}");
        }
    }
}
