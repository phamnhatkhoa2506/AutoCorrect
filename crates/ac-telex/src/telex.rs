//! Telex input method: raw keystrokes of one word -> Vietnamese text.
//!
//! Recomputed from the full key sequence on every key, so marks and tones can
//! be typed in any order ("tieengs", "tiesng" and "tieeng" + "s" all agree).

use crate::syllable::{check, nucleus, split_tone, tone_index, with_tone, Mode, Tone};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mark {
    None,
    Hat,
    Breve,
    Horn,
    Stroke,
}

#[derive(Debug, Clone, Copy)]
struct Letter {
    base: char,
    mark: Mark,
    upper: bool,
}

impl Letter {
    fn shape(self) -> char {
        match (self.base, self.mark) {
            ('a', Mark::Breve) => 'ă',
            ('a', Mark::Hat) => 'â',
            ('e', Mark::Hat) => 'ê',
            ('o', Mark::Hat) => 'ô',
            ('o', Mark::Horn) => 'ơ',
            ('u', Mark::Horn) => 'ư',
            ('d', Mark::Stroke) => 'đ',
            (base, _) => base,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A complete, correctly spelled Vietnamese syllable.
    Vietnamese,
    /// Not finished but could still become one ("tiê", "ă", "ngh").
    Partial,
    /// Not Vietnamese: the raw keys, or literal text after a repeated key
    /// cancelled a mark ("ass" -> "as").
    Literal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Composition {
    pub text: String,
    pub kind: Kind,
}

/// Telex keys that type `text` ("Việt" -> "Vieetj"), the inverse of
/// [`compose`] for Vietnamese words. The tone key goes last.
pub fn to_keys(text: &str) -> String {
    let mut keys = String::new();
    let mut tone_key = None;
    for c in text.chars() {
        let upper = c.is_uppercase();
        let (base, tone) = split_tone(c.to_lowercase().next().unwrap_or(c));
        if let Some(t) = tone {
            tone_key = Some(match t {
                Tone::Sac => 's',
                Tone::Huyen => 'f',
                Tone::Hoi => 'r',
                Tone::Nga => 'x',
                Tone::Nang => 'j',
            });
        }
        let spelled = match base {
            'â' => "aa",
            'ă' => "aw",
            'ê' => "ee",
            'ô' => "oo",
            'ơ' => "ow",
            'ư' => "uw",
            'đ' => "dd",
            _ => {
                if upper {
                    keys.extend(base.to_uppercase());
                } else {
                    keys.push(base);
                }
                continue;
            }
        };
        let mut chars = spelled.chars();
        if let Some(first) = chars.next() {
            if upper {
                keys.extend(first.to_uppercase());
            } else {
                keys.push(first);
            }
        }
        keys.extend(chars);
    }
    keys.extend(tone_key);
    keys
}

/// Composes the raw Telex keys of a single word.
pub fn compose(raw: &str) -> Composition {
    let mut state = State::default();
    for key in raw.chars() {
        state.press(key);
    }
    state.finish(raw)
}

#[derive(Default)]
struct State {
    letters: Vec<Letter>,
    tone: Option<Tone>,
    /// A repeated key cancelled a transformation: the rest is typed literally.
    reverted: bool,
}

impl State {
    fn press(&mut self, key: char) {
        if self.reverted || !key.is_ascii_alphabetic() {
            self.push(key);
            return;
        }
        let handled = match key.to_ascii_lowercase() {
            's' => self.tone_key(Tone::Sac, key),
            'f' => self.tone_key(Tone::Huyen, key),
            'r' => self.tone_key(Tone::Hoi, key),
            'x' => self.tone_key(Tone::Nga, key),
            'j' => self.tone_key(Tone::Nang, key),
            'z' => self.tone.take().is_some(),
            c @ ('a' | 'e' | 'o') => self.hat(c, key),
            'w' => self.horn_or_breve(key),
            'd' => self.stroke(key),
            _ => false,
        };
        if !handled {
            self.push(key);
        }
    }

    fn push(&mut self, key: char) {
        self.letters.push(Letter {
            base: key.to_lowercase().next().unwrap_or(key),
            mark: Mark::None,
            upper: key.is_uppercase(),
        });
    }

    /// Appends `key` literally and stops transforming this word.
    fn revert_with(&mut self, key: char) {
        self.push(key);
        self.reverted = true;
    }

    fn shapes(&self) -> Vec<char> {
        self.letters.iter().map(|l| l.shape()).collect()
    }

    fn nucleus(&self) -> std::ops::Range<usize> {
        nucleus(&self.shapes())
    }

    fn tone_key(&mut self, tone: Tone, key: char) -> bool {
        if self.nucleus().is_empty() {
            return false; // no vowel yet: it is a letter
        }
        if self.tone == Some(tone) {
            self.tone = None;
            self.revert_with(key);
        } else {
            self.tone = Some(tone);
        }
        true
    }

    /// aa -> â, ee -> ê, oo -> ô (on the nucleus vowel, not just the last key).
    fn hat(&mut self, base: char, key: char) -> bool {
        let Some(i) = self.nucleus().rev().find(|&i| self.letters[i].base == base) else {
            return false;
        };
        if self.letters[i].mark == Mark::Hat {
            self.letters[i].mark = Mark::None;
            self.revert_with(key);
        } else {
            self.letters[i].mark = Mark::Hat;
        }
        true
    }

    /// w: uo -> ươ, oa -> oă, u -> ư, o -> ơ, a -> ă; alone -> ư.
    fn horn_or_breve(&mut self, key: char) -> bool {
        let nuc: Vec<usize> = self.nucleus().collect();
        if nuc.is_empty() {
            self.letters.push(Letter { base: 'u', mark: Mark::Horn, upper: key.is_uppercase() });
            return true;
        }
        let is = |l: &Letter, base: char| l.base == base && l.mark == Mark::None;
        let pair = |a: char, b: char| {
            nuc.windows(2)
                .find(|w| is(&self.letters[w[0]], a) && is(&self.letters[w[1]], b))
                .map(|w| (w[0], w[1]))
        };

        if let Some((u, o)) = pair('u', 'o') {
            self.letters[u].mark = Mark::Horn;
            self.letters[o].mark = Mark::Horn;
        } else if let Some((_, a)) = pair('o', 'a') {
            self.letters[a].mark = Mark::Breve;
        } else if let Some(&i) = nuc.iter().find(|&&i| is(&self.letters[i], 'u') || is(&self.letters[i], 'o')) {
            self.letters[i].mark = Mark::Horn;
        } else if let Some(&i) = nuc.iter().find(|&&i| is(&self.letters[i], 'a')) {
            self.letters[i].mark = Mark::Breve;
        } else if nuc.iter().any(|&i| matches!(self.letters[i].mark, Mark::Horn | Mark::Breve)) {
            for &i in &nuc {
                if matches!(self.letters[i].mark, Mark::Horn | Mark::Breve) {
                    self.letters[i].mark = Mark::None;
                }
            }
            self.revert_with(key);
        } else {
            return false;
        }
        true
    }

    /// dd -> đ (the second d may come later: "dung" + "d" -> "đung").
    fn stroke(&mut self, key: char) -> bool {
        let Some(first) = self.letters.first_mut().filter(|l| l.base == 'd') else {
            return false;
        };
        if first.mark == Mark::Stroke {
            first.mark = Mark::None;
            self.revert_with(key);
        } else {
            first.mark = Mark::Stroke;
        }
        true
    }

    fn finish(&self, raw: &str) -> Composition {
        let shapes = self.shapes();
        if self.reverted {
            return Composition { text: self.render(&shapes, None), kind: Kind::Literal };
        }
        let kind = if check(&shapes, self.tone, Mode::Complete).is_some() {
            Kind::Vietnamese
        } else if check(&shapes, self.tone, Mode::Prefix).is_some() {
            Kind::Partial
        } else {
            return Composition { text: raw.to_string(), kind: Kind::Literal };
        };
        let toned = self.tone.and_then(|tone| {
            let parts = check(&shapes, self.tone, Mode::Prefix)?;
            Some((tone_index(&shapes, &parts), tone))
        });
        Composition { text: self.render(&shapes, toned), kind }
    }

    fn render(&self, shapes: &[char], toned: Option<(usize, Tone)>) -> String {
        let mut out = String::new();
        for (i, (&c, letter)) in shapes.iter().zip(&self.letters).enumerate() {
            let c = match toned {
                Some((at, tone)) if at == i => with_tone(c, tone),
                _ => c,
            };
            if letter.upper {
                out.extend(c.to_uppercase());
            } else {
                out.push(c);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vn(raw: &str) -> String {
        let c = compose(raw);
        assert_eq!(c.kind, Kind::Vietnamese, "{raw} -> {c:?}");
        c.text
    }

    #[test]
    fn composes_words() {
        for (raw, want) in [
            ("dduowcj", "được"),
            ("tieengs", "tiếng"),
            ("tieesng", "tiếng"),
            ("tiengse", "tiếng"),
            ("vieetj", "việt"),
            ("nguyeenx", "nguyễn"),
            ("hoaf", "hòa"),
            ("thuyr", "thủy"),
            ("muwa", "mưa"),
            ("muaw", "mưa"),
            ("duongw", "dương"),
            ("trwowngf", "trường"),
            ("quawngs", "quắng"),
            ("gif", "gì"),
            ("giuwx", "giữ"),
            ("tooi", "tôi"),
            ("khoong", "không"),
            ("cuar", "của"),
            ("cacs", "các"),
            ("w", "ư"),
            ("the", "the"),
            ("dungf", "dùng"),
        ] {
            assert_eq!(vn(raw), want, "{raw}");
        }
    }

    #[test]
    fn keeps_case() {
        assert_eq!(vn("Ddi"), "Đi");
        assert_eq!(vn("Vieetj"), "Việt");
        assert_eq!(vn("DDuwowngf"), "Đường");
        assert_eq!(vn("VIEETJ"), "VIỆT");
    }

    #[test]
    fn repeated_key_cancels_mark() {
        for (raw, want) in [("ass", "as"), ("aaa", "aa"), ("tesst", "test"), ("uww", "uw"), ("ddd", "dd")] {
            assert_eq!(compose(raw), Composition { text: want.into(), kind: Kind::Literal }, "{raw}");
        }
    }

    #[test]
    fn partial_words() {
        assert_eq!(compose("aw").kind, Kind::Partial); // ă, waiting for a coda
        assert_eq!(compose("tiee").kind, Kind::Partial); // tiê
        assert_eq!(compose("cac").kind, Kind::Partial); // needs sắc/nặng
        assert_eq!(compose("ngh").kind, Kind::Partial);
    }

    #[test]
    fn to_keys_round_trips() {
        assert_eq!(to_keys("Việt"), "Vieetj");
        assert_eq!(to_keys("được"), "dduwowcj");
        assert_eq!(to_keys("Đường"), "Dduwowngf");
        for w in [
            "việt", "được", "tiếng", "nguyễn", "hòa", "thủy", "mưa", "gì", "giữ", "quốc", "của",
            "không", "khuya", "người", "hoặc", "quân", "Đi", "việ", "tiê", "ă", "the",
        ] {
            assert_eq!(compose(&to_keys(w)).text, w, "{w} via {}", to_keys(w));
        }
    }

    #[test]
    fn non_vietnamese_falls_back_to_raw_keys() {
        for raw in ["hello", "first", "dunhf", "cacf", "ka", "123"] {
            assert_eq!(compose(raw), Composition { text: raw.into(), kind: Kind::Literal }, "{raw}");
        }
    }
}
