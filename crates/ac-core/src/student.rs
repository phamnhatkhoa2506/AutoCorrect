//! The student: a small network that judges one word of a window of words, learned from a larger
//! teacher (RESEARCH.md, section 5; training in `tools/student/student_train.py`, export in
//! `tools/student/export_student.py`, which documents the file format).
//!
//! Each word is read as hashed letter n-grams (so a misspelt word still has a shape), a vocabulary
//! id (for exact words) and a shape flag; three Transformer layers read the window, one word to
//! its right at least, and the head says: keep the word (class 0), or replace it by a syllable of
//! the vocabulary. Plain `f32`, no dependencies; one window of about 20 words takes a few
//! milliseconds.

use std::collections::HashMap;

/// The token every window starts with (the "environment": ordinary text).
const ENV_TOKEN: &str = "<<normal>>";
/// Longest window the network was trained on, in words (the environment token not counted).
pub const WINDOW: usize = 40;
const EPS: f32 = 1e-5;

struct Layer {
    norm1: (Vec<f32>, Vec<f32>),
    in_proj: (Vec<f32>, Vec<f32>),
    out_proj: (Vec<f32>, Vec<f32>),
    norm2: (Vec<f32>, Vec<f32>),
    linear1: (Vec<f32>, Vec<f32>),
    linear2: (Vec<f32>, Vec<f32>),
}

pub struct Student {
    buckets: usize,
    ngram_cap: usize,
    d_bucket: usize,
    d_model: usize,
    heads: usize,
    classes: usize,
    vocab: Vec<String>,
    ids: HashMap<String, usize>,
    bucket_table: Vec<f32>,
    bucket_proj: (Vec<f32>, Vec<f32>),
    vocab_emb: Vec<f32>,
    flag_emb: Vec<f32>,
    pos_emb: Vec<f32>,
    layers: Vec<Layer>,
    norm: (Vec<f32>, Vec<f32>),
    fix_head: (Vec<f32>, Vec<f32>),
}

/// What the student thinks of one word.
#[derive(Debug, Clone)]
pub struct Judgement {
    /// Probability that the word should change (1 minus the probability of keeping it).
    pub p_change: f32,
    /// The most likely replacements, best first, each with its probability.
    pub fixes: Vec<(String, f32)>,
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.at.checked_add(n).filter(|&e| e <= self.bytes.len()).ok_or("student file is cut short")?;
        let out = &self.bytes[self.at..end];
        self.at = end;
        Ok(out)
    }

    fn u32(&mut self) -> Result<usize, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()) as usize)
    }

    fn floats(&mut self, n: usize) -> Result<Vec<f32>, String> {
        let raw = self.take(n.checked_mul(4).ok_or("student file is corrupt")?)?;
        Ok(raw.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect())
    }

    /// A weight matrix `[out, inn]` and its bias `[out]`.
    fn linear(&mut self, out: usize, inn: usize) -> Result<(Vec<f32>, Vec<f32>), String> {
        Ok((self.floats(out * inn)?, self.floats(out)?))
    }

    fn norm(&mut self, n: usize) -> Result<(Vec<f32>, Vec<f32>), String> {
        Ok((self.floats(n)?, self.floats(n)?))
    }
}

/// CRC-32 (IEEE), the same as Python's `zlib.crc32`: how the letter n-grams are hashed.
fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

/// Abramowitz and Stegun 7.1.26, absolute error below 1.5e-7.
fn erf(x: f32) -> f32 {
    let t = 1.0 / (1.0 + 0.327_591_1 * x.abs());
    let poly = ((((1.061_405_4 * t - 1.453_152_1) * t + 1.421_413_8) * t - 0.284_496_72) * t + 0.254_829_6) * t;
    (1.0 - poly * (-x * x).exp()).copysign(x)
}

fn gelu(x: f32) -> f32 {
    0.5 * x * (1.0 + erf(x * std::f32::consts::FRAC_1_SQRT_2))
}

