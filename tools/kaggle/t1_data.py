"""T1 teacher: the data side, plain Python (no torch), so it can be tested anywhere.

What the teacher learns (RESEARCH.md, section 5): for every Vietnamese syllable of a phrase as it
stands on screen, either KEEP it or replace it with another syllable of the vocabulary, plus the
kind of mistake. Training examples are windows of tokens that end where the typist is: the last four
positions of a window have 3, 2, 1 and 0 words to their right, as when a word is typed, and
a whole-sentence window now and then (what a delayed pass can see).

Mistakes are made on the fly from clean sentences (`Corruptor`), with the operations and proportions
MEASURED on the real mistakes of VSEC (`mine_operations`, `KIND_WEIGHTS`), and real mistakes come from VSEC
itself (`vsec_sentence`). Viwiki-Spelling is only for testing (`viwiki_sentences`).

Not modelled yet, said plainly: the keyboard itself (Telex keys, Backspace, joined words), English
words, other kinds of app (the environment token is always `normal`).
"""

import itertools
import json
import random
import re
import unicodedata

IGNORE = -100           # label value that no loss looks at
KEEP = 0                # class 0 of the fix head
ENV_TOKEN = "<<normal>>"
WINDOW = 40             # most tokens of a window
RIGHT_SUPERVISED = 4    # a prefix window supervises its last four positions

# What a writer did to a syllable, as measured on VSEC (`classify_op`). "tone_convention" ("thoả" for "thỏa",
# in oa, oe and uy) is a convention that the corrector accepts either way, so it is neither taught nor counted
# as a mistake; "tone_place" is the real thing: the tone on the wrong vowel ("cuả" for "của").
OPERATIONS = ["tone", "tone_drop", "tone_add", "tone_place", "tone_convention", "mark_drop", "mark_add", "neighbour",
              "substitute", "omit", "double", "swap", "extra", "insert"]
# Kinds of mistake the second head tells apart; order is the label id. Class 0 is "no mistake".
KINDS = ["none", "tone", "tone_drop", "tone_add", "tone_place", "mark_drop", "mark_add", "neighbour", "substitute",
         "omit", "double", "swap", "extra", "insert"]
# Counts of each operation among the mistakes of the VSEC training split (2026-10-06, `mine_operations`):
# the writer's slips are mostly about tone and marks (about 70%), seldom about neighbouring keys.
KIND_COUNTS = {"tone": 2081, "tone_drop": 2294, "tone_add": 521, "tone_place": 236, "mark_drop": 855, "mark_add": 294,
               "neighbour": 212, "substitute": 267, "omit": 1285, "double": 108, "swap": 18, "extra": 176, "insert": 298}
KIND_WEIGHTS = {k: v / sum(KIND_COUNTS.values()) for k, v in KIND_COUNTS.items()}

# Held keys. VSEC (text written by people) has a letter typed twice in 1.2% of its mistakes and never a longer run,
# but a key held down or bouncing types "nguuu" for "ngu" (reported by the user, 2026-10-07; not measured). The
# Corruptor therefore makes `double` more often than VSEC has it, and a share of those runs of 3 to 5 letters.
# These two numbers are assumptions to revisit with the user's own typing (tools/kaggle/AUGMENT_RULES.md, B).
TRAIN_BOOST = {"double": 2.0}
RUN_SHARE = 0.35

TONE_MARKS = ["́", "̀", "̉", "̃", "̣"]   # sac, huyen, hoi, nga, nang
VOWEL_MARKS = ["̂", "̆", "̛"]                      # hat, breve, horn
VOWELS = set("aeiouyăâêôơư")
TOKEN = re.compile(r"\w+|[^\w\s]", re.UNICODE)


# ----------------------------------------------------------------------------- Vietnamese letters

def groups_of(syllable):
    """NFD groups: [base letter, [combining marks]] for each letter."""
    out = []
    for ch in unicodedata.normalize("NFD", syllable):
        if unicodedata.combining(ch) and out:
            out[-1][1].append(ch)
        else:
            out.append([ch, []])
    return out


def join(groups):
    return unicodedata.normalize("NFC", "".join(b + "".join(m) for b, m in groups))


def bare(word):
    """Letters without any mark or tone: "việt" -> "viet"."""
    folded = "".join(ch for ch in unicodedata.normalize("NFD", word.lower()) if not unicodedata.combining(ch))
    return folded.replace("đ", "d")


