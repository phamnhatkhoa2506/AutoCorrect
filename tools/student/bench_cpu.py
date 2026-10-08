"""CPU benchmark of the teacher and the student, as float32 and with int8 dynamic quantisation (RESEARCH.md, section 5).

    python tools/student/bench_cpu.py [--latency-windows 150] [--quality-errors 400] [--quality-clean 1600]

Measures on this machine, CPU only:
  * latency of one window (tokenise + forward), batch of 1, as the delayed pass in the app would call it,
    for 1, 4 and all threads; windows are real Viwiki windows with one word to the right of the word judged;
  * size of the weights;
  * quality on a sample of Viwiki-Spelling windows (all of a random part of the mistakes, plus random clean words),
    and how often the int8 model decides the same as its float32 original.
The sample's clean words are a random sample, so false changes per 1000 clean words are comparable to the
full-test numbers in RESEARCH.md; recall has less data behind it (see `--quality-errors`).
"""

import argparse
import io
import json
import os
import random
import statistics
import sys
import time
import zlib

import torch
import torch.nn as nn

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
sys.path.insert(0, os.path.join(ROOT, "tools", "kaggle"))
import t1_data as d  # noqa: E402

DATA = os.path.join(ROOT, "tools", "kaggle", "upload", "autocorrect-train")
MODELS = os.path.join(ROOT, "models")
TEACHER_PT = os.path.join(MODELS, "teacher_40k", "teacher.pt")
STUDENT_DIR = os.path.join(MODELS, "student_online", "student")

vocab = [w.strip() for w in open(os.path.join(DATA, "vocab.txt"), encoding="utf-8") if w.strip()]
vocab_ids = {w: i + 1 for i, w in enumerate(vocab)}
CLASSES = len(vocab) + 1


# ----------------------------------------------------------------------------- the teacher (as in t1_teacher_train.py)
class Teacher(nn.Module):
    def __init__(self):
        super().__init__()
        from transformers import AutoModel
        self.encoder = AutoModel.from_pretrained("FacebookAI/xlm-roberta-base")
        hidden = self.encoder.config.hidden_size
        self.fix_head = nn.Linear(hidden, CLASSES)
        self.kind_head = nn.Linear(hidden, len(d.KINDS))

    def forward(self, ids, mask):
        h = self.encoder(input_ids=ids, attention_mask=mask).last_hidden_state
        return self.fix_head(h), self.kind_head(h)


class TeacherRunner:
    def __init__(self, model):
        from transformers import AutoTokenizer
        self.model = model.eval()
        self.tok = AutoTokenizer.from_pretrained("FacebookAI/xlm-roberta-base")

    @torch.no_grad()
    def __call__(self, examples):
        """[(p_change, predicted fix id)] for (Example, ...) windows with one supervised word each."""
        enc = self.tok([e.tokens for e in examples], is_split_into_words=True, truncation=True, max_length=128,
                       padding=True, return_tensors="pt")
        logits, _ = self.model(enc["input_ids"], enc["attention_mask"])
        probs = torch.softmax(logits.float(), dim=-1)
        out = []
        for b, e in enumerate(examples):
            first, previous = {}, None
            for sub, word in enumerate(enc.word_ids(b)):
                if word is not None and word != previous:
                    first[word] = sub
                previous = word
            p = probs[b, first[e.supervised[0]]] if e.supervised[0] in first else None
            out.append((0.0, 0) if p is None else (1.0 - p[d.KEEP].item(), int(p[1:].argmax().item()) + 1))
        return out


