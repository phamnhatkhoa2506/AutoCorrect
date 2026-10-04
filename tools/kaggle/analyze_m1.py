"""Reads the export (and, optionally, the teacher's scores) and compares ways of choosing
a candidate, per (set, class):

* ngram:        the app's n-gram score (`s`), left context only
* ngram_right:  `s` plus the n-gram probability of the words that follow (`r`)
* teacher_left / teacher_full: the LLM with left only / with the right context too

    python tools/kaggle/analyze_m1.py export.jsonl teacher_scores.jsonl [report.csv]
    python tools/kaggle/analyze_m1.py export.jsonl - [report.csv]        (no teacher scores)

Candidates flagged `x` (letters changed in a well-formed syllable, which the app does not do)
are left out of the first two tables and measured separately in the third. Standard library only.
"""
import csv
import json
import math
import random
import sys
from collections import defaultdict

NEG = -1e9


def is_x(c):
    return bool(c.get("x"))


def load(export_path, scores_path):
    scores = {}
    if scores_path != "-":
        with open(scores_path, encoding="utf-8") as f:
            for line in f:
                row = json.loads(line)
                scores[row["id"]] = row
    samples = []
    with open(export_path, encoding="utf-8") as f:
        for line in f:
            sample = json.loads(line)
            if scores_path == "-":
                samples.append(sample)
            elif sample["id"] in scores:
                sample["teacher"] = scores[sample["id"]]
                samples.append(sample)
    return samples


def methods(sample, include_x=False):
    cands = sample["candidates"]

    def mask(values):
        values = list(values)
        values += [NEG] * (len(cands) - len(values))
        return values if include_x else [NEG if is_x(c) else v for c, v in zip(cands, values)]

    out = {
        "ngram": mask(c["s"] for c in cands),
        "ngram_right": mask(c["s"] + c.get("r", 0.0) for c in cands),
    }
    teacher = sample.get("teacher")
    if teacher:
        out["teacher_left"] = mask(teacher["ll_left"])
        out["teacher_full"] = mask(teacher["ll_full"])
    return out


def truth_index(sample, include_x=False):
    for i, c in enumerate(sample["candidates"]):
        if (include_x or not is_x(c)) and c["t"].lower() == sample["truth"].lower():
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


def safe_row(samples, values_of, tau, clean_total, include_x=False):
    fixes = right = wrong = clean_changed = 0
    for s in samples:
        top, prob = softmax_top(values_of(s))
        if top == keep_index(s) or prob < tau:
            continue
        fixes += 1
        if truth_index(s, include_x) == top:
            right += 1
        else:
            wrong += 1
            clean_changed += s["class"] == "clean"
    return fixes, right, wrong, 1000 * clean_changed / max(clean_total, 1)


def main():
    export_path, scores_path = sys.argv[1], sys.argv[2]
    report_path = sys.argv[3] if len(sys.argv) > 3 else "m1_report.csv"
    samples = load(export_path, scores_path)
    teacher = "teacher" in samples[0]
    names = ["ngram", "ngram_right"] + (["teacher_left", "teacher_full"] if teacher else [])
    print(len(samples), "samples\n")

    by_group = defaultdict(list)
    for s in samples:
        by_group[(s["set"], s["class"])].append(s)

    rows = []
    print("1. Share of samples where the method picks the right candidate (answer among the app's candidates)")
    print(f"{'set':3} {'class':10} {'n':>6} {'cover':>6}  " + " ".join(f"{n:>12}" for n in names))
    for (set_name, cls), group in sorted(by_group.items()):
        usable = [s for s in group if truth_index(s) is not None]
        if not usable:
            continue
        hits = {n: [] for n in names}
        for s in usable:
            want = truth_index(s)
            for n, values in methods(s).items():
                hits[n].append(1 if best(values) == want else 0)
        acc = {n: sum(v) / len(v) for n, v in hits.items()}
        cover = len(usable) / len(group)
        print(f"{set_name:3} {cls:10} {len(usable):6} {cover:6.1%}  " + " ".join(f"{acc[n]:12.1%}" for n in names))
        rows.append(["accuracy", set_name, cls, len(usable), round(cover, 4)] + [round(acc[n], 4) for n in names])

    # Expanded candidates: how many answers become reachable, and what choosing among them costs.
    has_x = any(is_x(c) for s in samples for c in s["candidates"])
    if has_x:
        print("\n2. Letters-changed candidates for well-formed syllables (ngram_right; rate = right choices / all samples of the class)")
        print(f"{'set':3} {'class':10} {'n':>6} {'answer reachable':>17} {'rate, app set':>14} {'rate, expanded':>15}")
        for (set_name, cls), group in sorted(by_group.items()):
            if cls == "clean" or not any(is_x(c) for s in group for c in s["candidates"]):
                continue
            n = len(group)
            reach_base = sum(truth_index(s) is not None for s in group) / n
            reach_all = sum(truth_index(s, True) is not None for s in group) / n
            right_base = sum(truth_index(s) is not None and best(methods(s)["ngram_right"]) == truth_index(s) for s in group) / n
            right_all = sum(
                truth_index(s, True) is not None and best(methods(s, True)["ngram_right"]) == truth_index(s, True) for s in group
            ) / n
            print(f"{set_name:3} {cls:10} {n:6} {reach_base:8.1%} -> {reach_all:5.1%} {right_base:14.1%} {right_all:15.1%}")
            rows.append(["expanded", set_name, cls, n, round(reach_base, 4), round(reach_all, 4), round(right_base, 4), round(right_all, 4)])

    print("\n3. Safe operation (all sets together): fix only when the best candidate has probability >= tau")
    print(f"{'method':22} {'tau':>8} {'fixes':>7} {'right':>7} {'wrong':>7} {'clean changed/1000':>19}")
    clean_total = sum(1 for s in samples if s["class"] == "clean")
    variants = [(n, n, False) for n in names]
    if has_x:
        variants.append(("ngram_right + expanded", "ngram_right", True))
    for label, name, include_x in variants:
        for tau in (0.5, 0.9, 0.99, 0.999, 0.9999, 0.99999):
            fixes, right, wrong, per_1000 = safe_row(samples, lambda s: methods(s, include_x)[name], tau, clean_total, include_x)
            print(f"{label:22} {tau:8g} {fixes:7} {right:7} {wrong:7} {per_1000:19.2f}")
            rows.append(["safe", label, tau, fixes, right, wrong, round(per_1000, 3)])

    with open(report_path, "w", encoding="utf-8", newline="") as f:
        csv.writer(f).writerows(rows)
    print("\nwrote", report_path)


if __name__ == "__main__":
    main()
