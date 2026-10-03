//! Interpolated Kneser-Ney word probabilities P(word | two previous words),
//! one table per language, searched in place.
//!
//! Layout (little endian): magic "ACK1", u32 vocabulary size, u32 pair count,
//! u32 triple count, u32 backoff count; then per word `p1: u16, g2: u16`
//! (unigram probability, and the weight left for the unigram when backing off
//! from a pair); then pairs `prev: u16, word: u16, q: u16`; triples
//! `first: u16, second: u16, word: u16, q: u16`; and backoff weights
//! `first: u16, second: u16, g3: u16` for the two-word contexts that have a
//! stored triple. Every `q` is `-ln(p) * 4096`.

const MAGIC: &[u8; 4] = b"ACK1";
const HEADER: usize = 20;
const SCALE: f64 = 4096.0;
/// Weight left for the pair estimate in a context with no stored triple.
const DEFAULT_G3: f64 = 0.85;

#[derive(Clone, Copy)]
pub struct Kn {
    uni: &'static [u8],
    pairs: &'static [u8],
    triples: &'static [u8],
    gammas: &'static [u8],
}

fn q_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn unq(q: u16) -> f64 {
    (-f64::from(q) / SCALE).exp()
}

/// Binary search over fixed-size records whose first u16 fields form `key`;
/// returns the u16 that follows the key.
fn find(records: &[u8], record: usize, key: &[u16]) -> Option<u16> {
    let (mut lo, mut hi) = (0, records.len() / record);
    while lo < hi {
        let mid = (lo + hi) / 2;
        let r = &records[mid * record..(mid + 1) * record];
        let cmp = key
            .iter()
            .enumerate()
            .map(|(i, k)| q_at(r, i * 2).cmp(k))
            .find(|o| o.is_ne())
            .unwrap_or(std::cmp::Ordering::Equal);
        match cmp {
            std::cmp::Ordering::Equal => return Some(q_at(r, key.len() * 2)),
            std::cmp::Ordering::Less => lo = mid + 1,
            std::cmp::Ordering::Greater => hi = mid,
        }
    }
    None
}

impl Kn {
    pub const EMPTY: Kn = Kn { uni: &[], pairs: &[], triples: &[], gammas: &[] };

    /// Reads a table built for a lexicon of `vocab` words; any mismatch yields an empty table.
    pub fn from_bytes(bytes: &'static [u8], vocab: usize) -> Self {
        let u32_at = |at: usize| bytes.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
        let (Some(v), Some(np), Some(nt), Some(ng)) = (u32_at(4), u32_at(8), u32_at(12), u32_at(16)) else {
            return Self::EMPTY;
        };
        let (uni_len, pair_len, tri_len, gam_len) = (v * 4, np * 6, nt * 8, ng * 6);
        if bytes.get(..4) != Some(&MAGIC[..]) || v != vocab || bytes.len() != HEADER + uni_len + pair_len + tri_len + gam_len {
            return Self::EMPTY;
        }
        let body = &bytes[HEADER..];
        let (uni, body) = body.split_at(uni_len);
        let (pairs, body) = body.split_at(pair_len);
        let (triples, gammas) = body.split_at(tri_len);
        Self { uni, pairs, triples, gammas }
    }

    pub fn is_empty(&self) -> bool {
        self.uni.is_empty()
    }

    pub fn pairs(&self) -> usize {
        self.pairs.len() / 6
    }

    pub fn triples(&self) -> usize {
        self.triples.len() / 8
    }

    fn p1(&self, w: u32) -> f64 {
        unq(q_at(self.uni, w as usize * 4))
    }

    fn g2(&self, b: u32) -> f64 {
        unq(q_at(self.uni, b as usize * 4 + 2))
    }

    fn p2(&self, b: u32, w: u32) -> f64 {
        match find(self.pairs, 6, &[b as u16, w as u16]) {
            Some(q) => unq(q),
            None => self.g2(b) * self.p1(w),
        }
    }

