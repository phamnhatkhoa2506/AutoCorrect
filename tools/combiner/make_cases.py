"""Cases for the combiner experiment (RESEARCH.md, section 5, tier 1): one word per line, with the words
written before it, the word as written, the word meant, and whether it is a mistake.

    python tools/combiner/make_cases.py [--synthetic 20000]

Writes `tools/combiner/work/cases.tsv` (git-ignored). Columns, tab separated:
    source  id  history  typed  gold  is_error
`gold` is the word meant (lower case; Viwiki may list several, separated by `|`); `history` the words
before it in the same phrase, as written, up to four. Sources:

    vsec_train, vsec_dev   real mistakes of VSEC (the same 90/10 split as tools/kaggle/prepare_corpus.py)
    viwiki                 real mistakes of Viwiki-Spelling: a test set, never trained on
    synth_train, synth_dev clean public sentences with mistakes made by t1_data.Corruptor (operations and
                           proportions measured on VSEC), never the held-out tail of the corpora

A tone placed the other way in oa, oe or uy ("thoả" for "thỏa") is a convention the corrector accepts either
way: such a word counts as correct, in VSEC and in Viwiki alike. A VSEC mistake without exactly one
annotated fix is skipped (its words still serve as history).
"""

import argparse
import hashlib
import os
import random
import sys
import unicodedata

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
sys.path.insert(0, os.path.join(ROOT, "tools", "kaggle"))

import t1_data as d  # noqa: E402

HISTORY = 4


class Writer:
    def __init__(self, path):
        self.f = open(path, "w", encoding="utf-8", newline="\n")
        self.counts = {}

    def row(self, source, history, typed, gold, error):
        n = self.counts.get(source, [0, 0])
        n[0] += 1
        n[1] += int(error)
        self.counts[source] = n
        self.f.write(f"{source}\t{n[0]}\t{' '.join(history[-HISTORY:])}\t{typed}\t{gold}\t{int(error)}\n")


def vsec_rows(writer, records, source):
    for record in records:
        history = []
        for a in record["annotations"]:
            lead, core, trail = d._core(a["current_syllable"])
            if lead:
                history = []
            if core and core.isalpha():
                alts = a["alternative_syllables"]
                if a["is_correct"]:
                    writer.row(source, history, core, core.lower(), False)
                elif len(alts) == 1:
                    gold = unicodedata.normalize("NFC", d._core(alts[0])[1].lower())
                    convention = d.classify_op(core, gold) == "tone_convention"
                    writer.row(source, history, core, core.lower() if convention else gold, not convention)
                history = history + [core]
            elif core:
                history = []
            if trail:
                history = []


def viwiki_rows(writer, docs):
    for doc in docs:
        text = doc["text"]
        mistakes = {}
        for m in doc.get("mistakes", []):
            try:
                mistakes[int(m["start_offset"])] = (m["text"], m.get("suggest") or [])
            except (KeyError, ValueError):
                pass
        history, last_end = [], 0
        for m in d.TOKEN.finditer(text):
            if "\n" in text[last_end:m.start()]:
                history = []
            last_end = m.end()
            tok = m.group()
            if not tok.isalpha():
                history = []
                continue
            wrong = mistakes.get(m.start())
            wrong = wrong if wrong and wrong[0] == tok else None
            if wrong and wrong[1]:
                golds = [unicodedata.normalize("NFC", s.lower()) for s in wrong[1]]
                convention = all(d.classify_op(tok, g) == "tone_convention" for g in golds)
                writer.row("viwiki", history, tok, tok.lower() if convention else "|".join(golds), not convention)
            else:
                writer.row("viwiki", history, tok, tok.lower(), False)
            history = history + [tok]


def synthetic_rows(writer, sentences, source, vocab, rng, seed):
    corruptor = d.Corruptor(vocab, seed=seed)
    ids = set(vocab)
    for sentence in sentences:
        rate = rng.choice([0.03, 0.08, 0.15])
        history = []
        for tok in d.split_tokens(sentence):
            if not tok.isalpha():
                history = []
                continue
            typed, error = tok, False
            if tok.lower() in ids and rng.random() < rate:
                made = corruptor.one(tok.lower())
                if made and made[0].isalpha():
                    typed, error = d.match_case(tok, made[0]), True
            writer.row(source, history, typed, tok.lower(), error)
            history = history + [typed]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--synthetic", type=int, default=20000, help="sentences for synth_train (synth_dev gets a seventh)")
    ap.add_argument("--out", default=os.path.join(HERE, "work"))
    args = ap.parse_args()
    os.makedirs(args.out, exist_ok=True)

    vocab = d.load_vocab(os.path.join(ROOT, "data", "vi_syllables.tsv"))
    vsec = d.read_jsonl(os.path.join(ROOT, "data", "raw", "vsec", "VSEC.jsonl"))
    train = [r for r in vsec if int(hashlib.md5(r["text"].encode("utf-8")).hexdigest(), 16) % 10 != 0]
    dev = [r for r in vsec if int(hashlib.md5(r["text"].encode("utf-8")).hexdigest(), 16) % 10 == 0]
    corpus = os.path.join(ROOT, "tools", "kaggle", "upload", "autocorrect-train", "corpus.txt")
    lines = [l for l in open(corpus, encoding="utf-8").read().split("\n") if l.strip()]
    random.Random(11).shuffle(lines)
    n_dev = max(1, args.synthetic // 7)
    w = Writer(os.path.join(args.out, "cases.tsv"))
    vsec_rows(w, train, "vsec_train")
    vsec_rows(w, dev, "vsec_dev")
    viwiki_rows(w, d.read_jsonl(os.path.join(ROOT, "data", "raw", "viwiki_spelling", "spelling_test.json")))
    synthetic_rows(w, lines[:args.synthetic], "synth_train", vocab, random.Random(5), 5)
    synthetic_rows(w, lines[args.synthetic:args.synthetic + n_dev], "synth_dev", vocab, random.Random(6), 6)
    w.f.close()
    for source, (rows, errors) in w.counts.items():
        print(f"{source:<12}{rows:>9} words, {errors:>7} mistakes ({1000 * errors / max(1, rows):.1f} per 1000)")
    print("written to", os.path.join(args.out, "cases.tsv"))


if __name__ == "__main__":
    main()
