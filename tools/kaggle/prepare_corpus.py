"""Builds the private Kaggle dataset `autocorrect-train` for the T1 teacher (tools/kaggle/README.md).

    python tools/kaggle/prepare_corpus.py [--news 600000] [--web 600000] [--subs 200000]

Writes `tools/kaggle/upload/autocorrect-train/`:
  corpus.txt        clean sentences, one per line (public corpora only, no data of yours)
  vocab.txt         the syllables of the fix head, one per line; line i is class i + 1 (0 is KEEP)
  vsec_train.jsonl, vsec_dev.jsonl   VSEC records, split by a hash of the sentence (90% / 10%)
  viwiki_test.jsonl Viwiki-Spelling, for testing only
  t1_data.py        the data module the notebook imports
  dataset-metadata.json

It only reads files and writes the folder: nothing is uploaded. The last 20,000 lines of every
corpus are skipped (the same held-out tail `ac-bench` uses). Subtitles carry OCR mistakes ("Iắm"), so
they are a small share and the filter below drops lines with many unknown words.
"""

import argparse
import hashlib
import json
import os
import random
import shutil
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
sys.path.insert(0, HERE)

import t1_data as d  # noqa: E402

HELD_OUT = 20_000
SOURCES = {
    "news": "data/raw/vie_news_2022_1M/vie_news_2022_1M-sentences.txt",
    "web": "data/raw/vie-vn_web_2015_1M/vie-vn_web_2015_1M-sentences.txt",
    "subs": "data/raw/vie_subtitles/vie_subtitles-sentences.txt",
}


def usable(tokens, vocab_ids):
    """Four to forty words, nearly all of them known syllables: nothing to learn from names and noise."""
    words = [t for t in tokens if t.isalpha()]
    if not 4 <= len(words) <= 40:
        return False
    known = sum(1 for w in words if w.lower() in vocab_ids)
    return known >= 0.9 * len(words)


def sample(path, wanted, vocab_ids, rng):
    with open(path, encoding="utf-8") as f:
        lines = f.read().split("\n")
    lines = lines[: max(0, len(lines) - HELD_OUT)]
    rng.shuffle(lines)
    out = []
    for line in lines:
        text = line.split("\t", 1)[-1].strip()
        if text and usable(d.split_tokens(text), vocab_ids):
            out.append(text)
            if len(out) >= wanted:
                break
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--news", type=int, default=600_000)
    ap.add_argument("--web", type=int, default=600_000)
    ap.add_argument("--subs", type=int, default=200_000)
    ap.add_argument("--out", default=os.path.join(HERE, "upload", "autocorrect-train"))
    ap.add_argument("--seed", type=int, default=7)
    args = ap.parse_args()

    os.makedirs(args.out, exist_ok=True)
    vocab = d.load_vocab(os.path.join(ROOT, "data", "vi_syllables.tsv"))
    vocab_ids = {w: i + 1 for i, w in enumerate(vocab)}
    with open(os.path.join(args.out, "vocab.txt"), "w", encoding="utf-8") as f:
        f.write("\n".join(vocab) + "\n")

    rng = random.Random(args.seed)
    corpus = []
    for name, wanted in (("news", args.news), ("web", args.web), ("subs", args.subs)):
        path = os.path.join(ROOT, SOURCES[name])
        if wanted <= 0 or not os.path.exists(path):
            print(f"{name}: skipped")
            continue
        got = sample(path, wanted, vocab_ids, rng)
        print(f"{name}: {len(got)} sentences (asked {wanted})")
        corpus += got
    rng.shuffle(corpus)
    with open(os.path.join(args.out, "corpus.txt"), "w", encoding="utf-8") as f:
        f.write("\n".join(corpus) + "\n")

    vsec = d.read_jsonl(os.path.join(ROOT, "data", "raw", "vsec", "VSEC.jsonl"))
    train, dev = [], []
    for r in vsec:
        bucket = int(hashlib.md5(r["text"].encode("utf-8")).hexdigest(), 16) % 10
        (dev if bucket == 0 else train).append(r)
    for name, rows in (("vsec_train.jsonl", train), ("vsec_dev.jsonl", dev)):
        with open(os.path.join(args.out, name), "w", encoding="utf-8") as f:
            f.write("\n".join(json.dumps(r, ensure_ascii=False) for r in rows) + "\n")
    print(f"VSEC: {len(train)} train, {len(dev)} dev records")

    shutil.copy(os.path.join(ROOT, "data", "raw", "viwiki_spelling", "spelling_test.json"), os.path.join(args.out, "viwiki_test.jsonl"))
    for name in ("t1_data.py", "t2_lm_negatives.py", "t2_augment_kaggle.py"):
        shutil.copy(os.path.join(HERE, name), os.path.join(args.out, name))
    with open(os.path.join(args.out, "dataset-metadata.json"), "w", encoding="utf-8") as f:
        f.write('{\n  "title": "autocorrect-train",\n  "id": "pnk123456/autocorrect-train",\n  "licenses": [{"name": "other"}]\n}\n')
    size = sum(os.path.getsize(os.path.join(args.out, n)) for n in os.listdir(args.out)) / 1e6
    print(f"written to {args.out} ({size:.0f} MB). Nothing was uploaded.")


if __name__ == "__main__":
    main()
