//! The VNI input method, by translation to Telex keys: digits become the
//! Telex keys that do the same thing, and everything else (composition,
//! spelling checks, the corrector) is shared with Telex.
//!
//! VNI: 1 sắc, 2 huyền, 3 hỏi, 4 ngã, 5 nặng, 6 â ê ô, 7 ơ ư, 8 ă, 9 đ.

use crate::syllable::{split_tone, Tone};
use crate::telex::{compose, Composition, Kind};

fn is_vowel(c: char) -> bool {
    matches!(c.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u' | 'y')
}

/// Telex keys with the same effect as the VNI keys `raw`. A digit that has
/// nothing to act on (a tone with no vowel, "7" with no o or u...) stays a
/// digit, so numbers and codes are left alone.
pub fn to_telex(raw: &str) -> String {
    let mut out: Vec<char> = Vec::new();
    for c in raw.chars() {
        match c {
            '1'..='5' if out.iter().any(|&k| is_vowel(k)) => {
                out.push(['s', 'f', 'r', 'x', 'j'][c as usize - '1' as usize]);
            }
            '6' => match out.iter().rposition(|&k| matches!(k.to_ascii_lowercase(), 'a' | 'e' | 'o')) {
                Some(i) => out.insert(i + 1, out[i]),
                None => out.push(c),
            },
            '7' if out.iter().any(|&k| matches!(k.to_ascii_lowercase(), 'o' | 'u')) => out.push('w'),
            '8' if out.iter().any(|&k| k.eq_ignore_ascii_case(&'a')) => out.push('w'),
            '9' if out.first().is_some_and(|k| k.eq_ignore_ascii_case(&'d'))
                && out.get(1).map(|k| k.to_ascii_lowercase()) != Some('d') =>
            {
                out.insert(1, out[0]);
            }
            _ => out.push(c),
        }
    }
    out.into_iter().collect()
}

/// Composes the raw VNI keys of a single word. Unlike Telex, text that is not
/// Vietnamese shows the keys as typed, digits included ("Win10").
pub fn compose_vni(raw: &str) -> Composition {
    let composed = compose(&to_telex(raw));
    match composed.kind {
        Kind::Literal => Composition { text: raw.to_string(), kind: Kind::Literal },
        _ => composed,
    }
}

/// VNI keys that type `text` ("Việt" -> "Vie6t5"): marks inline, tone last.
pub fn to_keys(text: &str) -> String {
    let mut keys = String::new();
    let mut tone_key = None;
    for c in text.chars() {
        let upper = c.is_uppercase();
        let (base, tone) = split_tone(c.to_lowercase().next().unwrap_or(c));
        if let Some(t) = tone {
            tone_key = Some(match t {
                Tone::Sac => '1',
                Tone::Huyen => '2',
                Tone::Hoi => '3',
                Tone::Nga => '4',
                Tone::Nang => '5',
            });
        }
        let (letter, mark) = match base {
            'â' => ('a', Some('6')),
            'ă' => ('a', Some('8')),
            'ê' => ('e', Some('6')),
            'ô' => ('o', Some('6')),
            'ơ' => ('o', Some('7')),
            'ư' => ('u', Some('7')),
            'đ' => ('d', Some('9')),
            other => (other, None),
        };
        if upper {
            keys.extend(letter.to_uppercase());
        } else {
            keys.push(letter);
        }
        keys.extend(mark);
    }
    keys.extend(tone_key);
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shows(raw: &str) -> String {
        compose_vni(raw).text
    }

    #[test]
    fn composes_vietnamese() {
        assert_eq!(shows("Vie6t5"), "Việt");
        assert_eq!(shows("tie6ng"), "tiêng");
        assert_eq!(shows("tie6ng1"), "tiếng");
        assert_eq!(shows("d9u7o7ng2"), "đường");
        assert_eq!(shows("to6i"), "tôi");
        assert_eq!(shows("toi6"), "tôi");
        assert_eq!(shows("a8"), "ă");
        assert_eq!(shows("d9i"), "đi");
        assert_eq!(shows("khong6"), "không");
    }

    #[test]
    fn digits_with_nothing_to_act_on_stay() {
        assert_eq!(shows("2024"), "2024");
        assert_eq!(shows("mp3"), "mp3");
        assert_eq!(shows("h2o"), "h2o");
        assert_eq!(shows("b6"), "b6");
    }

    #[test]
    fn not_vietnamese_shows_the_keys_as_typed() {
        assert_eq!(shows("Win10"), "Win10");
        assert_eq!(shows("hello"), "hello");
    }

    #[test]
    fn keys_round_trip() {
        for word in ["Việt", "tiếng", "đường", "người", "không", "Đà", "ă"] {
            assert_eq!(shows(&to_keys(word)), word, "{word}");
        }
    }
}