def _is_vowel(group):
    base = unicodedata.normalize("NFC", group[0] + "".join(m for m in group[1] if m in VOWEL_MARKS)).lower()
    return base in VOWELS


def _tone_index(groups):
    """Which letter carries (or would carry) the tone: the modern rule, roughly."""
    vowels = [i for i, g in enumerate(groups) if _is_vowel(g)]
    # "qu" and "gi" are consonants: the u of "quý" and the i of "gì" carry no tone.
    if len(vowels) > 1 and vowels[0] > 0:
        before = groups[vowels[0] - 1][0].lower()
        if (before, groups[vowels[0]][0].lower()) in (("q", "u"), ("g", "i")):
            vowels = vowels[1:]
    if not vowels:
        return None
    marked = [i for i in vowels if any(m in VOWEL_MARKS for m in groups[i][1])]
    if marked:
        return marked[-1]
    if len(vowels) == 1:
        return vowels[0]
    # Traditional placement, as the corrector writes it ("hòa", "thủy"): the tone goes on the
    # first vowel of an open syllable, on the second when a consonant closes it ("toán").
    ends_in_consonant = vowels[-1] != len(groups) - 1
    return vowels[1] if ends_in_consonant else vowels[0]


def tone_of(groups):
    for i, (_, marks) in enumerate(groups):
        for m in marks:
            if m in TONE_MARKS:
                return i, m
    return None


def with_tone(syllable, tone):
    """The syllable with `tone` (a combining mark, or None for no tone)."""
    old = tone_of(groups_of(syllable))
    groups = [[b, [m for m in marks if m not in TONE_MARKS]] for b, marks in groups_of(syllable)]
    index = old[0] if old else _tone_index(groups)
    if tone is not None and index is not None:
        groups[index][1].append(tone)
    return join(groups)


def drop_vowel_mark(syllable, rng=random):
    groups = groups_of(syllable)
    where = [i for i, (b, marks) in enumerate(groups) if any(m in VOWEL_MARKS for m in marks) or b.lower() == "đ"]
    if not where:
        return None
    i = where[0] if len(where) == 1 else rng.choice(where)
    if groups[i][0] in ("đ", "Đ"):
        groups[i][0] = "d" if groups[i][0] == "đ" else "D"
    groups[i][1] = [m for m in groups[i][1] if m not in VOWEL_MARKS]
    return join(groups)


def move_tone(syllable, rng=random):
    """The syllable with its tone on another vowel: "của" -> "cuả"."""
    groups = groups_of(syllable)
    old = tone_of(groups)
    if not old:
        return None
    others = [i for i, g in enumerate(groups) if _is_vowel(g) and i != old[0]]
    if not others:
        return None
    to = rng.choice(others)
    groups[old[0]][1] = [m for m in groups[old[0]][1] if m != old[1]]
    groups[to][1].append(old[1])
    return join(groups)


def add_vowel_mark(syllable, rng=random):
    """The syllable with a hat, breve or horn (or the stroke of đ) it does not have: a -> â, o -> ơ, d -> đ."""
    groups = groups_of(syllable)
    options = []
    for i, (b, marks) in enumerate(groups):
        if any(m in VOWEL_MARKS for m in marks):
            continue
        low = b.lower()
        for mark in {"a": ["̂", "̆"], "e": ["̂"], "o": ["̂", "̛"], "u": ["̛"]}.get(low, []):
            options.append((i, mark))
        if low == "d" and i == 0:
            options.append((i, "stroke"))
    if not options:
        return None
    i, mark = rng.choice(options)
    if mark == "stroke":
        groups[i][0] = "đ" if groups[i][0] == "d" else "Đ"
    else:
        groups[i][1].append(mark)
    return join(groups)


QWERTY = ["qwertyuiop", "asdfghjkl", "zxcvbnm"]


def neighbours(c):
    c = c.lower()
    out = []
    for r, row in enumerate(QWERTY):
        if c not in row:
            continue
        i = row.index(c)
        out += [row[j] for j in (i - 1, i + 1) if 0 <= j < len(row)]
        for rr in (r - 1, r + 1):
            if 0 <= rr < len(QWERTY):
                out += [QWERTY[rr][j] for j in (i, i + 1) if 0 <= j < len(QWERTY[rr])]
    return out


# ----------------------------------------------------------------------------- vocabulary