    /// ln P(word | first, second), backing off to the pair and then the word.
    /// `second` is the word just before; `first` the one before that.
    pub fn ln_prob(&self, first: Option<u32>, second: u32, word: u32) -> f64 {
        if self.is_empty() {
            return f64::NEG_INFINITY;
        }
        let Some(first) = first else { return self.p2(second, word).ln() };
        match find(self.triples, 8, &[first as u16, second as u16, word as u16]) {
            Some(q) => unq(q).ln(),
            None => {
                let g3 = find(self.gammas, 6, &[first as u16, second as u16]).map_or(DEFAULT_G3, unq);
                (g3 * self.p2(second, word)).ln()
            }
        }
    }

    /// Serialises the tables for a lexicon of `vocab` words. All lists are sorted here.
    pub fn encode(
        vocab: usize,
        uni: &[(f64, f64)],
        pairs: &mut [(u32, u32, f64)],
        triples: &mut [(u32, u32, u32, f64)],
        gammas: &mut [(u32, u32, f64)],
    ) -> Vec<u8> {
        assert!(vocab <= usize::from(u16::MAX) + 1 && uni.len() == vocab);
        let q = |p: f64| (-p.max(1e-300).ln() * SCALE).round().clamp(0.0, f64::from(u16::MAX)) as u16;
        pairs.sort_by_key(|p| (p.0, p.1));
        triples.sort_by_key(|t| (t.0, t.1, t.2));
        gammas.sort_by_key(|g| (g.0, g.1));
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        for n in [vocab, pairs.len(), triples.len(), gammas.len()] {
            out.extend_from_slice(&(n as u32).to_le_bytes());
        }
        let put = |out: &mut Vec<u8>, v: u16| out.extend_from_slice(&v.to_le_bytes());
        for &(p1, g2) in uni {
            put(&mut out, q(p1));
            put(&mut out, q(g2));
        }
        for &(a, b, p) in pairs.iter() {
            for v in [a as u16, b as u16, q(p)] {
                put(&mut out, v);
            }
        }
        for &(a, b, w, p) in triples.iter() {
            for v in [a as u16, b as u16, w as u16, q(p)] {
                put(&mut out, v);
            }
        }
        for &(a, b, g) in gammas.iter() {
            for v in [a as u16, b as u16, q(g)] {
                put(&mut out, v);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> Kn {
        let uni = vec![(0.5, 0.5), (0.3, 0.4), (0.2, 0.3)];
        let bytes = Kn::encode(3, &uni, &mut [(0, 1, 0.6)], &mut [(2, 0, 1, 0.9)], &mut [(2, 0, 0.1)]);
        Kn::from_bytes(Box::leak(bytes.into_boxed_slice()), 3)
    }

    #[test]
    fn looks_up_each_level() {
        let t = table();
        assert!((t.ln_prob(Some(2), 0, 1) - 0.9f64.ln()).abs() < 1e-3); // stored triple
        assert!((t.ln_prob(None, 0, 1) - 0.6f64.ln()).abs() < 1e-3); // stored pair
        // No pair (1, 2): the weight left for the word, times the word's probability.
        assert!((t.ln_prob(None, 1, 2) - (0.4f64 * 0.2).ln()).abs() < 1e-3);
        // Context (2, 0) has a stored weight 0.1 but no triple for word 2:
        // 0.1 * (pair (0, 2) is missing: g2(0) * p1(2) = 0.5 * 0.2).
        assert!((t.ln_prob(Some(2), 0, 2) - (0.1f64 * 0.5 * 0.2).ln()).abs() < 1e-3);
        // A context with no stored weight uses the default.
        assert!((t.ln_prob(Some(1), 0, 1) - (DEFAULT_G3 * 0.6).ln()).abs() < 1e-3);
    }

    #[test]
    fn rejects_a_table_for_another_lexicon() {
        let uni = vec![(0.5, 0.5), (0.5, 0.5)];
        let bytes: &'static [u8] = Box::leak(Kn::encode(2, &uni, &mut [], &mut [], &mut []).into_boxed_slice());
        assert!(Kn::from_bytes(bytes, 3).is_empty());
        assert!(Kn::from_bytes(&bytes[..bytes.len() - 1], 2).is_empty());
    }
}
