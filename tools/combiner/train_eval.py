"""The combiner experiment (RESEARCH.md, section 5, tier 1): does a learned decision over the same n-gram evidence
beat the hand-set thresholds, at the same number of wrong changes?

    cargo run -p ac-bench --release -- --evidence tools/combiner/work/cases.tsv tools/combiner/work/evidence.tsv --margins 1.5,2.5,3.5,5,7,10
    python tools/combiner/train_eval.py

For every word the corrector shows its evidence (`SmartCorrector::evidence`): the score of the word as typed and the
candidates of each stage (one slip, a cut into syllables, two slips). The thresholds of `correct_in` pick one or
nothing. Here a small model learns P(this candidate is the word meant) from the same numbers, and the best
candidate is taken when P reaches a confidence level. Both are measured the same way on mistakes that were
never trained on: VSEC dev, and Viwiki-Spelling.

A word counts as "changed" when the system returns another word. "Detected": changed and a mistake. "Right":
changed into the word meant. "False per 1000": changed words that were not mistakes, per 1000 words that were not.
"""

import argparse
import collections
import os
import sys
import time

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
sys.path.insert(0, os.path.join(ROOT, "tools", "kaggle"))
import t1_data as d  # noqa: E402

OPS = d.OPERATIONS + ["none"]
FIXED = 14                      # source id typed gold is_error keys vi cap skipped forced typed near_unacc far_unacc split


class Row:
    __slots__ = ("source", "typed", "golds", "error", "keys", "vi", "cap", "skipped", "forced", "typed_score",
                 "near_unacc", "far_unacc", "split", "base", "near", "far")


def parse_list(text):
    out = []
    for item in text.split("|") if text else []:
        word, score = item.rsplit(":", 1)
        out.append((word, float(score)))
    return out


def load(path, margins):
    rows = []
    with open(path, encoding="utf-8") as f:
        for line in f:
            p = line.rstrip("\n").split("\t")
            if len(p) != FIXED + len(margins) + 2:
                continue
            r = Row()
            r.source, r.typed = p[0], p[2]
            r.golds = p[3].split("|")
            r.error = p[4] == "1"
            r.keys, r.vi, r.cap, r.skipped = int(p[5]), int(p[6]), int(p[7]), p[8] == "1"
            r.forced, r.typed_score = p[9], float(p[10])
            r.near_unacc, r.far_unacc, r.split = p[11] == "1", p[12] == "1", p[13]
            r.base = p[FIXED:FIXED + len(margins)]
            r.near, r.far = parse_list(p[-2]), parse_list(p[-1])
            rows.append(r)
    return rows


def candidates(r):
    """[(text, stage, rank, score, next_gap, n_in_stage)] for the candidates the corrector listed."""
    out = []
    for stage, items, blocked in (("near", r.near[:3], r.near_unacc), ("far", r.far[:2], r.far_unacc)):
        if blocked:                       # correct_in leaves the word alone in this case
            continue
        for i, (w, s) in enumerate(items):
            nxt = items[i + 1][1] if i + 1 < len(items) else s - 10.0
            out.append((w, stage, i, s, s - nxt, len(items)))
    if r.split:
        out.append((r.split, "split", 0, 0.0, 0.0, 1))
    return out


def features(r, cand):
    w, stage, rank, score, gap, n = cand
    finite = r.typed_score > -1e9
    typed_s = r.typed_score if finite else 0.0
    over = (score - typed_s) if finite else 20.0
    op = d.classify_op(r.typed, w) or "none"
    bare_same = d.bare(w) == d.bare(r.typed)
    f = [
        float(finite), typed_s, score, over, gap, float(rank), float(n),
        float(stage == "near"), float(stage == "far"), float(stage == "split"),
        float(r.keys), float(r.vi), float(r.cap), float(len(w)), float(len(r.typed)), float(len(w) - len(r.typed)),
        float(bare_same), float(w.isascii()), float(" " in w), float(len(r.near)), float(len(r.far)),
        # Classes of wrong fixes seen in the journal (RESEARCH.md / AUGMENT_RULES.md, F): an acronym, a word
        # that starts with a tone or mark key, a cut with a one-vowel piece.
        float(len(r.typed) > 1 and r.typed.isupper()), float(r.typed[:1].lower() in "jwzfsrx"),
        float(stage == "split" and any(len(p) == 1 for p in w.split())),
    ]
    f += [float(op == o) for o in OPS]
    return f


def build(rows):
    """Feature matrix, labels, and for each row the index range of its candidates."""
    X, y, owner, texts = [], [], [], []
    for i, r in enumerate(rows):
        if r.skipped or r.forced:
            continue
        for cand in candidates(r):
            X.append(features(r, cand))
            y.append(int(cand[0].lower() in r.golds and cand[0].lower() != r.typed.lower()))
            owner.append(i)
            texts.append(cand[0])
    return np.array(X, dtype=np.float64), np.array(y), np.array(owner), texts