def load_vocab(path, size=8000):
    """Syllables of `data/vi_syllables.tsv` (word, count per billion), most common first."""
    words = []
    for line in open(path, encoding="utf-8"):
        if line.startswith("#") or "\t" not in line:
            continue
        word, count = line.rstrip("\n").split("\t")[:2]
        word = unicodedata.normalize("NFC", word.lower())
        if word.isalpha():
            words.append((float(count), word))
    words.sort(reverse=True)
    return [w for _, w in words[:size]]


class Siblings:
    """Valid syllables one slip away from a syllable: same letters with other marks or tone
    (`bare`), or one letter added, left out or changed (`edit`)."""

    def __init__(self, vocab):
        by_bare, by_key = {}, {}
        for w in vocab:
            by_bare.setdefault(bare(w), []).append(w)
            for key in {w} | {w[:i] + w[i + 1:] for i in range(len(w))}:
                by_key.setdefault(key, []).append(w)
        self.by_bare, self.by_key = by_bare, by_key

    def bare(self, w):
        return [x for x in self.by_bare.get(bare(w), []) if x != w]

    def edit(self, w):
        out = set()
        for key in {w} | {w[:i] + w[i + 1:] for i in range(len(w))}:
            out.update(self.by_key.get(key, []))
        return [x for x in out if x != w and bare(x) != bare(w)]


# ----------------------------------------------------------------------------- real mistakes

def _convention_nucleus(groups):
    """The vowels are oa, oe or uy (not the u of "qu"): where the tone goes is a matter of style."""
    letters = [b.lower() for b, _ in groups]
    for i in range(len(letters) - 1):
        pair = letters[i] + letters[i + 1]
        if pair in ("oa", "oe") or (pair == "uy" and not (i > 0 and letters[i - 1] == "q")):
            return True
    return False


def _untoned(groups):
    return [[b, [m for m in marks if m not in TONE_MARKS]] for b, marks in groups]


def _runs(s):
    return [(letter, len(list(group))) for letter, group in itertools.groupby(s)]


def _repeat_of(w, r):
    """True if `w` is `r` with the letters of exactly one run typed more times ("nguuu" for "ngu")."""
    rw, rr = _runs(w), _runs(r)
    if len(rw) != len(rr) or any(a[0] != b[0] for a, b in zip(rw, rr)):
        return False
    differ = [(a[1], b[1]) for a, b in zip(rw, rr) if a[1] != b[1]]
    return len(differ) == 1 and differ[0][0] > differ[0][1]


def classify_op(wrong, right):
    """The operation that turns `right` (what was meant) into `wrong` (what was written), one of
    OPERATIONS, or None if they are the same or too far apart to call it one slip."""
    w, r = unicodedata.normalize("NFC", wrong.lower()), unicodedata.normalize("NFC", right.lower())
    if w == r or not w or not r:
        return None
    if bare(w) == bare(r):
        gw, gr = groups_of(w), groups_of(r)
        tw, tr = tone_of(gw), tone_of(gr)
        if tw and tr and tw[1] == tr[1] and tw[0] != tr[0] and _untoned(gw) == _untoned(gr):
            # Only the letter that carries the tone differs. In oa, oe and uy both places are in use.
            return "tone_convention" if _convention_nucleus(gw) else "tone_place"
        if tr and not tw:
            return "tone_drop"
        if tw and not tr:
            return "tone_add"
        if tw and tr and tw[1] != tr[1]:
            return "tone"

        def marks(gs):
            return sum(1 for b, m in gs if any(x in VOWEL_MARKS for x in m)) + sum(1 for b, _ in gs if b.lower() == "đ")

        return "mark_drop" if marks(gw) < marks(gr) else "mark_add"
    if len(w) > len(r) + 1 and _repeat_of(w, r):
        return "double"
    if abs(len(w) - len(r)) > 1 or (len(w) == len(r) and sum(a != b for a, b in zip(w, r)) > 2):
        return None
    if len(w) == len(r):
        diff = [i for i in range(len(w)) if w[i] != r[i]]
        if len(diff) == 2 and diff[1] == diff[0] + 1 and w[diff[0]] == r[diff[1]] and w[diff[1]] == r[diff[0]]:
            return "swap"
        if len(diff) == 1:
            return "neighbour" if bare(w[diff[0]]) in neighbours(bare(r[diff[0]])) else "substitute"
        return None
    if len(w) == len(r) - 1:
        return "omit" if any(r[:i] + r[i + 1:] == w for i in range(len(r))) else None
    for i in range(len(w)):
        if w[:i] + w[i + 1:] == r:
            c = w[i]
            if (i > 0 and w[i - 1] == c) or (i + 1 < len(w) and w[i + 1] == c):
                return "double"
            near = neighbours(bare(c)) if bare(c) else []
            return "extra" if any(bare(w[j]) in near for j in (i - 1, i + 1) if 0 <= j < len(w)) else "insert"
    return None