# ----------------------------------------------------------------------------- the student (as in student_train.py)
def load_student():
    cfg = json.load(open(os.path.join(STUDENT_DIR, "student_config.json")))
    B, DB, DM = cfg["buckets"], cfg["d_bucket"], cfg["d_model"]

    class Student(nn.Module):
        def __init__(self):
            super().__init__()
            self.buckets = nn.EmbeddingBag(B, DB, mode="mean")
            self.bucket_proj = nn.Linear(DB, DM)
            self.vocab_emb = nn.Embedding(CLASSES, DM)
            self.flag_emb = nn.Embedding(6, DM)
            self.pos_emb = nn.Embedding(64, DM)
            layer = nn.TransformerEncoderLayer(DM, cfg["heads"], cfg["ff"], 0.1, batch_first=True, norm_first=True, activation="gelu")
            self.encoder = nn.TransformerEncoder(layer, cfg["layers"], enable_nested_tensor=False)
            self.norm = nn.LayerNorm(DM)
            self.fix_head = nn.Linear(DM, CLASSES)

        def forward(self, x):
            n, width = x["vid"].shape
            h = self.buckets(x["flat"], x["offsets"]).view(n, width, DB)
            h = self.bucket_proj(h) + self.vocab_emb(x["vid"]) + self.flag_emb(x["flag"])
            h = h + self.pos_emb(torch.arange(width))[None]
            h = self.encoder(h, src_key_padding_mask=x["pad"])
            return self.fix_head(self.norm(h))

    model = Student()
    model.load_state_dict(torch.load(os.path.join(STUDENT_DIR, "student.pt"), map_location="cpu"))
    return model.eval(), cfg


class StudentRunner:
    def __init__(self, model, cfg):
        self.model, self.cfg, self.cache = model.eval(), cfg, {}

    def buckets_of(self, token):
        got = self.cache.get(token)
        if got is None:
            s = "^" + token.lower() + "$"
            grams = [s] + [s[i:i + n] for n in (1, 2, 3) for i in range(len(s) - n + 1)]
            got = [zlib.crc32(g.encode("utf-8")) % self.cfg["buckets"] for g in grams[:self.cfg["ngram_cap"]]]
            self.cache[token] = got
        return got

    @staticmethod
    def flag(token):
        if token.startswith("<<"):
            return 5
        if not any(c.isalpha() for c in token):
            return 4
        if token.isupper() and len(token) > 1:
            return 2
        return 1 if token[:1].isupper() else 0

    @torch.no_grad()
    def __call__(self, examples):
        width = max(len(e.tokens) for e in examples)
        flat, offsets, vid, flag, pad = [], [], [], [], []
        for e in examples:
            for i in range(width):
                offsets.append(len(flat))
                if i < len(e.tokens):
                    flat += self.buckets_of(e.tokens[i])
                    vid.append(vocab_ids.get(e.tokens[i].lower(), 0))
                    flag.append(self.flag(e.tokens[i]))
                    pad.append(False)
                else:
                    flat.append(0)
                    vid.append(0)
                    flag.append(0)
                    pad.append(True)
        n = len(examples)
        x = {"flat": torch.tensor(flat), "offsets": torch.tensor(offsets), "vid": torch.tensor(vid).view(n, width),
             "flag": torch.tensor(flag).view(n, width), "pad": torch.tensor(pad).view(n, width)}
        logits = self.model(x)
        out = []
        for b, e in enumerate(examples):
            p = torch.softmax(logits[b, e.supervised[0]].float(), dim=-1)
            out.append((1.0 - p[d.KEEP].item(), int(p[1:].argmax().item()) + 1))
        return out


def quantise(model):
    return torch.quantization.quantize_dynamic(model, {nn.Linear}, dtype=torch.qint8)


def megabytes(model):
    buf = io.BytesIO()
    torch.save(model.state_dict(), buf)
    return buf.tell() / 1e6


# ----------------------------------------------------------------------------- the sample
def sample_windows(n_errors, n_clean, seed=3):
    """(Example, is_error) pairs, one word judged with one word to its right, from Viwiki-Spelling."""
    docs = d.read_jsonl(os.path.join(DATA, "viwiki_test.jsonl"))
    sentences = [s for doc in docs for s in d.viwiki_sentences(doc, vocab_ids)]
    rng = random.Random(seed)
    rng.shuffle(sentences)
    errors, clean = [], []
    for tokens, fix, kind, error in sentences:
        for pair in d.eval_examples(tokens, fix, kind, error, 1):
            (errors if pair[1] else clean).append(pair)
        if len(errors) >= n_errors and len(clean) >= 40 * n_clean:
            break
    return rng.sample(errors, min(n_errors, len(errors))) + rng.sample(clean, min(n_clean, len(clean)))