def same(a, b):
    return a.lower() == b.lower()


def score_system(rows, outputs):
    """(changed, detected, right, false, mistakes, clean) for the output text of each row ('' = left alone)."""
    changed = detected = right = false = mistakes = clean = 0
    for r, out in zip(rows, outputs):
        mistakes += r.error
        clean += not r.error
        if out and not same(out, r.typed):
            changed += 1
            if r.error:
                detected += 1
                right += out.lower() in r.golds
            else:
                false += 1
    return changed, detected, right, false, mistakes, clean


def fmt(name, s):
    changed, detected, right, false, mistakes, clean = s
    return (f"  {name:<34}{changed:>7}  right-P {100 * right / max(1, changed):5.1f}%  right-R {100 * right / max(1, mistakes):5.1f}%"
            f"  detect-R {100 * detected / max(1, mistakes):5.1f}%  false/1000 {1000 * false / max(1, clean):6.2f}")


def learned_outputs(rows, owner, texts, p, tau):
    best = {}
    for k, i in enumerate(owner):
        if i not in best or p[k] > best[i][0]:
            best[i] = (p[k], texts[k])
    out = []
    for i, r in enumerate(rows):
        if r.forced:
            out.append(r.forced)
        elif i in best and best[i][0] >= tau:
            out.append(best[i][1])
        else:
            out.append("")
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--evidence", default=os.path.join(HERE, "work", "evidence.tsv"))
    ap.add_argument("--margins", default="1.5,2.5,3.5,5,7,10")
    ap.add_argument("--train", default="vsec_train,synth_train")
    ap.add_argument("--test", default="vsec_dev,viwiki,synth_dev")
    ap.add_argument("--out", default="results.md")
    args = ap.parse_args()
    margins = [float(m) for m in args.margins.split(",")]
    default_at = margins.index(3.5) if 3.5 in margins else 0
    from sklearn.ensemble import HistGradientBoostingClassifier
    from sklearn.linear_model import LogisticRegression
    from sklearn.preprocessing import StandardScaler
    from sklearn.pipeline import make_pipeline

    t0 = time.time()
    rows = load(args.evidence, margins)
    by = collections.defaultdict(list)
    for r in rows:
        by[r.source].append(r)
    print(f"{len(rows)} cases in {time.time() - t0:.0f} s:", {k: len(v) for k, v in by.items()})
    train_sources = args.train.split(",")
    train_rows = [r for s in train_sources for r in by[s]]
    X, y, _, _ = build(train_rows)
    print(f"training on {train_sources}: {len(y)} candidates, {int(y.sum())} are the word meant ({100 * y.mean():.1f}%)")

    models = {
        "logistic regression": make_pipeline(StandardScaler(), LogisticRegression(C=1.0, max_iter=400)),
        "gradient boosting": HistGradientBoostingClassifier(max_iter=200, learning_rate=0.1, max_leaf_nodes=31, random_state=0),
    }
    for name, m in models.items():
        t1 = time.time()
        m.fit(X, y)
        print(f"fitted {name} in {time.time() - t1:.0f} s")

    taus = [round(0.02 + 0.01 * i, 3) for i in range(0, 97)]
    report = []
    for source in args.test.split(","):
        sub = by[source]
        Xs, ys, owner, texts = build(sub)
        report.append(f"\n== {source}: {len(sub)} words, {sum(r.error for r in sub)} mistakes ==")
        base_scores = []
        for k, m in enumerate(margins):
            s = score_system(sub, [r.base[k] for r in sub])
            base_scores.append(s)
            report.append(fmt(f"thresholds, margin {m}" + (" (default)" if k == default_at else ""), s))
        for name, model in models.items():
            p = model.predict_proba(Xs)[:, 1]
            curve = [(tau, score_system(sub, learned_outputs(sub, owner, texts, p, tau))) for tau in taus]
            report.append(f"  -- {name}: at the same wrong changes as each threshold setting")
            for k, bs in enumerate(base_scores):
                target = 1000 * bs[3] / max(1, bs[5])
                ok = [(tau, s) for tau, s in curve if 1000 * s[3] / max(1, s[5]) <= target + 1e-9]
                if ok:
                    tau, s = max(ok, key=lambda t: t[1][2])           # the most right changes within that budget
                    report.append(fmt(f"learned (confidence {tau}) ~ margin {margins[k]}", s))
                else:
                    report.append(f"  learned: no confidence level reaches false/1000 <= {target:.2f}")
    text = "\n".join(report)
    print(text)
    open(os.path.join(HERE, "work", args.out), "w", encoding="utf-8").write("```\n" + text + "\n```\n")


if __name__ == "__main__":
    main()