def mine_operations(records, vocab_ids):
    """From VSEC records: how often each operation occurs, how often its result is a syllable of the
    vocabulary, and the confusions seen for each syllable.

    Returns (op counts, valid counts, confusions) with confusions[right][wrong] = times seen.
    Only mistakes with one annotated fix and a recognisable operation count."""
    ops, valid, confusions = {}, {}, {}
    for r in records:
        for a in r["annotations"]:
            if a["is_correct"] or len(a["alternative_syllables"]) != 1:
                continue
            wrong, right = _core(a["current_syllable"])[1], _core(a["alternative_syllables"][0])[1]
            op = classify_op(wrong, right)
            if op is None:
                continue
            ops[op] = ops.get(op, 0) + 1
            right, wrong = unicodedata.normalize("NFC", right.lower()), unicodedata.normalize("NFC", wrong.lower())
            if wrong in vocab_ids:
                valid[op] = valid.get(op, 0) + 1
            confusions.setdefault(right, {})
            confusions[right][wrong] = confusions[right].get(wrong, 0) + 1
    return ops, valid, confusions


# ----------------------------------------------------------------------------- mistakes

class Corruptor:
    """Makes one realistic mistake on a syllable: an operation drawn with the frequencies measured on
    VSEC (`KIND_WEIGHTS`), applied the way a writer would."""

    def __init__(self, vocab, seed=0, weights=None):
        self.rng = random.Random(seed)
        w = weights or {k: v * TRAIN_BOOST.get(k, 1.0) for k, v in KIND_WEIGHTS.items()}
        self.kinds = list(w)
        self.weights = [w[k] for k in self.kinds]

    def one(self, syllable):
        """(mistake, kind) for a lower-case syllable, or None if nothing could be done to it."""
        for _ in range(8):
            kind = self.rng.choices(self.kinds, self.weights)[0]
            out = self.make(syllable, kind)
            if out and out != syllable:
                return out, kind
        return None

    def make(self, w, kind):
        rng = self.rng
        groups = groups_of(w)
        old = tone_of(groups)
        if kind == "tone":
            return with_tone(w, rng.choice([t for t in TONE_MARKS if t != old[1]])) if old else None
        if kind == "tone_drop":
            return with_tone(w, None) if old else None
        if kind == "tone_add":
            return with_tone(w, rng.choice(TONE_MARKS)) if not old and _tone_index(groups) is not None else None
        if kind == "tone_place":
            out = move_tone(w, rng)
            return out if out and classify_op(out, w) == "tone_place" else None
        if kind == "mark_drop":
            return drop_vowel_mark(w, rng)
        if kind == "mark_add":
            return add_vowel_mark(w, rng)
        n = len(groups)
        i = rng.randrange(n)
        if kind == "neighbour":
            near = neighbours(groups[i][0])
            if not near:
                return None
            groups[i][0] = rng.choice(near)
        elif kind == "substitute":
            groups[i][0] = rng.choice("abcdefghiklmnopqrstuvxy")
        elif kind == "omit":
            if n < 3:
                return None
            del groups[i]
        elif kind == "double":
            times = rng.choices([2, 3, 4], [60, 25, 15])[0] if rng.random() < RUN_SHARE else 1
            for _ in range(times):
                groups.insert(i, [groups[i][0], []])
        elif kind == "swap":
            if n < 2:
                return None
            i = min(i, n - 2)
            groups[i], groups[i + 1] = groups[i + 1], groups[i]
        elif kind == "extra":
            near = neighbours(groups[i][0])
            if not near:
                return None
            groups.insert(i + rng.randrange(2), [rng.choice(near), []])
        elif kind == "insert":
            groups.insert(i, [rng.choice("abcdefghiklmnopqrstuvxy"), []])
        return join(groups)


def match_case(model, text):
    if model.isupper() and len(model) > 1:
        return text.upper()
    if model[:1].isupper():
        return text[:1].upper() + text[1:]
    return text


# ----------------------------------------------------------------------------- examples

