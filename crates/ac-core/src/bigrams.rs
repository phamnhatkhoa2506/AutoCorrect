//! Word-pair probabilities P(word | previous word), stored as a sorted table
//! of fixed-size records that is searched in place (no parsing at startup).
//!
//! Layout (little endian): magic "ACB1", u32 vocabulary size, u32 record
//! count, then per record `prev: u16, word: u16, q: u16` sorted by (prev,
//! word), where the probability is `exp(-q / 4096)`. Ids are positions in the
//! [`crate::Lexicon`] the table was built from.

/// Sentences at the end of each corpus that training leaves out and the
/// benchmark (`ac-bench`) tests on.
pub const HELD_OUT_SENTENCES: usize = 20_000;

const MAGIC: &[u8; 4] = b"ACB1";
const HEADER: usize = 12;
const RECORD: usize = 6;
const SCALE: f64 = 4096.0;

#[derive(Clone, Copy)]
pub struct Bigrams {
    records: &'static [u8],
}

impl Bigrams {
    pub const EMPTY: Bigrams = Bigrams { records: &[] };

    /// Reads a table built for a lexicon of `vocab` words. A table for a
    /// different lexicon would silently give wrong probabilities, so any
    /// mismatch yields an empty table.
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

    /// ln P(word | prev), if that pair was frequent enough to be kept.
    pub fn ln_prob(&self, prev: u32, word: u32) -> Option<f64> {
        let key = (prev << 16) | word;
        let (mut lo, mut hi) = (0, self.len());
        while lo < hi {
            let mid = (lo + hi) / 2;
            let (p, w, q) = self.record(mid);
            match ((u32::from(p) << 16) | u32::from(w)).cmp(&key) {
                std::cmp::Ordering::Equal => return Some(-f64::from(q) / SCALE),
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
            }
        }
        None
    }

    fn record(&self, i: usize) -> (u16, u16, u16) {
        let r = &self.records[i * RECORD..(i + 1) * RECORD];
        (
            u16::from_le_bytes([r[0], r[1]]),
            u16::from_le_bytes([r[2], r[3]]),
            u16::from_le_bytes([r[4], r[5]]),
        )
    }

    /// Serialises `(prev, word, probability)` triples for a lexicon of
    /// `vocab` words. Ids must fit in 16 bits.
    pub fn encode(vocab: usize, pairs: &[(u32, u32, f64)]) -> Vec<u8> {
        assert!(vocab <= usize::from(u16::MAX) + 1, "vocabulary too large for 16-bit ids");
        let mut sorted: Vec<&(u32, u32, f64)> = pairs.iter().collect();
        sorted.sort_by_key(|p| (p.0, p.1));
        let mut out = Vec::with_capacity(HEADER + sorted.len() * RECORD);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&(vocab as u32).to_le_bytes());
        out.extend_from_slice(&(sorted.len() as u32).to_le_bytes());
        for &&(prev, word, p) in &sorted {
            let q = (-p.ln() * SCALE).round().clamp(0.0, f64::from(u16::MAX)) as u16;
            out.extend_from_slice(&(prev as u16).to_le_bytes());
            out.extend_from_slice(&(word as u16).to_le_bytes());
            out.extend_from_slice(&q.to_le_bytes());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> Bigrams {
        let bytes = Bigrams::encode(10, &[(3, 4, 0.5), (1, 9, 0.01), (3, 2, 0.25)]);
        Bigrams::from_bytes(Box::leak(bytes.into_boxed_slice()), 10)
    }

    #[test]
    fn round_trips_probabilities() {
        let t = table();
        assert_eq!(t.len(), 3);
        assert!((t.ln_prob(3, 4).unwrap() - 0.5f64.ln()).abs() < 1e-3);
        assert!((t.ln_prob(1, 9).unwrap() - 0.01f64.ln()).abs() < 1e-3);
        assert_eq!(t.ln_prob(3, 3), None);
        assert_eq!(t.ln_prob(0, 0), None);
    }

    #[test]
    fn rejects_a_table_for_another_lexicon() {
        let bytes: &'static [u8] = Box::leak(Bigrams::encode(10, &[(1, 2, 0.5)]).into_boxed_slice());
        assert!(Bigrams::from_bytes(bytes, 11).is_empty());
        assert!(Bigrams::from_bytes(&bytes[..bytes.len() - 1], 10).is_empty());
        assert!(Bigrams::from_bytes(b"junk", 10).is_empty());
    }
}