fn layer_norm(x: &[f32], (weight, bias): &(Vec<f32>, Vec<f32>)) -> Vec<f32> {
    let n = x.len() as f32;
    let mean = x.iter().sum::<f32>() / n;
    let var = x.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / n;
    let scale = 1.0 / (var + EPS).sqrt();
    x.iter().zip(weight.iter().zip(bias)).map(|(v, (w, b))| (v - mean) * scale * w + b).collect()
}

/// Dot product with eight running sums, which the compiler turns into vector instructions
/// (a single running sum cannot be: floating-point addition is not associative).
fn dot(a: &[f32], b: &[f32]) -> f32 {
    let mut lanes = [0.0f32; 8];
    let (ca, cb) = (a.chunks_exact(8), b.chunks_exact(8));
    let tail: f32 = ca.remainder().iter().zip(cb.remainder()).map(|(x, y)| x * y).sum();
    for (x, y) in ca.zip(cb) {
        for k in 0..8 {
            lanes[k] += x[k] * y[k];
        }
    }
    lanes.iter().sum::<f32>() + tail
}

/// `out[o] = bias[o] + sum_i weight[o][i] * x[i]`.
fn affine(x: &[f32], (weight, bias): &(Vec<f32>, Vec<f32>)) -> Vec<f32> {
    let inn = x.len();
    bias.iter().enumerate().map(|(o, b)| b + dot(&weight[o * inn..(o + 1) * inn], x)).collect()
}

fn is_cased_upper(token: &str) -> bool {
    let mut any = false;
    for c in token.chars() {
        if c.is_lowercase() {
            return false;
        }
        any |= c.is_uppercase();
    }
    any
}

/// Shape of a word, as in training: 5 environment token, 4 no letters, 2 ALL CAPS, 1 Capitalised, 0 other.
fn shape_flag(token: &str) -> usize {
    if token.starts_with("<<") {
        5
    } else if !token.chars().any(char::is_alphabetic) {
        4
    } else if is_cased_upper(token) && token.chars().count() > 1 {
        2
    } else if token.chars().next().is_some_and(char::is_uppercase) {
        1
    } else {
        0
    }
}