class Example:
    """One window: tokens as they stand on screen, per-token labels, and which positions count."""

    __slots__ = ("tokens", "fix", "kind", "supervised")

    def __init__(self, tokens, fix, kind, supervised):
        self.tokens, self.fix, self.kind, self.supervised = tokens, fix, kind, supervised


def split_tokens(text):
    return TOKEN.findall(unicodedata.normalize("NFC", text))


def kind_id(op):
    """Label of a kind of mistake; IGNORE for an operation the head does not tell apart."""
    return KINDS.index(op) if op in KINDS else IGNORE


def corrupt_tokens(tokens, corruptor, vocab_ids, rate):
    """(screen tokens, fix labels, kind labels) for a clean sentence. A syllable of the
    vocabulary becomes a mistake with chance `rate`; anything else stays as it is."""
    out, fix, kind = [], [], []
    for tok in tokens:
        low = tok.lower()
        if low in vocab_ids and corruptor.rng.random() < rate:
            made = corruptor.one(low)
            if made:
                text, k = made
                out.append(match_case(tok, text))
                fix.append(vocab_ids[low])
                kind.append(kind_id(k))
                continue
        out.append(tok)
        fix.append(KEEP)
        kind.append(0)
    return out, fix, kind


def make_window(tokens, fix, kind, end, whole, rng):
    """The example that ends at token `end` (exclusive), or a whole sentence."""
    if whole:
        lo, hi = 0, len(tokens)
        if hi > WINDOW:
            lo = rng.randrange(hi - WINDOW + 1)
            hi = lo + WINDOW
        sup = list(range(1, hi - lo + 1))
    else:
        hi = end
        lo = max(0, hi - WINDOW)
        sup = list(range(max(1, hi - lo - RIGHT_SUPERVISED + 1), hi - lo + 1))
    # position 0 of the model input is the environment token: everything shifts by one
    return Example([ENV_TOKEN] + tokens[lo:hi], [IGNORE] + fix[lo:hi], [IGNORE] + kind[lo:hi], sup)


def synthetic_example(tokens, corruptor, vocab_ids, rng, whole_probability=0.4):
    """A training example from a clean sentence: fresh mistakes every time it is read."""
    rate = rng.choice([0.01, 0.03, 0.06, 0.10, 0.15])
    screen, fix, kind = corrupt_tokens(tokens, corruptor, vocab_ids, rate)
    n = len(tokens)
    whole = n <= 4 or rng.random() < whole_probability
    return make_window(screen, fix, kind, n if n <= 3 else rng.randint(3, n), whole, rng)


def llm_negative_example(rec, vocab_ids, rng):
    """A training window from a record of `t2_lm_negatives.py`: one syllable changed to a worse one,
    the original as the label. The window ends 0 to 3 words after the mistake (what the typist and a
    delayed pass see), or is the whole sentence."""
    tokens, pos = rec["tokens"], rec["pos"]
    fix, kind = [KEEP] * len(tokens), [0] * len(tokens)
    fix[pos] = vocab_ids.get(rec["gold"], IGNORE)
    kind[pos] = kind_id(rec["kind"])
    end = min(len(tokens), pos + 1 + rng.randint(0, RIGHT_SUPERVISED - 1))
    return make_window(tokens, fix, kind, end, rng.random() < 0.4, rng)


def _core(s):
    m = re.match(r"^(\W*)(.*?)(\W*)$", s, re.S)
    return m.group(1), m.group(2), m.group(3)


def vsec_sentence(record, vocab_ids):
    """(screen tokens, fix labels, kind labels, is_error flags) for one VSEC record. A mistake whose
    right syllable is not in the vocabulary is left unlabelled (IGNORE), not taught as KEEP. A tone
    placed the other way in oa, oe or uy ("thoả" for "thỏa") is a convention, not a mistake: no label,
    not counted."""
    tokens, fix, kind, error = [], [], [], []

    def plain(tok):
        tokens.append(tok), fix.append(KEEP), kind.append(0), error.append(False)

    for a in record["annotations"]:
        lead, core, trail = _core(a["current_syllable"])
        for ch in lead:
            plain(ch)
        if core:
            if a["is_correct"]:
                plain(core)
            else:
                alt = (a["alternative_syllables"] or [""])[0]
                right = unicodedata.normalize("NFC", _core(alt)[1].lower())
                op = classify_op(core, right) if len(a["alternative_syllables"]) == 1 else None
                tokens.append(core)
                if op == "tone_convention":
                    fix.append(IGNORE), kind.append(IGNORE), error.append(False)
                else:
                    fix.append(vocab_ids.get(right, IGNORE))
                    kind.append(kind_id(op))
                    error.append(True)
        for ch in trail:
            plain(ch)
    return tokens, fix, kind, error


