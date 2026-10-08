//! The writer's keyboard: which key a finger hits instead of the one it aimed at.
//!
//! The table is `tools/kaggle/layouts/dell_vostro_3405.json`, made by `tools/kaggle/keyboard.py` from a photo of the
//! writer's own keyboard (a Dell Vostro 3405: US, ANSI, island keys): each key has the start and end of its footprint, in
//! key widths (x) and rows (y), and two keys touch when their footprints meet side by side or one above the other and
//! share at least `min_overlap` of a key. The Python and this code compute the neighbours the same way, and the tests
//! here give the sets the Python tests expect. The table also holds what the writer says about particular keys (the key
//! `t` taken for r, y, g and, more, e, u, f, h): the habits.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::Rng;

const EPS: f64 = 1e-9;

struct Key {
    name: String,
    y0: f64,
    y1: f64,
    x0: f64,
    x1: f64,
}

pub struct Keyboard {
    adjacent: HashMap<String, Vec<String>>,
    /// key -> (keys also taken for it, of which heavier)
    habits: HashMap<char, (Vec<char>, Vec<char>)>,
}

static LAPTOP: OnceLock<Keyboard> = OnceLock::new();

/// The writer's laptop keyboard.
pub fn laptop() -> &'static Keyboard {
    LAPTOP.get_or_init(|| Keyboard::parse(include_str!("../../../tools/kaggle/layouts/dell_vostro_3405.json")).expect("keyboard layout"))
}

fn overlap(a0: f64, a1: f64, b0: f64, b1: f64) -> f64 {
    a1.min(b1) - a0.max(b0)
}

impl Keyboard {
    pub fn parse(json: &str) -> Result<Self, String> {
        let doc: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let min_overlap = doc["min_overlap"].as_f64().ok_or("min_overlap")?;
        let mut keys = Vec::new();
        for k in doc["keys"].as_array().ok_or("keys")? {
            let num = |name: &str| k[name].as_f64().ok_or(format!("key field {name}"));
            keys.push(Key { name: k["name"].as_str().ok_or("name")?.to_string(), y0: num("y0")?, y1: num("y1")?, x0: num("x0")?, x1: num("x1")? });
        }
        let mut adjacent = HashMap::new();
        for a in &keys {
            let near: Vec<String> = keys
                .iter()
                .filter(|b| b.name != a.name)
                .filter(|b| {
                    let beside = (b.x0 - a.x1).abs() < EPS || (b.x1 - a.x0).abs() < EPS;
                    let stacked = (b.y0 - a.y1).abs() < EPS || (b.y1 - a.y0).abs() < EPS;
                    (beside && overlap(a.y0, a.y1, b.y0, b.y1) >= min_overlap - EPS) || (stacked && overlap(a.x0, a.x1, b.x0, b.x1) >= min_overlap - EPS)
                })
                .map(|b| b.name.clone())
                .collect();
            adjacent.insert(a.name.clone(), near);
        }
        let mut habits = HashMap::new();
        if let Some(map) = doc["habits"].as_object() {
            for (key, v) in map {
                let chars = |name: &str| v[name].as_str().unwrap_or("").chars().collect::<Vec<char>>();
                if let Some(c) = key.chars().next() {
                    habits.insert(c, (chars("partners"), chars("heavy")));
                }
            }
        }
        Ok(Self { adjacent, habits })
    }

    /// Every key next to the key named `key` (modifiers and punctuation included).
    pub fn adjacent(&self, key: &str) -> &[String] {
        self.adjacent.get(key).map_or(&[], Vec::as_slice)
    }

    /// The letters and digits typed by the keys next to the key of `c`, with the case of `c` (inside a word, a slip
    /// puts one of these; Space, punctuation, Shift and Caps Lock act differently and are not candidates here).
    pub fn neighbours(&self, c: char) -> Vec<char> {
        let key = c.to_ascii_lowercase().to_string();
        let mut out: Vec<char> = self
            .adjacent(&key)
            .iter()
            .filter_map(|name| {
                let mut it = name.chars();
                match (it.next(), it.next()) {
                    (Some(n), None) if n.is_ascii_alphanumeric() => Some(n),
                    _ => None,
                }
            })
            .collect();
        out.sort_unstable();
        if c.is_ascii_uppercase() {
            out.iter_mut().for_each(|n| *n = n.to_ascii_uppercase());
        }
        out
    }