def latency(run, windows, threads):
    torch.set_num_threads(threads)
    for pair in windows[:5]:                                   # warm-up
        run([pair[0]])
    times, lengths = [], []
    for pair in windows:
        start = time.perf_counter()
        run([pair[0]])
        times.append((time.perf_counter() - start) * 1000)
        lengths.append(len(pair[0].tokens))
    times.sort()
    return {"threads": threads, "mean_ms": round(statistics.mean(times), 1), "p50_ms": round(times[len(times) // 2], 1),
            "p95_ms": round(times[int(len(times) * 0.95)], 1), "max_ms": round(times[-1], 1),
            "mean_words": round(statistics.mean(lengths), 1)}


def quality(run, pairs, batch=16):
    rows = []
    for i in range(0, len(pairs), batch):
        chunk = pairs[i:i + batch]
        for (p, fix), (e, is_error) in zip(run([c[0] for c in chunk]), chunk):
            rows.append((p, fix, e.fix[e.supervised[0]], is_error))
    return rows


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--latency-windows", type=int, default=150)
    ap.add_argument("--quality-errors", type=int, default=400)
    ap.add_argument("--quality-clean", type=int, default=1600)
    ap.add_argument("--only", default="", help="run only the models whose name starts with this ('student', 'teacher')")
    args = ap.parse_args()
    cores = torch.get_num_threads()
    print(f"CPU threads available to torch: {cores}", flush=True)

    t = Teacher()
    t.load_state_dict(torch.load(TEACHER_PT, map_location="cpu"))
    t.eval()
    s, scfg = load_student()
    models = {
        "teacher fp32": TeacherRunner(t),
        "teacher int8": TeacherRunner(quantise(t)),
        "student fp32": StudentRunner(s, scfg),
        "student int8": StudentRunner(quantise(s), scfg),
    }
    models = {k: v for k, v in models.items() if k.startswith(args.only)}
    sizes = {"teacher fp32": megabytes(t), "teacher int8": megabytes(models["teacher int8"].model) if "teacher int8" in models else 0,
             "student fp32": megabytes(s), "student int8": megabytes(models["student int8"].model) if "student int8" in models else 0}
    pairs = sample_windows(args.quality_errors, args.quality_clean)
    rng = random.Random(1)
    lat_windows = rng.sample(pairs, min(args.latency_windows, len(pairs)))
    print(f"{len(pairs)} windows for quality ({sum(1 for p in pairs if p[1])} mistakes), {len(lat_windows)} for latency", flush=True)

    report = {"threads_available": cores, "sizes_mb": {k: round(v, 1) for k, v in sizes.items()}, "latency": {}, "quality": {}}
    reference = {}
    for name, run in models.items():
        if name == "student int8":              # quantised Linear layers break PyTorch's fused Transformer fast path
            torch.backends.mha.set_fastpath_enabled(False)
        print(f"\n== {name} ({sizes[name]:.0f} MB)", flush=True)
        report["latency"][name] = []
        for threads in sorted({1, min(4, cores), cores}):
            r = latency(run, lat_windows, threads)
            report["latency"][name].append(r)
            print(f"  {threads:>2} thread(s): mean {r['mean_ms']} ms, p50 {r['p50_ms']}, p95 {r['p95_ms']}, max {r['max_ms']} (windows of {r['mean_words']} words)", flush=True)
        torch.set_num_threads(cores)
        start = time.perf_counter()
        rows = quality(run, pairs)
        took = time.perf_counter() - start
        cv = {c["threshold"]: c for c in d.curve(rows, (0.9, 0.99))}
        q = {"seconds_for_all_windows": round(took, 1)}
        for tau, c in cv.items():
            q[f"tau_{tau}"] = {"right_recall": round(c["right_recall"], 4), "right_precision": round(c["right_precision"], 4),
                               "false_per_1000": round(c["false_per_1000"], 3), "changed": c["changed"]}
        if name.endswith("fp32"):
            reference[name.split()[0]] = rows
        else:
            ref = reference[name.split()[0]]
            q["mean_abs_p_change_gap_to_fp32"] = round(statistics.mean(abs(a[0] - b[0]) for a, b in zip(rows, ref)), 4)
            for tau in (0.9, 0.99):
                q[f"same_decision_as_fp32_at_{tau}"] = round(sum((a[0] >= tau) == (b[0] >= tau) for a, b in zip(rows, ref)) / len(rows), 4)
        report["quality"][name] = q
        print("  quality:", json.dumps(q), flush=True)
    out = os.path.join(MODELS, f"bench_cpu{'_' + args.only if args.only else ''}.json")
    json.dump(report, open(out, "w"), indent=1)
    print("\nwritten to", out)


if __name__ == "__main__":
    main()
