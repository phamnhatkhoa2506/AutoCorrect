//! Vietnamese syllable = initial consonant + vowel nucleus + coda (+ tone).
//!
//! Characters here are lowercase and toneless ("đươc"); the tone is carried
//! separately and placed on a nucleus vowel by [`tone_index`].

use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Sac,
    Huyen,
    Hoi,
    Nga,
    Nang,
}

/// `Complete`: a finished word. `Prefix`: could still become one with more keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Complete,
    Prefix,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parts {
    pub initial: Range<usize>,
    pub nucleus: Range<usize>,
    pub coda: Range<usize>,
}

const VOWELS: &str = "aăâeêioôơuưy";

pub fn is_vowel(c: char) -> bool {
    VOWELS.contains(c)
}

/// Longest first, so "ngh" wins over "ng" over "n".
const INITIALS: &[&str] = &[
    "ngh", "ng", "nh", "gh", "gi", "kh", "ph", "th", "tr", "ch", "qu", "b", "c", "d", "đ", "g",
    "h", "k", "l", "m", "n", "p", "r", "s", "t", "v", "x",
];

const CODAS: &[&str] = &["c", "ch", "m", "n", "ng", "nh", "p", "t"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum CodaRule {
    Optional,
    Required,
    Forbidden,
}

use CodaRule::{Forbidden, Optional, Required};

/// Every vowel nucleus Vietnamese spelling allows, with its coda rule.
const NUCLEI: &[(&str, CodaRule)] = &[
    ("a", Optional), ("ă", Required), ("â", Required), ("e", Optional), ("ê", Optional),
    ("i", Optional), ("o", Optional), ("ô", Optional), ("ơ", Optional), ("u", Optional),
    ("ư", Optional), ("y", Forbidden),
    ("ai", Forbidden), ("ao", Forbidden), ("au", Forbidden), ("ay", Forbidden),
    ("âu", Forbidden), ("ây", Forbidden), ("eo", Forbidden), ("êu", Forbidden),
    ("ia", Forbidden), ("iê", Required), ("iu", Forbidden), ("oa", Optional),
    ("oă", Required), ("oe", Optional), ("oi", Forbidden), ("oo", Required),
    ("ôi", Forbidden), ("ơi", Forbidden), ("ua", Forbidden), ("uâ", Required),
    ("uê", Optional), ("ui", Forbidden), ("uô", Required), ("uơ", Forbidden),
    ("uy", Optional), ("ưa", Forbidden), ("ưi", Forbidden), ("ươ", Required),
    ("ưu", Forbidden), ("yê", Required),
    ("oai", Forbidden), ("oay", Forbidden), ("oeo", Forbidden), ("uây", Forbidden),
    ("uôi", Forbidden), ("uya", Forbidden), ("uyê", Required), ("uyu", Forbidden),
    ("ươi", Forbidden), ("ươu", Forbidden), ("iêu", Forbidden), ("yêu", Forbidden),
];

/// Nuclei that may precede the codas "ch" / "nh".
const CH_NH_NUCLEI: &[&str] = &["a", "ê", "i", "y", "oa", "uê", "uy"];

/// Length in chars of the initial consonant. "gi"/"qu" count only when a
/// vowel follows ("gì" is g + i, "qua" is qu + a).
pub fn initial_len(chars: &[char]) -> usize {
    for init in INITIALS {
        // Hot path of the corrector: compare without allocating.
        let len = init.chars().count();
        if chars.len() < len || !init.chars().zip(chars).all(|(a, &b)| a == b) {
            continue;
        }
        if matches!(*init, "gi" | "qu") && !chars.get(2).is_some_and(|&c| is_vowel(c)) {
            continue;
        }
        return len;
    }
    0
}

/// The vowel run right after the initial consonant.
pub fn nucleus(chars: &[char]) -> Range<usize> {
    let start = initial_len(chars);
    let len = chars[start..].iter().take_while(|&&c| is_vowel(c)).count();
    start..start + len
}

/// Parses `chars` (lowercase, toneless) and checks spelling rules.
pub fn check(chars: &[char], tone: Option<Tone>, mode: Mode) -> Option<Parts> {
    let text: String = chars.iter().collect();
    if mode == Mode::Prefix {
        // Consonants only, or "qu" waiting for its vowel.
        if text == "qu" || chars.iter().all(|&c| !is_vowel(c)) {
            let ok = INITIALS.iter().any(|i| i.starts_with(text.as_str())) && tone.is_none();
            return ok.then_some(Parts { initial: 0..chars.len(), nucleus: 0..0, coda: 0..0 });
        }
    }

    let nuc = nucleus(chars);
    if nuc.is_empty() {
        return None;
    }
    let initial: String = chars[..nuc.start].iter().collect();
    let nuc_str: String = chars[nuc.clone()].iter().collect();
    let coda: String = chars[nuc.end..].iter().collect();

    if !initial.is_empty() && !INITIALS.contains(&initial.as_str()) {
        return None;
    }
    // Spelling: k/gh/ngh only before e ê i y; c/ng never before them; g is
    // fine before i ("gì" = g + i) but not before e ê y (that is "gh").
    let first = chars[nuc.start];
    let front = matches!(first, 'e' | 'ê' | 'i' | 'y');
    match initial.as_str() {
        "k" | "gh" | "ngh" if !front => return None,
        "c" | "ng" if front => return None,
        "g" if matches!(first, 'e' | 'ê' | 'y') => return None,
        _ => {}
    }
    // "yê…" starts a syllable on its own (yên) or after "qu" (quyên).
    if nuc_str.starts_with('y') && nuc_str.chars().count() > 1 && !matches!(initial.as_str(), "" | "qu") {
        return None;
    }

    let rule = match NUCLEI.iter().find(|(n, _)| *n == nuc_str) {
        Some((_, rule)) => Some(*rule),
        None if mode == Mode::Prefix => NUCLEI
            .iter()
            .any(|(n, _)| n.starts_with(nuc_str.as_str()))
            .then_some(Optional),
        None => None,
    }?;

    if coda.is_empty() {
        if rule == Required && mode == Mode::Complete {
            return None;
        }
    } else {
        if rule == Forbidden {
            return None;
        }
        let known = match mode {
            Mode::Complete => CODAS.contains(&coda.as_str()),
            Mode::Prefix => CODAS.iter().any(|c| c.starts_with(coda.as_str())),
        };
        if !known {
            return None;
        }
        if matches!(coda.as_str(), "ch" | "nh") && !CH_NH_NUCLEI.contains(&nuc_str.as_str()) {
            return None;
        }
    }

    // Stop codas only take sắc or nặng (and need one once the word is done).
    if matches!(coda.as_str(), "c" | "ch" | "p" | "t") {
        match tone {
            Some(Tone::Sac | Tone::Nang) => {}
            None if mode == Mode::Prefix => {}
            _ => return None,
        }
    }

    Some(Parts { initial: 0..nuc.start, nucleus: nuc.clone(), coda: nuc.end..chars.len() })
}

/// Index of the vowel that carries the tone (traditional style: "hòa", "thủy").
pub fn tone_index(chars: &[char], parts: &Parts) -> usize {
    let start = parts.nucleus.start;
    let nuc = &chars[parts.nucleus.clone()];
    if let Some(i) = nuc.iter().rposition(|c| "ăâêôơư".contains(*c)) {
        return start + i;
    }
    match nuc.len() {
        1 => start,
        _ if !parts.coda.is_empty() => start + nuc.len() - 1,
        3 => start + 1,
        _ => start,
    }
}

const TONE_TABLE: &[(char, [char; 5])] = &[
    ('a', ['á', 'à', 'ả', 'ã', 'ạ']),
    ('ă', ['ắ', 'ằ', 'ẳ', 'ẵ', 'ặ']),
    ('â', ['ấ', 'ầ', 'ẩ', 'ẫ', 'ậ']),
    ('e', ['é', 'è', 'ẻ', 'ẽ', 'ẹ']),
    ('ê', ['ế', 'ề', 'ể', 'ễ', 'ệ']),
    ('i', ['í', 'ì', 'ỉ', 'ĩ', 'ị']),
    ('o', ['ó', 'ò', 'ỏ', 'õ', 'ọ']),
    ('ô', ['ố', 'ồ', 'ổ', 'ỗ', 'ộ']),
    ('ơ', ['ớ', 'ờ', 'ở', 'ỡ', 'ợ']),
    ('u', ['ú', 'ù', 'ủ', 'ũ', 'ụ']),
    ('ư', ['ứ', 'ừ', 'ử', 'ữ', 'ự']),
    ('y', ['ý', 'ỳ', 'ỷ', 'ỹ', 'ỵ']),
];

const TONES: [Tone; 5] = [Tone::Sac, Tone::Huyen, Tone::Hoi, Tone::Nga, Tone::Nang];

pub fn with_tone(c: char, tone: Tone) -> char {
    let i = TONES.iter().position(|&t| t == tone).unwrap_or(0);
    TONE_TABLE.iter().find(|(base, _)| *base == c).map_or(c, |(_, forms)| forms[i])
}

/// Splits a toned lowercase vowel into its toneless form and tone.
pub fn split_tone(c: char) -> (char, Option<Tone>) {
    for (base, forms) in TONE_TABLE {
        if let Some(i) = forms.iter().position(|&f| f == c) {
            return (*base, Some(TONES[i]));
        }
    }
    (c, None)
}

/// Lowercase toneless chars and the tone of `word`; `None` if it carries
/// more than one tone mark.
fn untone(word: &str) -> Option<(Vec<char>, Option<Tone>)> {
    let mut tone = None;
    let mut chars = Vec::new();
    for c in word.chars().flat_map(char::to_lowercase) {
        let (base, t) = split_tone(c);
        if t.is_some() {
            if tone.is_some() {
                return None;
            }
            tone = t;
        }
        chars.push(base);
    }
    Some((chars, tone))
}

/// True if `word` is a correctly spelled Vietnamese syllable (either tone
/// placement style is accepted).
pub fn is_valid_word(word: &str) -> bool {
    untone(word).is_some_and(|(chars, tone)| check(&chars, tone, Mode::Complete).is_some())
}

/// Lowercase form with the tone on the traditional vowel ("Hoà" -> "hòa"),
/// matching what the Telex composer produces; `None` if not a valid syllable.
/// Expects precomposed (NFC) input.
pub fn canonical(word: &str) -> Option<String> {
    let (chars, tone) = untone(word)?;
    let parts = check(&chars, tone, Mode::Complete)?;
    let at = tone.map(|t| (tone_index(&chars, &parts), t));
    Some(
        chars
            .iter()
            .enumerate()
            .map(|(i, &c)| match at {
                Some((pos, t)) if pos == i => with_tone(c, t),
                _ => c,
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_words() {
        for w in [
            "được", "tiếng", "việt", "nguyễn", "hòa", "hoà", "thủy", "thuỷ", "mưa", "gì", "giữ",
            "quốc", "của", "không", "khuya", "thuở", "người", "nghiêng", "ếch", "quyền", "yêu",
            "Đi", "ăn", "ấy", "khuếch", "xoong", "các", "học", "gìn", "ịch",
        ] {
            assert!(is_valid_word(w), "{w} should be valid");
        }
    }

    #[test]
    fn invalid_words() {
        for w in [
            "dunhf", "teh", "hello", "ka", "ge", "nge", "cac", "càc", "ă", "tiê", "aiu", "anhh",
            "tyên", "fan", "wa", "ácc", "áá",
        ] {
            assert!(!is_valid_word(w), "{w} should be invalid");
        }
    }

    #[test]
    fn canonical_form() {
        assert_eq!(canonical("Hoà").as_deref(), Some("hòa"));
        assert_eq!(canonical("hòa").as_deref(), Some("hòa"));
        assert_eq!(canonical("THUỶ").as_deref(), Some("thủy"));
        assert_eq!(canonical("được").as_deref(), Some("được"));
        assert_eq!(canonical("hello"), None);
    }

    #[test]
    fn prefixes() {
        let p = |s: &str| check(&s.chars().collect::<Vec<_>>(), None, Mode::Prefix).is_some();
        assert!(p("ng"));
        assert!(p("ngh"));
        assert!(p("qu"));
        assert!(p("tiê"));
        assert!(p("ă"));
        assert!(p("tiên"));
        assert!(!p("tz"));
        assert!(!p("hel"));
    }

    #[test]
    fn tone_position() {
        let pos = |s: &str| {
            let c: Vec<char> = s.chars().collect();
            let parts = check(&c, None, Mode::Prefix).unwrap();
            tone_index(&c, &parts)
        };
        assert_eq!(pos("hoa"), 1); // hòa
        assert_eq!(pos("hoan"), 2); // hoàn
        assert_eq!(pos("đươc"), 2); // được
        assert_eq!(pos("thuy"), 2); // thủy
        assert_eq!(pos("ngoai"), 3); // ngoài
        assert_eq!(pos("gi"), 1); // gì
        assert_eq!(pos("giư"), 2); // giữ
        assert_eq!(pos("qua"), 2); // quá
    }
}