    /// The keys the finger may hit instead of `c`, each with a weight: the keys next to it (1), and the writer's own
    /// habits for it (1, and 2 for the heavier ones).
    pub fn mistakes_for(&self, c: char) -> Vec<(char, u64)> {
        let lower = c.to_ascii_lowercase();
        let mut out: Vec<(char, u64)> = self.neighbours(lower).into_iter().map(|n| (n, 1)).collect();
        if let Some((partners, heavy)) = self.habits.get(&lower) {
            for &p in partners {
                let weight = if heavy.contains(&p) { 2 } else { 1 };
                match out.iter_mut().find(|(n, _)| *n == p) {
                    Some(entry) => entry.1 = entry.1.max(weight),
                    None => out.push((p, weight)),
                }
            }
        }
        if c.is_ascii_uppercase() {
            out.iter_mut().for_each(|(n, _)| *n = n.to_ascii_uppercase());
        }
        out
    }

    /// One key hit instead of `c`, drawn by the weights of [`Keyboard::mistakes_for`].
    pub fn mistake_for(&self, c: char, rng: &mut Rng) -> Option<char> {
        let options = self.mistakes_for(c);
        let total: u64 = options.iter().map(|(_, w)| w).sum();
        if total == 0 {
            return None;
        }
        let mut x = rng.below(total as usize) as u64;
        options
            .iter()
            .find(|(_, w)| {
                if x < *w {
                    true
                } else {
                    x -= w;
                    false
                }
            })
            .map(|(n, _)| *n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(s: &str) -> Vec<char> {
        let mut v: Vec<char> = s.chars().collect();
        v.sort_unstable();
        v
    }

    /// The sets the Python tests (tools/kaggle/test_keyboard.py) expect.
    #[test]
    fn neighbours_agree_with_the_python_layout() {
        let kb = laptop();
        assert_eq!(kb.neighbours('t'), set("ry56fg"));
        assert_eq!(kb.neighbours('a'), set("qwsz"));
        assert_eq!(kb.neighbours('f'), set("rtdgcv"));
        assert_eq!(kb.neighbours('m'), set("jkn"));
        assert_eq!(kb.neighbours('q'), set("12wa"));
        for c in "kop".chars() {
            assert!(kb.neighbours('l').contains(&c), "{c}");
        }
    }

    #[test]
    fn the_case_of_the_key_is_kept() {
        assert_eq!(laptop().neighbours('A'), set("QWSZ"));
    }

    #[test]
    fn wide_keys_touch_what_they_cover() {
        let kb = laptop();
        let space: Vec<&str> = kb.adjacent("space").iter().map(String::as_str).collect();
        for key in ["b", "c", "m", "n", "v", "lalt", "ralt"] {
            assert!(space.contains(&key), "{key}");
        }
        assert!(!space.contains(&",") && !space.contains(&"x"));
        let shift = kb.adjacent("lshift");
        assert!(["z", "a", "caps"].iter().all(|k| shift.iter().any(|s| s == k)));
        assert!(["pgup", "up", "pgdn"].iter().all(|k| kb.adjacent("rshift").iter().any(|s| s == k)));
    }

    #[test]
    fn adjacency_is_mutual_and_every_key_has_some() {
        let kb = laptop();
        for (key, near) in &kb.adjacent {
            assert!(near.len() >= 2, "{key}");
            for other in near {
                assert!(kb.adjacent(other).contains(key), "{key} {other}");
            }
        }
    }

    #[test]
    fn the_writers_habit_for_t_is_added_and_the_heavy_keys_are_likelier() {
        let kb = laptop();
        let options = kb.mistakes_for('t');
        for c in "rygeufh56".chars() {
            assert!(options.iter().any(|(n, _)| *n == c), "{c}");
        }
        assert_eq!(options.iter().find(|(n, _)| *n == 'e').unwrap().1, 2);
        assert_eq!(options.iter().find(|(n, _)| *n == 'r').unwrap().1, 1);
        let mut rng = Rng::new(7);
        let (mut e, mut r) = (0, 0);
        for _ in 0..6000 {
            match kb.mistake_for('t', &mut rng) {
                Some('e') => e += 1,
                Some('r') => r += 1,
                _ => {}
            }
        }
        assert!(e as f64 > 1.5 * r as f64, "e {e} r {r}");
        // a key with no habit: the geometry alone
        assert_eq!(kb.mistakes_for('m').len(), 3);
        // the case follows the key
        assert!(kb.mistakes_for('T').iter().all(|(n, _)| n.is_ascii_uppercase() || n.is_ascii_digit()));
    }
}
