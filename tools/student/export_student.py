"""Exports the trained student to the binary file the app reads (`crates/ac-core/src/student.rs`), and a few
windows with the probabilities PyTorch gives, so the Rust side can be checked against them.

    python tools/student/export_student.py [--student-dir models/student_online/student] [--out models/student.acs]

File format (little endian), all tensors float32:
    magic "ACST", u32 version = 1
    u32 buckets, ngram_cap, d_bucket, d_model, heads, layers, ff, classes
    u32 n_vocab, then n_vocab times: u16 byte length, utf-8 bytes        (class i + 1 is vocab[i]; class 0 keeps the word)
    tensors in this order, each row-major as PyTorch stores it:
        buckets [buckets, d_bucket], bucket_proj.weight [d_model, d_bucket], bucket_proj.bias [d_model],
        vocab_emb [classes, d_model], flag_emb [6, d_model], pos_emb [64, d_model],
        per layer: norm1.weight, norm1.bias [d_model], in_proj.weight [3 d_model, d_model], in_proj.bias [3 d_model],
                   out_proj.weight [d_model, d_model], out_proj.bias [d_model], norm2.weight, norm2.bias [d_model],
                   linear1.weight [ff, d_model], linear1.bias [ff], linear2.weight [d_model, ff], linear2.bias [d_model],
        norm.weight, norm.bias [d_model], fix_head.weight [classes, d_model], fix_head.bias [classes]
"""

import argparse
import json
import os
import random
import struct
import sys

import numpy as np
import torch

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(ROOT, "tools", "kaggle"))
import bench_cpu as b  # noqa: E402  (the student network and its input features)

d = b.d


def tensors(layers):
    names = ["buckets.weight", "bucket_proj.weight", "bucket_proj.bias", "vocab_emb.weight", "flag_emb.weight", "pos_emb.weight"]
    for i in range(layers):
        p = f"encoder.layers.{i}."
        names += [p + "norm1.weight", p + "norm1.bias", p + "self_attn.in_proj_weight", p + "self_attn.in_proj_bias",
                  p + "self_attn.out_proj.weight", p + "self_attn.out_proj.bias", p + "norm2.weight", p + "norm2.bias",
                  p + "linear1.weight", p + "linear1.bias", p + "linear2.weight", p + "linear2.bias"]
    names += ["norm.weight", "norm.bias", "fix_head.weight", "fix_head.bias"]
    return names


def run_inputs(run, ex):
    """The input dictionary StudentRunner builds, for one window (so the full logits can be read)."""
    width = len(ex.tokens)
    flat, offsets, vid, flag = [], [], [], []
    for tok in ex.tokens:
        offsets.append(len(flat))
        flat += run.buckets_of(tok)
        vid.append(b.vocab_ids.get(tok.lower(), 0))
        flag.append(run.flag(tok))
    return {"flat": torch.tensor(flat), "offsets": torch.tensor(offsets), "vid": torch.tensor(vid).view(1, width),
            "flag": torch.tensor(flag).view(1, width), "pad": torch.zeros((1, width), dtype=torch.bool)}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--student-dir", default=b.STUDENT_DIR)
    ap.add_argument("--out", default=os.path.join(b.MODELS, "student.acs"))
    ap.add_argument("--golden", default=os.path.join(ROOT, "crates", "ac-core", "testdata", "student_golden.json"))
    ap.add_argument("--golden-windows", type=int, default=16)
    args = ap.parse_args()
    b.STUDENT_DIR = args.student_dir
    model, cfg = b.load_student()
    sd = model.state_dict()
    layers = cfg["layers"]
    with open(args.out, "wb") as f:
        f.write(b"ACST" + struct.pack("<I", 1))
        f.write(struct.pack("<8I", cfg["buckets"], cfg["ngram_cap"], cfg["d_bucket"], cfg["d_model"], cfg["heads"], layers, cfg["ff"], cfg["classes"]))
        f.write(struct.pack("<I", len(b.vocab)))
        for w in b.vocab:
            raw = w.encode("utf-8")
            f.write(struct.pack("<H", len(raw)) + raw)
        for name in tensors(layers):
            f.write(sd[name].detach().cpu().numpy().astype("<f4").tobytes())
    print(f"{args.out}: {os.path.getsize(args.out) / 1e6:.1f} MB")

    # Windows with the probabilities PyTorch gives: real Viwiki windows, half of them mistakes.
    run = b.StudentRunner(model, cfg)
    sents = [x for doc in d.read_jsonl(os.path.join(b.DATA, "viwiki_test.jsonl")) for x in d.viwiki_sentences(doc, b.vocab_ids)]
    rng = random.Random(9)
    errs, clean = [], []
    for tokens, fix, kind, error in sents:
        for ex, is_error in d.eval_examples(tokens, fix, kind, error, 1):
            (errs if is_error else clean).append(ex)
    chosen = rng.sample(errs, args.golden_windows // 2) + rng.sample(clean, args.golden_windows - args.golden_windows // 2)
    cases = []
    with torch.no_grad():
        for ex in chosen:
            logits = model(run_inputs(run, ex))
            probs = torch.softmax(logits[0, ex.supervised[0]].float(), -1)
            best = probs.topk(5)
            cases.append({"tokens": ex.tokens, "position": ex.supervised[0], "p_keep": probs[0].item(),
                          "top": [[int(i), round(float(v), 6)] for v, i in zip(best.values, best.indices)]})
    os.makedirs(os.path.dirname(args.golden), exist_ok=True)
    json.dump(cases, open(args.golden, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    print(f"{len(cases)} golden windows -> {args.golden}")


if __name__ == "__main__":
    main()
