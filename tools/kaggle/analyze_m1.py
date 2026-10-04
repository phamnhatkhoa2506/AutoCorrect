"""Reads the export and the teacher's scores and compares three ways of choosing
a candidate, per (set, class): the app's n-gram score (`s` in the export), the
teacher with the left context only (what the app can see), and the teacher with
the right context too (what a delayed correction could see).

    python tools/kaggle/analyze_m1.py export.jsonl teacher_scores.jsonl [report.csv]

Standard library only. Accuracy is over samples whose right answer is among the
candidates; `cover` says how many that is. The last table is the safe-operation
view: fix only when the chosen candidate's probability is at least tau.
"""
import csv
import json
import math
import random
import sys
from collections import defaultdict

METHODS = ("ngram", "teacher_left", "teacher_full")


def load(export_path, scores_path):
    scores = {}
    with open(scores_path, encoding="utf-8") as f:
        for line in f:
            row = json.loads(line)
            scores[row["id"]] = row
    samples = []
    with open(export_path, encoding="utf-8") as f:
        for line in f:
            sample = json.loads(line)
            if sample["id"] in scores:
                sample["teacher"] = scores[sample["id"]]
                samples.append(sample)
    return samples


def methods(sample):
    return {
        "ngram": [c["s"] for c in sample["candidates"]],
        "teacher_left": sample["teacher"]["ll_left"],
        "teacher_full": sample["teacher"]["ll_full"],
    }


def truth_index(sample):
    for i, c in enumerate(sample["candidates"]):
        if c["t"].lower() == sample["truth"].lower():
            return i
    return None


def keep_index(sample):
    return next(i for i, c in enumerate(sample["candidates"]) if c.get("keep"))


def best(values):
    return max(range(len(values)), key=lambda i: values[i])


def softmax_top(values):
    top = best(values)
    peak = values[top]
    return top, 1.0 / sum(math.exp(v - peak) for v in values)


def bootstrap_diff(pairs, rounds=1000, seed=1):
    """95% interval of mean(a - b) over (a, b) pairs of 0/1 outcomes."""
    rng = random.Random(seed)
    n = len(pairs)
    diffs = []
    for _ in range(rounds):
        draw = [pairs[rng.randrange(n)] for _ in range(n)]
        diffs.append(sum(a - b for a, b in draw) / n)
    diffs.sort()
    return diffs[int(0.025 * rounds)], diffs[int(0.975 * rounds)]


def main():
    export_path, scores_path = sys.argv[1], sys.argv[2]
    report_path = sys.argv[3] if len(sys.argv) > 3 else "m1_report.csv"
    samples = load(export_path, scores_path)
    print(len(samples), "samples with teacher scores\n")

    by_group = defaultdict(list)
    for s in samples:
        by_group[(s["set"], s["class"])].append(s)

    rows = []
    print("Share of samples where the method picks the right candidate (only samples whose answer is a candidate)")
    print(f"{'set':3} {'class':10} {'n':>6} {'cover':>6}  {'ngram':>6} {'t.left':>7} {'t.full':>7}   left-ngram (95% CI)")
    for (set_name, cls), group in sorted(by_group.items()):
        usable = [s for s in group if truth_index(s) is not None]
        if not usable:
            continue
        hits = {m: [] for m in METHODS}
        for s in usable:
            want = truth_index(s)
            for m, values in methods(s).items():
                hits[m].append(1 if best(values) == want else 0)
        acc = {m: sum(v) / len(v) for m, v in hits.items()}
        lo, hi = bootstrap_diff(list(zip(hits["teacher_left"], hits["ngram"])))
        cover = len(usable) / len(group)
        print(
            f"{set_name:3} {cls:10} {len(usable):6} {cover:6.1%}  {acc['ngram']:6.1%} {acc['teacher_left']:7.1%} "
            f"{acc['teacher_full']:7.1%}   {acc['teacher_left'] - acc['ngram']:+.1%} ({lo:+.1%}, {hi:+.1%})"
        )
        rows.append(["accuracy", set_name, cls, len(usable), round(cover, 4)] + [round(acc[m], 4) for m in METHODS] + [round(lo, 4), round(hi, 4)])

    # Safe operation: fix only when the best candidate is not "keep" and has probability >= tau.
    print("\nSafe operation (all sets together): fixes made / right / wrong, and clean words changed per 1000")
    print(f"{'method':13} {'tau':>5} {'fixes':>7} {'right':>7} {'wrong':>7} {'clean changed/1000':>19}")
    clean_total = sum(1 for s in samples if s["class"] == "clean")
    for m in METHODS:
        for tau in (0.5, 0.7, 0.9, 0.97, 0.99):
            fixes = right = wrong = clean_changed = 0
            for s in samples:
                top, prob = softmax_top(methods(s)[m])
                if top == keep_index(s) or prob < tau:
                    continue
                fixes += 1
                if truth_index(s) == top:
                    right += 1
                else:
                    wrong += 1
                    clean_changed += s["class"] == "clean"
            per_1000 = 1000 * clean_changed / max(clean_total, 1)
            print(f"{m:13} {tau:5.2f} {fixes:7} {right:7} {wrong:7} {per_1000:19.2f}")
            rows.append(["safe", m, tau, fixes, right, wrong, round(per_1000, 3), "", ""])

    with open(report_path, "w", encoding="utf-8", newline="") as f:
        csv.writer(f).writerows(rows)
    print("\nwrote", report_path)


if __name__ == "__main__":
    main()
