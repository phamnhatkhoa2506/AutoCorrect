//! Keystroke-level typo model: every raw key sequence one slip away from what
//! was typed, with the cost (negative log-likelihood) of that slip.
//!
//! Costs are hand-set priors, to be fitted on real undo/correction data later.

use std::collections::HashMap;

pub const TRANSPOSE: f64 = 4.0;
/// Hitting a neighbouring key instead of the intended one.
pub const ADJACENT: f64 = 4.5;
/// Hỏi typed for ngã or the reverse, a very common spelling confusion.
pub const TONE_SWAP: f64 = 4.0;
/// Forgetting a Telex mark key (the second "o" of "ô", a "w", a tone).
pub const MISSING_MARK: f64 = 4.5;
/// Doubling or un-doubling a letter ("occured", "untill").
pub const DOUBLE: f64 = 3.5;
/// Spelling a vowel by ear ("seperate").
pub const VOWEL: f64 = 5.5;
pub const MISSING: f64 = 5.0;
pub const EXTRA: f64 = 4.5;

const TONE_KEYS: &str = "sfrxj";
const VOWELS: &str = "aeiouy";

/// Whether adding or dropping `c` only changes Telex marks: a tone key, "w",
/// or the doubled half of aa/ee/oo/dd. Dropping a lone "o" changes letters.
fn is_mark(c: char, doubled: bool) -> bool {
    TONE_KEYS.contains(c) || c == 'w' || (doubled && "aeod".contains(c))
}

