//! What goes into the journal, and when.
//!
//! A line says what the app did to a word and what stood around it. The words
//! before are known at once; the words after are not typed yet, so with
//! `journal_right` a line waits in a small queue until the next few words end
//! (or the text around it is lost: Enter, a click, another window, a pause in
//! typing), and is written then. Pure logic, no file or hook: the caller writes
//! the lines this returns.
//!
//! Line format, after the Unix time the log adds, tab separated:
//! `kind, keys, fix, context, mode, app, left, right`
//! * `context`: the word just before (the old format's fourth column);
//! * `left`: all the words before it that the corrector saw, oldest first, space separated;
//! * `right`: up to `RIGHT_WORDS` words typed after it, as they ended up on screen
//!   (empty when `journal_right` is off or nothing followed).

use std::time::{Duration, Instant};

/// Words of right-hand context kept per line.
pub const RIGHT_WORDS: usize = 3;
/// A line that has waited this long is written with what it has.
const MAX_WAIT: Duration = Duration::from_secs(60);
/// Lines waiting at once; the oldest is written when more arrive.
const MAX_WAITING: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// FIX, LATE, UNDO, UNDO-LATE, EDIT or NEAR.
    pub kind: &'static str,
    pub keys: String,
    pub fix: String,
    pub mode: &'static str,
    pub app: String,
    /// The words before, oldest first.
    pub left: Vec<String>,
    /// The words after, in order.
    pub right: Vec<String>,
}

impl Entry {
    pub fn line(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            self.kind,
            self.keys,
            self.fix,
            self.left.last().map_or("", String::as_str),
            self.mode,
            self.app,
            self.left.join(" "),
            self.right.join(" ")
        )
    }
}

#[derive(Default)]
pub struct Journal {
    /// Keep lines until the words after them are known.
    wait_for_right: bool,
    waiting: Vec<(Instant, Entry)>,
}

impl Journal {
    /// Turns the waiting on or off; turning it off writes what is waiting.
    pub fn set_wait_for_right(&mut self, on: bool, out: &mut Vec<String>) {
        if self.wait_for_right && !on {
            self.flush(out);
        }
        self.wait_for_right = on;
    }

    /// A new line: written now, or queued for the words that follow.
    pub fn add(&mut self, entry: Entry, now: Instant, out: &mut Vec<String>) {
        if !self.wait_for_right || entry.right.len() >= RIGHT_WORDS {
            out.push(entry.line());
            return;
        }
        self.waiting.push((now, entry));
        while self.waiting.len() > MAX_WAITING {
            out.push(self.waiting.remove(0).1.line());
        }
    }

    /// A word ended. Every waiting line gets it as right-hand context; the line is
    /// written once it has enough, or when `sentence_ends` (the text after a full
    /// stop is another sentence).
    pub fn word_finished(&mut self, word: &str, sentence_ends: bool, out: &mut Vec<String>) {
        for (_, entry) in &mut self.waiting {
            entry.right.push(word.to_string());
        }
        let (done, waiting): (Vec<_>, Vec<_>) =
            std::mem::take(&mut self.waiting).into_iter().partition(|(_, e)| sentence_ends || e.right.len() >= RIGHT_WORDS);
        out.extend(done.into_iter().map(|(_, e)| e.line()));
        self.waiting = waiting;
    }

    /// The text around the waiting lines is lost (Enter, a click, another window,
    /// the app quitting): write them with what they have.
    pub fn flush(&mut self, out: &mut Vec<String>) {
        out.extend(self.waiting.drain(..).map(|(_, e)| e.line()));
    }

    /// Writes the lines that have waited too long (the typist stopped).
    pub fn expire(&mut self, now: Instant, out: &mut Vec<String>) {
        let (old, fresh): (Vec<_>, Vec<_>) =
            std::mem::take(&mut self.waiting).into_iter().partition(|(since, _)| now.duration_since(*since) >= MAX_WAIT);
        out.extend(old.into_iter().map(|(_, e)| e.line()));
        self.waiting = fresh;
    }

    pub fn waiting(&self) -> usize {
        self.waiting.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: &'static str, keys: &str, fix: &str, left: &[&str]) -> Entry {
        Entry {
            kind,
            keys: keys.into(),
            fix: fix.into(),
            mode: "vi",
            app: "Normal".into(),
            left: left.iter().map(|w| w.to_string()).collect(),
            right: Vec::new(),
        }
    }

    #[test]
    fn a_line_is_written_at_once_when_not_waiting() {
        let mut j = Journal::default();
        let mut out = Vec::new();
        j.add(entry("FIX", "teh", "the", &["a", "b"]), Instant::now(), &mut out);
        assert_eq!(out, ["FIX\tteh\tthe\tb\tvi\tNormal\ta b\t"]);
        assert_eq!(j.waiting(), 0);
    }

    #[test]
    fn a_waiting_line_collects_the_words_after_it() {
        let mut j = Journal::default();
        let mut out = Vec::new();
        j.set_wait_for_right(true, &mut out);
        j.add(entry("FIX", "quanheej", "quan hệ", &["mối"]), Instant::now(), &mut out);
        assert!(out.is_empty());
        for word in ["rất", "chặt"] {
            j.word_finished(word, false, &mut out);
        }
        assert!(out.is_empty());
        j.word_finished("chẽ", false, &mut out);
        assert_eq!(out, ["FIX\tquanheej\tquan hệ\tmối\tvi\tNormal\tmối\trất chặt chẽ"]);
        assert_eq!(j.waiting(), 0);
    }

    #[test]
    fn a_line_stops_waiting_at_the_end_of_a_sentence_or_when_the_text_is_lost() {
        let mut j = Journal::default();
        let mut out = Vec::new();
        j.set_wait_for_right(true, &mut out);
        j.add(entry("FIX", "teh", "the", &[]), Instant::now(), &mut out);
        j.word_finished("end", true, &mut out); // typed before a full stop
        assert_eq!(out, ["FIX\tteh\tthe\t\tvi\tNormal\t\tend"]);
        out.clear();
        j.add(entry("FIX", "adn", "and", &["x"]), Instant::now(), &mut out);
        j.flush(&mut out); // Enter, a click, another window
        assert_eq!(out, ["FIX\tadn\tand\tx\tvi\tNormal\tx\t"]);
    }

    #[test]
    fn a_line_that_waited_too_long_is_written_and_turning_the_wait_off_flushes() {
        let mut j = Journal::default();
        let mut out = Vec::new();
        let start = Instant::now();
        j.set_wait_for_right(true, &mut out);
        j.add(entry("FIX", "a", "b", &[]), start, &mut out);
        j.expire(start + Duration::from_secs(30), &mut out);
        assert!(out.is_empty());
        j.expire(start + Duration::from_secs(61), &mut out);
        assert_eq!(out.len(), 1);
        out.clear();
        j.add(entry("FIX", "c", "d", &[]), start, &mut out);
        j.set_wait_for_right(false, &mut out);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn no_more_than_a_few_lines_wait_at_once() {
        let mut j = Journal::default();
        let mut out = Vec::new();
        j.set_wait_for_right(true, &mut out);
        for n in 0..MAX_WAITING + 3 {
            j.add(entry("FIX", &format!("k{n}"), "f", &[]), Instant::now(), &mut out);
        }
        assert_eq!(j.waiting(), MAX_WAITING);
        assert_eq!(out.len(), 3);
        assert!(out[0].starts_with("FIX\tk0\t"));
    }
}