impl Student {
    /// Reads a `student.acs` file.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let mut r = Reader { bytes, at: 0 };
        if r.take(4)? != b"ACST" {
            return Err("not a student file".into());
        }
        if r.u32()? != 1 {
            return Err("unknown student file version".into());
        }
        let (buckets, ngram_cap, d_bucket, d_model, heads, layer_count, ff, classes) =
            (r.u32()?, r.u32()?, r.u32()?, r.u32()?, r.u32()?, r.u32()?, r.u32()?, r.u32()?);
        if heads == 0 || d_model % heads != 0 || layer_count > 64 {
            return Err("student file has a strange shape".into());
        }
        let vocab_len = r.u32()?;
        let mut vocab = Vec::with_capacity(vocab_len);
        for _ in 0..vocab_len {
            let n = u16::from_le_bytes(r.take(2)?.try_into().unwrap()) as usize;
            vocab.push(String::from_utf8(r.take(n)?.to_vec()).map_err(|e| e.to_string())?);
        }
        if vocab.len() + 1 != classes {
            return Err("student vocabulary does not match its classes".into());
        }
        let ids = vocab.iter().enumerate().map(|(i, w)| (w.clone(), i + 1)).collect();
        let bucket_table = r.floats(buckets * d_bucket)?;
        let bucket_proj = r.linear(d_model, d_bucket)?;
        let vocab_emb = r.floats(classes * d_model)?;
        let flag_emb = r.floats(6 * d_model)?;
        let pos_emb = r.floats(64 * d_model)?;
        let mut layers = Vec::with_capacity(layer_count);
        for _ in 0..layer_count {
            layers.push(Layer {
                norm1: r.norm(d_model)?,
                in_proj: r.linear(3 * d_model, d_model)?,
                out_proj: r.linear(d_model, d_model)?,
                norm2: r.norm(d_model)?,
                linear1: r.linear(ff, d_model)?,
                linear2: r.linear(d_model, ff)?,
            });
        }
        let norm = r.norm(d_model)?;
        let fix_head = r.linear(classes, d_model)?;
        if r.at != bytes.len() {
            return Err("student file has more data than expected".into());
        }
        Ok(Self { buckets, ngram_cap, d_bucket, d_model, heads, classes, vocab, ids, bucket_table, bucket_proj, vocab_emb, flag_emb, pos_emb, layers, norm, fix_head })
    }

    /// Bucket ids of the letter n-grams of a word: the whole word with boundaries, then the 1, 2 and 3-grams.
    fn ngram_buckets(&self, token: &str) -> Vec<usize> {
        let chars: Vec<char> = std::iter::once('^').chain(token.to_lowercase().chars()).chain(std::iter::once('$')).collect();
        let mut grams: Vec<String> = vec![chars.iter().collect()];
        for n in 1..=3 {
            for window in chars.windows(n) {
                grams.push(window.iter().collect());
            }
        }
        grams.iter().take(self.ngram_cap).map(|g| crc32(g.as_bytes()) as usize % self.buckets).collect()
    }

    /// Embedding of every word of the window (environment token first).
    fn embed(&self, tokens: &[&str]) -> Vec<Vec<f32>> {
        let dm = self.d_model;
        tokens
            .iter()
            .enumerate()
            .map(|(i, token)| {
                let grams = self.ngram_buckets(token);
                let mut mean = vec![0.0f32; self.d_bucket];
                for &g in &grams {
                    for (m, w) in mean.iter_mut().zip(&self.bucket_table[g * self.d_bucket..(g + 1) * self.d_bucket]) {
                        *m += w;
                    }
                }
                let count = grams.len() as f32;
                mean.iter_mut().for_each(|m| *m /= count);
                let mut h = affine(&mean, &self.bucket_proj);
                let vid = self.ids.get(&token.to_lowercase()).copied().unwrap_or(0);
                let flag = shape_flag(token);
                for k in 0..dm {
                    h[k] += self.vocab_emb[vid * dm + k] + self.flag_emb[flag * dm + k] + self.pos_emb[i * dm + k];
                }
                h
            })
            .collect()
    }

    fn attention(&self, layer: &Layer, x: &[Vec<f32>]) -> Vec<Vec<f32>> {
        let (dm, heads) = (self.d_model, self.heads);
        let hd = dm / heads;
        let qkv: Vec<Vec<f32>> = x.iter().map(|row| affine(&layer_norm(row, &layer.norm1), &layer.in_proj)).collect();
        let scale = 1.0 / (hd as f32).sqrt();
        let mut out = vec![vec![0.0f32; dm]; x.len()];
        for h in 0..heads {
            for (i, row) in out.iter_mut().enumerate() {
                let q = &qkv[i][h * hd..(h + 1) * hd];
                let mut scores: Vec<f32> = qkv
                    .iter()
                    .map(|other| dot(q, &other[dm + h * hd..dm + (h + 1) * hd]) * scale)
                    .collect();
                let top = scores.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                let mut total = 0.0;
                for s in scores.iter_mut() {
                    *s = (*s - top).exp();
                    total += *s;
                }
                for (j, s) in scores.iter().enumerate() {
                    let weight = s / total;
                    for (o, v) in row[h * hd..(h + 1) * hd].iter_mut().zip(&qkv[j][2 * dm + h * hd..2 * dm + (h + 1) * hd]) {
                        *o += weight * v;
                    }
                }
            }
        }
        out.iter().map(|row| affine(row, &layer.out_proj)).collect()
    }

    /// Probability of each class at `window[at]` (class 0 keeps the word; class `i` is vocabulary word
    /// `i - 1`). The window holds the words of the phrase as shown, oldest first, with at least one
    /// word after the one judged; only the last [`WINDOW`] are read.
    pub fn probs(&self, window: &[&str], at: usize) -> Option<Vec<f32>> {
        if at >= window.len() {
            return None;
        }
        let from = (window.len() - WINDOW.min(window.len())).min(at);
        let words = &window[from..];
        let at = at - from;
        let tokens: Vec<&str> = std::iter::once(ENV_TOKEN).chain(words.iter().copied()).collect();
        let mut x = self.embed(&tokens);
        for layer in &self.layers {
            let attended = self.attention(layer, &x);
            for (row, a) in x.iter_mut().zip(&attended) {
                row.iter_mut().zip(a).for_each(|(r, v)| *r += v);
            }
            for row in x.iter_mut() {
                let mut h = affine(&layer_norm(row, &layer.norm2), &layer.linear1);
                h.iter_mut().for_each(|v| *v = gelu(*v));
                let delta = affine(&h, &layer.linear2);
                row.iter_mut().zip(&delta).for_each(|(r, v)| *r += v);
            }
        }
        let logits = affine(&layer_norm(&x[at + 1], &self.norm), &self.fix_head);
        let top = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let exp: Vec<f32> = logits.iter().map(|l| (l - top).exp()).collect();
        let total: f32 = exp.iter().sum();
        Some(exp.into_iter().map(|e| e / total).collect())
    }

    /// Probability of replacing the word by `word` (lower case), 0 if it is not in the vocabulary.
    pub fn prob_of(&self, probs: &[f32], word: &str) -> f32 {
        self.ids.get(word).map_or(0.0, |&class| probs[class])
    }

    /// Judges `window[at]` (see [`Student::probs`]): the chance it should change and the likeliest fixes.
    pub fn judge(&self, window: &[&str], at: usize) -> Option<Judgement> {
        let p = self.probs(window, at)?;
        let mut order: Vec<usize> = (1..self.classes).collect();
        order.sort_by(|&a, &b| p[b].total_cmp(&p[a]));
        Some(Judgement { p_change: 1.0 - p[0], fixes: order.iter().take(5).map(|&c| (self.vocab[c - 1].clone(), p[c])).collect() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_matches_zlib() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn erf_is_close() {
        assert!((erf(0.5) - 0.520_499_9).abs() < 1e-6);
        assert!((erf(-1.0) + 0.842_700_8).abs() < 1e-6);
        assert!(gelu(0.0).abs() < 1e-9);
    }

    #[test]
    fn shape_flags_follow_training() {
        assert_eq!(shape_flag("<<normal>>"), 5);
        assert_eq!(shape_flag("2016"), 4);
        assert_eq!(shape_flag("."), 4);
        assert_eq!(shape_flag("NASA"), 2);
        assert_eq!(shape_flag("Việt"), 1);
        assert_eq!(shape_flag("việt"), 0);
        assert_eq!(shape_flag("Đ"), 1);
    }

    /// The same windows through PyTorch (tools/student/export_student.py) and through this code.
    /// Needs `models/student.acs`; skipped when it is not there.
    #[test]
    fn agrees_with_pytorch() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let Ok(bytes) = std::fs::read(root.join("models/student.acs")) else {
            eprintln!("models/student.acs not found: skipped");
            return;
        };
        let student = Student::from_bytes(&bytes).expect("student file");
        let cases = golden::load(&root.join("crates/ac-core/testdata/student_golden.json"));
        assert!(!cases.is_empty());
        for case in &cases {
            // the golden windows start with the environment token, which `judge` adds itself
            let words: Vec<&str> = case.tokens.iter().skip(1).map(String::as_str).collect();
            let got = student.judge(&words, case.position - 1).expect("judged");
            assert!((1.0 - got.p_change - case.p_keep).abs() < 2e-3, "{:?}: keep {} vs {}", words, 1.0 - got.p_change, case.p_keep);
            for (rank, (class, p)) in case.top.iter().enumerate() {
                if *class == 0 || *p <= 0.01 {
                    continue;
                }
                let word = &student.vocab[class - 1];
                let (_, q) = got.fixes.iter().find(|(w, _)| w == word).unwrap_or_else(|| panic!("{:?}: fix {word} (rank {rank}) missing", words));
                assert!((q - p).abs() < 2e-3, "{:?}: {word} {q} vs {p}", words);
            }
        }
    }

    /// A very small JSON reader for the golden file (the crate has no dependencies).
    mod golden {
        pub struct Case {
            pub tokens: Vec<String>,
            pub position: usize,
            pub p_keep: f32,
            pub top: Vec<(usize, f32)>,
        }

        pub fn load(path: &std::path::Path) -> Vec<Case> {
            let text = std::fs::read_to_string(path).expect("golden file");
            let mut p = Parser { s: text.as_bytes(), at: 0 };
            let Value::List(items) = p.value() else { panic!("list expected") };
            items.into_iter().map(Case::from).collect()
        }

        enum Value {
            Str(String),
            Num(f64),
            List(Vec<Value>),
            Map(Vec<(String, Value)>),
        }

        impl Value {
            fn num(&self) -> f64 {
                if let Value::Num(n) = self { *n } else { panic!("number expected") }
            }
        }

        impl From<Value> for Case {
            fn from(v: Value) -> Case {
                let Value::Map(fields) = v else { panic!("object expected") };
                let get = |name: &str| fields.iter().find(|(k, _)| k == name).map(|(_, v)| v).expect(name);
                let Value::List(tokens) = get("tokens") else { panic!() };
                let Value::List(top) = get("top") else { panic!() };
                Case {
                    tokens: tokens.iter().map(|t| if let Value::Str(s) = t { s.clone() } else { panic!() }).collect(),
                    position: get("position").num() as usize,
                    p_keep: get("p_keep").num() as f32,
                    top: top
                        .iter()
                        .map(|pair| {
                            let Value::List(pair) = pair else { panic!() };
                            (pair[0].num() as usize, pair[1].num() as f32)
                        })
                        .collect(),
                }
            }
        }

        struct Parser<'a> {
            s: &'a [u8],
            at: usize,
        }

        impl Parser<'_> {
            fn ws(&mut self) {
                while self.s[self.at].is_ascii_whitespace() {
                    self.at += 1;
                }
            }

            fn value(&mut self) -> Value {
                self.ws();
                match self.s[self.at] {
                    b'[' => {
                        self.at += 1;
                        let mut out = Vec::new();
                        loop {
                            self.ws();
                            if self.s[self.at] == b']' {
                                self.at += 1;
                                return Value::List(out);
                            }
                            out.push(self.value());
                            self.ws();
                            if self.s[self.at] == b',' {
                                self.at += 1;
                            }
                        }
                    }
                    b'{' => {
                        self.at += 1;
                        let mut out = Vec::new();
                        loop {
                            self.ws();
                            if self.s[self.at] == b'}' {
                                self.at += 1;
                                return Value::Map(out);
                            }
                            let Value::Str(key) = self.value() else { panic!("key expected") };
                            self.ws();
                            self.at += 1; // ':'
                            out.push((key, self.value()));
                            self.ws();
                            if self.s[self.at] == b',' {
                                self.at += 1;
                            }
                        }
                    }
                    b'"' => {
                        self.at += 1;
                        let mut raw = Vec::new();
                        while self.s[self.at] != b'"' {
                            if self.s[self.at] == b'\\' {
                                self.at += 1;
                                match self.s[self.at] {
                                    b'n' => raw.push(b'\n'),
                                    b't' => raw.push(b'\t'),
                                    b'u' => {
                                        let code = u32::from_str_radix(std::str::from_utf8(&self.s[self.at + 1..self.at + 5]).unwrap(), 16).unwrap();
                                        let mut buf = [0u8; 4];
                                        raw.extend_from_slice(char::from_u32(code).unwrap().encode_utf8(&mut buf).as_bytes());
                                        self.at += 4;
                                    }
                                    other => raw.push(other),
                                }
                            } else {
                                raw.push(self.s[self.at]);
                            }
                            self.at += 1;
                        }
                        self.at += 1;
                        Value::Str(String::from_utf8(raw).unwrap())
                    }
                    _ => {
                        let start = self.at;
                        while self.at < self.s.len() && !matches!(self.s[self.at], b',' | b']' | b'}') && !self.s[self.at].is_ascii_whitespace() {
                            self.at += 1;
                        }
                        Value::Num(std::str::from_utf8(&self.s[start..self.at]).unwrap().parse().unwrap())
                    }
                }
            }
        }
    }
}