/// QWERTY neighbours of each letter.
fn neighbours(c: char) -> &'static str {
    match c {
        'q' => "wa", 'w' => "qeas", 'e' => "wrsd", 'r' => "etdf", 't' => "ryfg",
        'y' => "tugh", 'u' => "yihj", 'i' => "uojk", 'o' => "ipkl", 'p' => "ol",
        'a' => "qwsz", 's' => "weadzx", 'd' => "erfsxc", 'f' => "rtdgcv", 'g' => "tyfhvb",
        'h' => "yugjbn", 'j' => "uihknm", 'k' => "ijolm", 'l' => "opk",
        'z' => "asx", 'x' => "zsdc", 'c' => "xdfv", 'v' => "cfgb", 'b' => "vghn",
        'n' => "bhjm", 'm' => "njk",
        _ => "",
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Slip {
    pub keys: String,
    pub cost: f64,
    /// Only Telex mark keys were added, dropped or swapped: the letters of
    /// the word are untouched.
    pub marks_only: bool,
}

/// All single-slip variants of `keys` (lowercase ASCII letters). Each
/// (variant, marks_only) pair appears once, with its cheapest cost.
pub fn edits1(keys: &str) -> Vec<Slip> {
    let k: Vec<char> = keys.chars().collect();
    let mut out: HashMap<(String, bool), f64> = HashMap::new();
    let mut add = |chars: Vec<char>, cost: f64, marks_only: bool| {
        let e = out.entry((chars.into_iter().collect(), marks_only)).or_insert(f64::INFINITY);
        *e = e.min(cost);
    };

    for i in 0..k.len() {
        // Extra key typed (cheaper when it doubled its neighbour).
        let mut v = k.clone();
        v.remove(i);
        let doubled = (i > 0 && k[i - 1] == k[i]) || k.get(i + 1) == Some(&k[i]);
        add(v, if doubled { DOUBLE } else { EXTRA }, is_mark(k[i], doubled));

        if i + 1 < k.len() && k[i] != k[i + 1] {
            let mut v = k.clone();
            v.swap(i, i + 1);
            add(v, TRANSPOSE, false);
        }

        for c in ('a'..='z').filter(|&c| c != k[i]) {
            let tones = TONE_KEYS.contains(k[i]) && TONE_KEYS.contains(c);
            let (cost, marks_only) = if tones && matches!((k[i], c), ('r', 'x') | ('x', 'r')) {
                (TONE_SWAP, true) // hỏi/ngã: a spelling confusion, not a slip
            } else if tones && neighbours(k[i]).contains(c) {
                (ADJACENT, true) // neighbouring tone keys: s/x, r/f
            } else if tones {
                continue; // j for s is not a plausible mistake
            } else if neighbours(k[i]).contains(c) {
                (ADJACENT, false)
            } else if VOWELS.contains(k[i]) && VOWELS.contains(c) {
                (VOWEL, false)
            } else {
                continue;
            };
            let mut v = k.clone();
            v[i] = c;
            add(v, cost, marks_only);
        }
    }
    // Missing key, anywhere (including the end).
    for i in 0..=k.len() {
        for c in 'a'..='z' {
            let doubled = (i > 0 && k[i - 1] == c) || k.get(i) == Some(&c);
            let mark = is_mark(c, doubled);
            let cost = if doubled {
                DOUBLE
            } else if mark {
                MISSING_MARK
            } else {
                MISSING
            };
            let mut v = k.clone();
            v.insert(i, c);
            add(v, cost, mark);
        }
    }
    out.into_iter()
        .filter(|((s, _), _)| s != keys)
        .map(|((keys, marks_only), cost)| Slip { keys, cost, marks_only })
        .collect()
}

/// Costliest two-slip correction accepted by [`slip_cost`]: two ordinary
/// slips fit, three never do (the cheapest three cost 10.5).
pub const FAR_MAX_COST: f64 = 10.4;

/// Longest target [`slip_cost`] handles.
const MAX_LEN: usize = 31;

/// Letter counts of a lowercase ASCII word, for [`bag_distance`].
pub fn letter_counts(word: &[u8]) -> [i8; 26] {
    let mut bag = [0i8; 26];
    for &b in word.iter().filter(|b| b.is_ascii_lowercase()) {
        bag[usize::from(b - b'a')] += 1;
    }
    bag
}

/// How many letters differ between two words, ignoring order. Two slips
/// change at most 4, so anything farther is skipped before the costly
/// alignment.
pub fn bag_distance(typed: &[i8; 26], target: &[u8]) -> usize {
    let mut bag = *typed;
    for &b in target.iter().filter(|b| b.is_ascii_lowercase()) {
        bag[usize::from(b - b'a')] -= 1;
    }
    bag.iter().map(|c| usize::from(c.unsigned_abs())).sum()
}

/// Cost of turning the keys `typed` into `target` with the same slip kinds as
/// [`edits1`] (transposition, neighbouring key, vowel for vowel, a missing,
/// extra or doubled key), or `None` when that costs more than `limit`.
/// Optimal string alignment: every slip is cheaper than starting over.
pub fn slip_cost(typed: &[u8], target: &[u8], limit: f64) -> Option<f64> {
    const IMPOSSIBLE: f64 = f64::INFINITY;
    let (n, m) = (typed.len(), target.len());
    if m > MAX_LEN {
        return None;
    }
    let is_vowel = |c: u8| b"aeiouy".contains(&c);
    let adjacent = |a: u8, b: u8| neighbours(a as char).contains(b as char);
    let sub = |a: u8, b: u8| {
        if a == b {
            0.0
        } else if adjacent(a, b) {
            ADJACENT
        } else if is_vowel(a) && is_vowel(b) {
            VOWEL
        } else {
            IMPOSSIBLE
        }
    };
    // Extra key (cheaper when it doubled its neighbour) and missing key.
    let extra = |i: usize| {
        let c = typed[i];
        if (i > 0 && typed[i - 1] == c) || typed.get(i + 1) == Some(&c) { DOUBLE } else { EXTRA }
    };
    let missing = |j: usize| {
        let c = target[j];
        if (j > 0 && target[j - 1] == c) || target.get(j + 1) == Some(&c) { DOUBLE } else { MISSING }
    };

    // d[i][j]: cost of typed[..i] -> target[..j]; rows i-2, i-1, i kept (on
    // the stack: this runs against every word of the lexicon).
    let mut prev2 = [IMPOSSIBLE; MAX_LEN + 1];
    let mut prev = [0.0; MAX_LEN + 1];
    for j in 1..=m {
        prev[j] = prev[j - 1] + missing(j - 1);
    }
    let mut cur = [0.0; MAX_LEN + 1];
    for i in 1..=n {
        cur[0] = prev[0] + extra(i - 1);
        let mut row_min = cur[0];
        for j in 1..=m {
            let mut best = (prev[j - 1] + sub(typed[i - 1], target[j - 1]))
                .min(prev[j] + extra(i - 1))
                .min(cur[j - 1] + missing(j - 1));
            if i > 1 && j > 1 && typed[i - 1] == target[j - 2] && typed[i - 2] == target[j - 1] && typed[i - 1] != typed[i - 2] {
                best = best.min(prev2[j - 2] + TRANSPOSE);
            }
            cur[j] = best;
            row_min = row_min.min(best);
        }
        if row_min > limit {
            return None; // every path already costs too much
        }
        std::mem::swap(&mut prev2, &mut prev);
        std::mem::swap(&mut prev, &mut cur);
    }
    let cost = prev[m];
    (cost <= limit).then_some(cost)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn far_slips() {
        let cost = |a: &str, b: &str| slip_cost(a.as_bytes(), b.as_bytes(), FAR_MAX_COST);
        assert_eq!(cost("khogn", "khoong"), Some(DOUBLE + TRANSPOSE));
        assert_eq!(cost("seperate", "separate"), Some(VOWEL));
        assert_eq!(cost("teh", "the"), Some(TRANSPOSE));
        assert_eq!(cost("same", "same"), Some(0.0));
        // Three slips, or a slip of unrelated keys, are out of reach.
        assert_eq!(cost("kubectl", "cube"), None);
        assert_eq!(cost("abcdef", "uvwxyz"), None);
    }

    #[test]
    fn bag_distance_bounds_two_slips() {
        let bag = letter_counts(b"khogn");
        assert_eq!(bag_distance(&bag, b"khoong"), 1); // one letter more
        assert_eq!(bag_distance(&bag, b"khong"), 0); // transposition
        assert!(bag_distance(&bag, b"plane") > 4);
    }


    fn cost(keys: &str, target: &str) -> Option<f64> {
        edits1(keys)
            .into_iter()
            .filter(|s| s.keys == target)
            .map(|s| s.cost)
            .reduce(f64::min)
    }

    #[test]
    fn slip_kinds() {
        assert_eq!(cost("teh", "the"), Some(TRANSPOSE));
        assert_eq!(cost("dunhf", "dungf"), Some(ADJACENT)); // h next to g
        assert_eq!(cost("mooix", "mooir"), Some(TONE_SWAP));
        assert_eq!(cost("as", "ax"), Some(ADJACENT)); // neighbouring tone keys
        assert_eq!(cost("anj", "ans"), None); // j and s: not a plausible slip
        assert_eq!(cost("toi", "tooi"), Some(DOUBLE));
        assert_eq!(cost("toi", "tois"), Some(MISSING_MARK));
        assert_eq!(cost("untill", "until"), Some(DOUBLE));
        assert_eq!(cost("occured", "occurred"), Some(DOUBLE));
        assert_eq!(cost("seperate", "separate"), Some(VOWEL));
        assert_eq!(cost("helo", "help"), Some(ADJACENT));
        assert_eq!(cost("hlo", "hilo"), Some(MISSING));
        assert_eq!(cost("teh", "tzh"), None); // e and z are not neighbours
    }

    #[test]
    fn marks_only_slips() {
        let marks = |keys: &str, target: &str| {
            edits1(keys).into_iter().any(|s| s.keys == target && s.marks_only)
        };
        assert!(marks("mooir", "mooix")); // tone swap
        assert!(marks("khoi", "khoir")); // missing tone
        assert!(marks("khoi", "khooi")); // missing hat
        assert!(!marks("khoi", "khi")); // dropped a letter
    }

    #[test]
    fn excludes_the_input() {
        assert!(edits1("aa").iter().all(|s| s.keys != "aa"));
    }
}
