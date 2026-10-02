//! Word-triple probabilities P(word | two previous words), stored like
//! [`crate::Bigrams`]: a sorted table of fixed-size records searched in place.
//!
//! Layout (little endian): magic "ACT1", u32 vocabulary size, u32 record
//! count, then per record `first: u16, second: u16, word: u16, q: u16` sorted
//! by (first, second, word), where the probability is `exp(-q / 4096)`.

const MAGIC: &[u8; 4] = b"ACT1";
const HEADER: usize = 12;
const RECORD: usize = 8;
const SCALE: f64 = 4096.0;

#[derive(Clone, Copy)]
pub struct Trigrams {
    records: &'static [u8],
}

impl Trigrams {
    pub const EMPTY: Trigrams = Trigrams { records: &[] };

    /// Reads a table built for a lexicon of `vocab` words; any mismatch
    /// yields an empty table.
    pub fn from_bytes(bytes: &'static [u8], vocab: usize) -> Self {
        let u32_at = |at: usize| bytes.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        let ok = bytes.get(..4) == Some(&MAGIC[..])
            && u32_at(4) == Some(vocab as u32)
            && u32_at(8).map(|n| HEADER + n as usize * RECORD) == Some(bytes.len());
        if ok {
            Self { records: &bytes[HEADER..] }
        } else {
            Self::EMPTY
        }
    }

    pub fn len(&self) -> usize {
        self.records.len() / RECORD
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// ln P(word | first, second), if that triple was frequent enough to be kept.
    pub fn ln_prob(&self, first: u32, second: u32, word: u32) -> Option<f64> {
        let key = (u64::from(first) << 32) | (u64::from(second) << 16) | u64::from(word);
        let (mut lo, mut hi) = (0, self.len());
        while lo < hi {
            let mid = (lo + hi) / 2;
            let r = &self.records[mid * RECORD..(mid + 1) * RECORD];
            let at = (u64::from(u16::from_le_bytes([r[0], r[1]])) << 32)
                | (u64::from(u16::from_le_bytes([r[2], r[3]])) << 16)
                | u64::from(u16::from_le_bytes([r[4], r[5]]));
            match at.cmp(&key) {
                std::cmp::Ordering::Equal => return Some(-f64::from(u16::from_le_bytes([r[6], r[7]])) / SCALE),
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
            }
        }
        None
    }

    /// Serialises `(first, second, word, probability)` for a lexicon of
    /// `vocab` words. Ids must fit in 16 bits.
    pub fn encode(vocab: usize, triples: &[(u32, u32, u32, f64)]) -> Vec<u8> {
        assert!(vocab <= usize::from(u16::MAX) + 1, "vocabulary too large for 16-bit ids");
        let mut sorted: Vec<&(u32, u32, u32, f64)> = triples.iter().collect();
        sorted.sort_by_key(|t| (t.0, t.1, t.2));
        let mut out = Vec::with_capacity(HEADER + sorted.len() * RECORD);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&(vocab as u32).to_le_bytes());
        out.extend_from_slice(&(sorted.len() as u32).to_le_bytes());
        for &&(a, b, w, p) in &sorted {
            let q = (-p.ln() * SCALE).round().clamp(0.0, f64::from(u16::MAX)) as u16;
            for v in [a as u16, b as u16, w as u16, q] {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_probabilities() {
        let bytes = Trigrams::encode(10, &[(3, 4, 5, 0.5), (1, 2, 9, 0.01), (3, 4, 2, 0.25)]);
        let t = Trigrams::from_bytes(Box::leak(bytes.into_boxed_slice()), 10);
        assert_eq!(t.len(), 3);
        assert!((t.ln_prob(3, 4, 5).unwrap() - 0.5f64.ln()).abs() < 1e-3);
        assert!((t.ln_prob(1, 2, 9).unwrap() - 0.01f64.ln()).abs() < 1e-3);
        assert_eq!(t.ln_prob(3, 4, 3), None);
        assert_eq!(t.ln_prob(4, 3, 5), None);
    }

    #[test]
    fn rejects_a_table_for_another_lexicon() {
        let bytes: &'static [u8] = Box::leak(Trigrams::encode(10, &[(1, 2, 3, 0.5)]).into_boxed_slice());
        assert!(Trigrams::from_bytes(bytes, 11).is_empty());
        assert!(Trigrams::from_bytes(&bytes[..bytes.len() - 1], 10).is_empty());
    }
}