def viwiki_sentences(doc, vocab_ids):
    """Sentences of one Viwiki-Spelling document as (tokens, fix, kind, is_error) lists."""
    text = doc["text"]
    mistakes = {}
    for m in doc.get("mistakes", []):
        try:
            mistakes[int(m["start_offset"])] = (m["text"], m.get("suggest") or [])
        except (KeyError, ValueError):
            pass
    sentences, cur = [], ([], [], [], [])
    last_end = 0
    for m in TOKEN.finditer(text):
        if "\n" in text[last_end:m.start()] and cur[0]:
            sentences.append(cur)
            cur = ([], [], [], [])
        last_end = m.end()
        tok = m.group()
        wrong = mistakes.get(m.start())
        wrong = wrong if wrong and wrong[0] == tok else None
        fix, op = KEEP, None
        if wrong:
            fix = IGNORE
            if wrong[1]:
                right = unicodedata.normalize("NFC", wrong[1][0].lower())
                fix = vocab_ids.get(right, IGNORE)
                op = classify_op(tok, right)
        cur[0].append(tok)
        cur[1].append(fix)
        cur[2].append(kind_id(op) if wrong else 0)
        cur[3].append(bool(wrong))
        if tok in ".!?":
            sentences.append(cur)
            cur = ([], [], [], [])
    if cur[0]:
        sentences.append(cur)
    return sentences


def eval_examples(tokens, fix, kind, error, right):
    """One example per token with exactly `right` words to its right (the delayed pass sees 1, the
    immediate pass 0). Returns (Example, is_error) pairs."""
    out = []
    for j in range(len(tokens)):
        end = j + right + 1
        if end > len(tokens):
            break
        lo = max(0, end - WINDOW)
        # token j sits at index j - lo + 1 of the model input (the environment token is first)
        ex = Example([ENV_TOKEN] + tokens[lo:end], [IGNORE] + fix[lo:end], [IGNORE] + kind[lo:end], [j - lo + 1])
        out.append((ex, error[j]))
    return out


# ----------------------------------------------------------------------------- measuring

def curve(rows, thresholds=(0.5, 0.7, 0.8, 0.9, 0.95, 0.98, 0.99)):
    """Precision and recall of the model's changes at each confidence threshold.

    rows: (p_change, predicted fix id, gold fix id, is_error) per scored token. A change is "right"
    when it is the gold fix. Mistakes with an unknown fix (IGNORE) count as mistakes to detect but
    cannot be made right. Returns dicts, one per threshold, ready to print."""
    mistakes = sum(1 for r in rows if r[3])
    clean = len(rows) - mistakes
    out = []
    for tau in thresholds:
        changed = [r for r in rows if r[0] >= tau]
        detected = sum(1 for r in changed if r[3])
        right = sum(1 for r in changed if r[3] and r[2] != IGNORE and r[1] == r[2])
        out.append({
            "threshold": tau,
            "changed": len(changed),
            "detect_precision": detected / max(1, len(changed)),
            "detect_recall": detected / max(1, mistakes),
            "right_precision": right / max(1, len(changed)),
            "right_recall": right / max(1, mistakes),
            "false_per_1000": 1000.0 * (len(changed) - detected) / max(1, clean),
        })
    return out


def format_curve(name, rows):
    lines = [f"{name}: {len(rows)} words, {sum(1 for r in rows if r[3])} mistakes",
             f"{'tau':>6}{'changed':>9}{'detect-P':>10}{'detect-R':>10}{'right-P':>9}{'right-R':>9}{'false/1000':>12}"]
    for c in curve(rows):
        lines.append(f"{c['threshold']:>6}{c['changed']:>9}{100*c['detect_precision']:>9.1f}%{100*c['detect_recall']:>9.1f}%"
                     f"{100*c['right_precision']:>8.1f}%{100*c['right_recall']:>8.1f}%{c['false_per_1000']:>12.2f}")
    return "\n".join(lines)


def read_jsonl(path, limit=None):
    out = []
    with open(path, encoding="utf-8") as f:
        for i, line in enumerate(f):
            if limit and i >= limit:
                break
            if line.strip():
                out.append(json.loads(line))
    return out
