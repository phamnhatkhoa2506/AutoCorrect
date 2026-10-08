# %% [markdown]
# # The student (RESEARCH.md, section 5, tier 2)
#
# A small network that learns the teacher's judgement from its soft labels (`soft_labels.jsonl`, written by
# `t1_teacher_train.py`) and is small enough to run in the app: words are read by hashed letter n-grams (so a
# misspelt word is still a known shape), a vocabulary id for exact words, and a small bidirectional Transformer
# over the window. It is meant for the delayed pass: the word to judge has at least one word to its right.
#
# Loss: cross-entropy against the teacher's top-8 fix probabilities (renormalised), plus a little on the hard labels.
# Test: Viwiki-Spelling, never trained on, at 0 and 1 words to the right, on the same curve as the teacher.
#
# Kaggle: attach the private dataset `autocorrect-train` and the output of the kernel `autocorrect-t1-teacher`
# (kernel_sources), GPU on. Locally, `STUDENT_SMOKE=1` runs a tiny CPU check (see tools/student/README.md).

# %% Settings
import glob
import json
import math
import os
import random
import sys
import time
import zlib

SMOKE = bool(os.environ.get("STUDENT_SMOKE"))
BUCKETS, NGRAM_CAP = 65_536, 40       # hashed letter n-grams per word
D_BUCKET, D_MODEL, HEADS, LAYERS, FF = 128, 256, 4, 3, 768
DROPOUT = 0.1
BATCH, EPOCHS, LR, WARMUP = 256, 6, 1e-3, 500
HARD_WEIGHT = 0.3                      # weight of the hard-label loss next to the soft one
HELD_OUT = 2_000                       # soft-label examples kept to measure agreement with the teacher
DEV_WINDOWS = 6_000
TAUS = (0.5, 0.7, 0.8, 0.9, 0.95, 0.98, 0.99, 0.995, 0.998, 0.999, 0.9995, 0.9999)
SEED = 7
# False: no teacher. The same network learns from hard labels on windows with fresh mistakes (the teacher's
# own training stream: synthetic mistakes on clean sentences, real VSEC windows, LLM-picked mistakes).
DISTILL = os.environ.get("STUDENT_DISTILL", "1") == "1"     # the no-teacher Kaggle copy changes this default to "0"
# True: the same fresh stream, with the teacher in the loop: it grades every batch while the student learns
# (needs the teacher's weights, kernel_sources autocorrect-t1-teacher; same data and steps as the hard run).
ONLINE = os.environ.get("STUDENT_ONLINE", "0") == "1"
if ONLINE:
    DISTILL = False
REAL_FRACTION, LLM_FRACTION = 0.15, 0.10
if not DISTILL and not ONLINE:
    HARD_WEIGHT = 0.0                  # the "soft" loss is already the hard-label loss
if SMOKE:
    EPOCHS, BATCH, HELD_OUT, DEV_WINDOWS = 1, 32, 100, 200

HERE = os.path.dirname(os.path.abspath(__file__)) if "__file__" in globals() else "."
if SMOKE:
    ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
    DATA_DIR = os.path.join(ROOT, "tools", "kaggle", "upload", "autocorrect-train")
    SOFT_PATH = os.environ.get("STUDENT_SOFT", "")
    SOFT_LIMIT = int(os.environ.get("STUDENT_SOFT_LIMIT", "3000"))
    OUT_DIR = os.path.join(ROOT, "tools", "student", "work")
    sys.path.insert(0, os.path.join(ROOT, "tools", "kaggle"))
else:
    _found = glob.glob("/kaggle/input/**/corpus.txt", recursive=True)
    DATA_DIR = os.path.dirname(_found[0]) if _found else "/kaggle/input/autocorrect-train"
    _soft = glob.glob("/kaggle/input/**/soft_labels.jsonl", recursive=True)
    assert _soft or not DISTILL, "attach the output of the kernel autocorrect-t1-teacher (kernel_sources)"
    SOFT_PATH, SOFT_LIMIT = (_soft[0] if _soft else ""), 0
    OUT_DIR = "/kaggle/working/student"
    sys.path.insert(0, DATA_DIR)
os.makedirs(OUT_DIR, exist_ok=True)

# %% Imports and data
import torch
import torch.nn as nn
import torch.nn.functional as F
from torch.utils.data import DataLoader

import t1_data as d

device = "cuda" if torch.cuda.is_available() else "cpu"
random.seed(SEED)
torch.manual_seed(SEED)
print("device:", device, torch.cuda.get_device_name(0) if device == "cuda" else "")

vocab = [w.strip() for w in open(f"{DATA_DIR}/vocab.txt", encoding="utf-8") if w.strip()]
vocab_ids = {w: i + 1 for i, w in enumerate(vocab)}
CLASSES = len(vocab) + 1                      # class 0 keeps the word, class i is vocab[i - 1]
vsec_dev = [d.vsec_sentence(r, vocab_ids) for r in d.read_jsonl(f"{DATA_DIR}/vsec_dev.jsonl")]
viwiki = [s for doc in d.read_jsonl(f"{DATA_DIR}/viwiki_test.jsonl") for s in d.viwiki_sentences(doc, vocab_ids)]

soft = []
if DISTILL:
    with open(SOFT_PATH, encoding="utf-8") as f:
        for line in f:
            j = json.loads(line)
            soft.append((j["tokens"], j["positions"], j["fix_top_ids"], j["fix_top_probs"], j["hard_fix"]))
            if SOFT_LIMIT and len(soft) >= SOFT_LIMIT:
                break
    random.shuffle(soft)
    held, train = soft[:HELD_OUT], soft[HELD_OUT:]
    print(f"{len(train)} soft-label examples to train on, {len(held)} held out; {len(vocab)} syllables")
else:
    corpus = [d.split_tokens(l) for l in open(f"{DATA_DIR}/corpus.txt", encoding="utf-8").read().split("\n") if l.strip()]
    vsec_train = [d.vsec_sentence(r, vocab_ids) for r in d.read_jsonl(f"{DATA_DIR}/vsec_train.jsonl")]
    _llm = f"{DATA_DIR}/llm_negatives.jsonl"
    llm_negatives = d.read_jsonl(_llm) if os.path.exists(_llm) else []
    if SMOKE:
        corpus = corpus[:2000]
    # as many steps as the distilled run would take, so the two runs compare at equal cost
    train = [None] * (320 if SMOKE else 198_000)
    held = []
    print(f"hard labels only: {len(corpus)} clean sentences, {len(vsec_train)} VSEC, {len(llm_negatives)} LLM-picked; {len(vocab)} syllables")


class HardStream(torch.utils.data.IterableDataset):
    """Windows with fresh mistakes, in the shape of a soft-label example with the hard label as the only target."""

    def __iter__(self):
        info = torch.utils.data.get_worker_info()
        seed = SEED * 1000 + (info.id if info else 0)
        rng, corruptor = random.Random(seed), d.Corruptor(vocab, seed=seed)
        while True:
            r = rng.random()
            if r < REAL_FRACTION:
                tokens, fix, kind, _ = rng.choice(vsec_train)
                n = len(tokens)
                ex = d.make_window(tokens, fix, kind, n if n <= 3 else rng.randint(3, n), n <= 4 or rng.random() < 0.4, rng)
            elif r < REAL_FRACTION + LLM_FRACTION and llm_negatives:
                ex = d.llm_negative_example(rng.choice(llm_negatives), vocab_ids, rng)
            else:
                ex = d.synthetic_example(rng.choice(corpus), corruptor, vocab_ids, rng)
            keep = [p for p in ex.supervised if ex.fix[p] != d.IGNORE]
            if keep:
                yield (ex.tokens, keep, [[ex.fix[p]] + [0] * 7 for p in keep], [[1.0] + [0.0] * 7 for p in keep],
                       [ex.fix[p] for p in keep])

# %% Word features: hashed letter n-grams, vocabulary id, shape flag
_cache = {}


def ngram_buckets(token):
    """Bucket ids of the letter n-grams (1 to 3, with word boundaries) and of the whole word, lower case.
    crc32 so that the Rust side can compute the very same ids."""
    got = _cache.get(token)
    if got is None:
        s = "^" + token.lower() + "$"
        grams = [s] + [s[i:i + n] for n in (1, 2, 3) for i in range(len(s) - n + 1)]
        got = [zlib.crc32(g.encode("utf-8")) % BUCKETS for g in grams[:NGRAM_CAP]]
        _cache[token] = got
    return got


def shape_flag(token):
    if token.startswith("<<"):
        return 5                              # the environment token
    if not any(c.isalpha() for c in token):
        return 4                              # digits, punctuation
    if token.isupper() and len(token) > 1:
        return 2
    if token[:1].isupper():
        return 1
    return 0


def collate_windows(items):
    """items: (tokens, positions) per window -> model inputs; pad word has bucket 0 and the mask set."""
    width = max(len(t) for t, _ in items)
    flat, offsets, vid, flag, pad = [], [], [], [], []
    for tokens, _ in items:
        for i in range(width):
            offsets.append(len(flat))
            if i < len(tokens):
                flat += ngram_buckets(tokens[i])
                vid.append(vocab_ids.get(tokens[i].lower(), 0))
                flag.append(shape_flag(tokens[i]))
                pad.append(False)
            else:
                flat.append(0)
                vid.append(0)
                flag.append(0)
                pad.append(True)
    n = len(items)
    return {
        "flat": torch.tensor(flat), "offsets": torch.tensor(offsets),
        "vid": torch.tensor(vid).view(n, width), "flag": torch.tensor(flag).view(n, width),
        "pad": torch.tensor(pad).view(n, width),
    }


def collate_train(batch):
    out = collate_windows([(t, p) for t, p, _, _, _ in batch])
    pb, pi, ids, probs, hard = [], [], [], [], []
    for b, (_, positions, top_ids, top_probs, hard_fix) in enumerate(batch):
        for k, p in enumerate(positions):
            pb.append(b)
            pi.append(p)
            ids.append(top_ids[k])
            probs.append(top_probs[k])
            hard.append(hard_fix[k])
    out.update(pb=torch.tensor(pb), pi=torch.tensor(pi), ids=torch.tensor(ids),
               probs=torch.tensor(probs, dtype=torch.float32), hard=torch.tensor(hard))
    return out


# %% Model
class Student(nn.Module):
    def __init__(self):
        super().__init__()
        self.buckets = nn.EmbeddingBag(BUCKETS, D_BUCKET, mode="mean")
        self.bucket_proj = nn.Linear(D_BUCKET, D_MODEL)
        self.vocab_emb = nn.Embedding(CLASSES, D_MODEL)       # id 0: not a vocabulary word
        self.flag_emb = nn.Embedding(6, D_MODEL)
        self.pos_emb = nn.Embedding(64, D_MODEL)
        layer = nn.TransformerEncoderLayer(D_MODEL, HEADS, FF, DROPOUT, batch_first=True, norm_first=True, activation="gelu")
        self.encoder = nn.TransformerEncoder(layer, LAYERS, enable_nested_tensor=False)
        self.norm = nn.LayerNorm(D_MODEL)
        self.fix_head = nn.Linear(D_MODEL, CLASSES)

    def forward(self, x):
        n, width = x["vid"].shape
        h = self.buckets(x["flat"], x["offsets"]).view(n, width, D_BUCKET)
        h = self.bucket_proj(h) + self.vocab_emb(x["vid"]) + self.flag_emb(x["flag"])
        h = h + self.pos_emb(torch.arange(width, device=h.device))[None]
        h = self.encoder(h, src_key_padding_mask=x["pad"])
        return self.fix_head(self.norm(h))


def to_device(x):
    return {k: v.to(device) for k, v in x.items()}


model = Student().to(device)
print(f"{sum(p.numel() for p in model.parameters()) / 1e6:.1f}M parameters")

# %% The teacher in the loop (online mode only): the same network as t1_teacher_train.py, frozen
TEACHER_TOP = 8
if ONLINE:
    from transformers import AutoModel, AutoTokenizer

    class Teacher(nn.Module):
        def __init__(self):
            super().__init__()
            self.encoder = AutoModel.from_pretrained("FacebookAI/xlm-roberta-base")
            hidden = self.encoder.config.hidden_size
            self.fix_head = nn.Linear(hidden, CLASSES)
            self.kind_head = nn.Linear(hidden, len(d.KINDS))

        def forward(self, ids, mask):
            h = self.encoder(input_ids=ids, attention_mask=mask).last_hidden_state
            return self.fix_head(h), self.kind_head(h)

    _tw = os.environ.get("STUDENT_TEACHER") or (glob.glob("/kaggle/input/**/teacher.pt", recursive=True) or [""])[0]
    assert _tw, "attach the output of the kernel autocorrect-t1-teacher (kernel_sources)"
    teacher = Teacher().to(device)
    teacher.load_state_dict(torch.load(_tw, map_location=device))
    teacher.eval()
    teacher_tok = AutoTokenizer.from_pretrained("FacebookAI/xlm-roberta-base")

    @torch.no_grad()
    def grade(raw):
        """raw: (tokens, positions, _, _, hard) per window -> the same with the teacher's top-8 at each position.
        A position outside the teacher's truncated input is dropped."""
        enc = teacher_tok([r[0] for r in raw], is_split_into_words=True, truncation=True, max_length=128,
                          padding=True, return_tensors="pt")
        with torch.autocast(device_type=device, dtype=torch.float16, enabled=device == "cuda"):
            logits, _ = teacher(enc["input_ids"].to(device), enc["attention_mask"].to(device))
        probs = torch.softmax(logits.float(), dim=-1)
        out = []
        for b, (tokens, positions, _, _, hard) in enumerate(raw):
            first, previous = {}, None
            for sub, word in enumerate(enc.word_ids(b)):
                if word is not None and word != previous:
                    first[word] = sub
                previous = word
            keep = [k for k, p in enumerate(positions) if p in first]
            if not keep:
                continue
            top = probs[b, [first[positions[k]] for k in keep]].topk(TEACHER_TOP, dim=-1)
            out.append((tokens, [positions[k] for k in keep], top.indices.cpu().tolist(), top.values.cpu().tolist(),
                        [hard[k] for k in keep]))
        return out


def identity(x):
    return x

# %% Measuring: the teacher's curve, on the student
@torch.no_grad()
def predict(pairs, batch=256):
    """(p_change, predicted fix, gold fix, is_error) for (Example, is_error) pairs with one supervised word each."""
    model.eval()
    rows = []
    for i in range(0, len(pairs), batch):
        chunk = pairs[i:i + batch]
        x = collate_windows([(e.tokens, e.supervised) for e, _ in chunk])
        with torch.autocast(device_type=device, dtype=torch.float16, enabled=device == "cuda"):
            logits = model(to_device(x))
        for b, (ex, is_error) in enumerate(chunk):
            p = torch.softmax(logits[b, ex.supervised[0]].float(), dim=-1).cpu()
            rows.append((1.0 - p[d.KEEP].item(), int(p[1:].argmax().item()) + 1, ex.fix[ex.supervised[0]], is_error))
    model.train()
    return rows


def pairs_of(sentences, right, limit=None, seed=0):
    out = []
    for tokens, fix, kind, error in sentences:
        out += d.eval_examples(tokens, fix, kind, error, right)
    if limit and len(out) > limit:
        rng = random.Random(seed)
        errors = [p for p in out if p[1]]
        rest = rng.sample([p for p in out if not p[1]], max(0, limit - len(errors)))
        out = errors + rest
    return out


def right_f1(rows, tau=0.9):
    c = {x["threshold"]: x for x in d.curve(rows)}[tau]
    p, r = c["right_precision"], c["right_recall"]
    return 2 * p * r / (p + r) if p + r else 0.0


@torch.no_grad()
def agreement(examples):
    """Share of supervised positions where the student's top fix equals the teacher's, and mean |p_keep| gap."""
    model.eval()
    same = total = 0
    gap = 0.0
    for i in range(0, len(examples), 128):
        batch = collate_train(examples[i:i + 128])
        with torch.autocast(device_type=device, dtype=torch.float16, enabled=device == "cuda"):
            logits = model(to_device(batch))
        sel = logits[batch["pb"].to(device), batch["pi"].to(device)].float()
        prob = torch.softmax(sel, -1).cpu()
        top = batch["ids"][:, 0]
        same += (prob.argmax(-1) == top).sum().item()
        keep_teacher = torch.where(batch["ids"] == 0, batch["probs"], torch.zeros_like(batch["probs"])).sum(-1)
        gap += (prob[:, 0] - keep_teacher).abs().sum().item()
        total += len(top)
    model.train()
    return same / max(1, total), gap / max(1, total)


# %% Train
if DISTILL:
    loader = DataLoader(train, batch_size=BATCH, shuffle=True, collate_fn=collate_train, num_workers=0 if SMOKE else 3, drop_last=True)
    steps_per_epoch = len(loader)
else:
    loader = DataLoader(HardStream(), batch_size=BATCH, collate_fn=identity if ONLINE else collate_train,
                        num_workers=0 if SMOKE else 3, prefetch_factor=None if SMOKE else 4)
    steps_per_epoch = len(train) // BATCH
    _stream = iter(loader)
steps_total = EPOCHS * steps_per_epoch
opt = torch.optim.AdamW(model.parameters(), lr=LR, weight_decay=0.01)
sched = torch.optim.lr_scheduler.LambdaLR(opt, lambda s: min(1.0, (s + 1) / WARMUP) * max(0.02, 1 - s / steps_total))
scaler = torch.amp.GradScaler(enabled=device == "cuda")
best, started, log, step = -1.0, time.time(), [], 0
model.train()
import itertools
for epoch in range(1, EPOCHS + 1):
    for batch in (loader if DISTILL else itertools.islice(_stream, steps_per_epoch)):
        step += 1
        if ONLINE:
            batch = collate_train(grade(batch))
        batch = to_device(batch)
        with torch.autocast(device_type=device, dtype=torch.float16, enabled=device == "cuda"):
            logits = model(batch)
        sel = logits[batch["pb"], batch["pi"]].float()
        logq = F.log_softmax(sel, dim=-1)
        target = batch["probs"] / batch["probs"].sum(-1, keepdim=True).clamp_min(1e-6)
        soft_loss = -(target * logq.gather(1, batch["ids"])).sum(-1).mean()
        known = batch["hard"] != d.IGNORE
        hard_loss = F.cross_entropy(sel[known], batch["hard"][known]) if known.any() else sel.sum() * 0
        loss = soft_loss + HARD_WEIGHT * hard_loss
        opt.zero_grad(set_to_none=True)
        scaler.scale(loss).backward()
        scaler.unscale_(opt)
        nn.utils.clip_grad_norm_(model.parameters(), 1.0)
        scaler.step(opt)
        scaler.update()
        sched.step()
        if step % 200 == 0:
            print(f"epoch {epoch} step {step:6d}/{steps_total}  loss {loss.item():.4f} (soft {soft_loss.item():.4f}, hard {hard_loss.item():.4f})  {(time.time() - started) / 60:.1f} min", flush=True)
    agree, gap = agreement(held) if held else (0.0, 0.0)
    scores = {}
    for right in (0, 1):
        rows = predict(pairs_of(vsec_dev, right, DEV_WINDOWS))
        scores[right] = right_f1(rows)
        print(d.format_curve(f"VSEC dev, {right} word(s) to the right (epoch {epoch})", rows), flush=True)
    print(f"epoch {epoch}: top-1 agreement with the teacher {100 * agree:.1f}%, mean |p_keep gap| {gap:.4f}, dev right-F1 at 0.9: {scores}", flush=True)
    log.append({"epoch": epoch, "agreement": agree, "keep_gap": gap, "dev_right_f1_at_0.9": scores})
    score = scores[1]                           # the delayed pass is what the student is for
    if score > best:
        best = score
        torch.save(model.state_dict(), f"{OUT_DIR}/student.pt")
        print(f"saved (dev right-F1 at 0.9, 1 word to the right: {score:.4f})", flush=True)
    json.dump(log, open(f"{OUT_DIR}/train_log.json", "w"), indent=1)

# %% Final test on Viwiki-Spelling
model.load_state_dict(torch.load(f"{OUT_DIR}/student.pt", map_location=device))
report = {}
for right in (0, 1):
    rows = predict(pairs_of(viwiki[:20] if SMOKE else viwiki, right))
    print(f"Viwiki, {right} word(s) to the right: {len(rows)} words, {sum(1 for r in rows if r[3])} mistakes", flush=True)
    print(f"{'tau':>9}{'changed':>9}{'right-P':>9}{'right-R':>9}{'false/1000':>12}", flush=True)
    cv = d.curve(rows, TAUS)
    for c in cv:
        print(f"{c['threshold']:>9}{c['changed']:>9}{100 * c['right_precision']:>8.1f}%{100 * c['right_recall']:>8.1f}%{c['false_per_1000']:>12.2f}", flush=True)
    report[f"viwiki_right_{right}"] = cv
json.dump(report, open(f"{OUT_DIR}/viwiki_student.json", "w"), indent=1)

# %% Held keys: a letter repeated 2 to 4 more times in a word that is correct on the page ("nguuu" for "ngu")
class _Window:
    def __init__(self, tokens, pos):
        self.tokens, self.supervised, self.fix = tokens, [pos], [d.IGNORE] * len(tokens)


def held_key_windows(count=3000, seed=11):
    rng = random.Random(seed)
    pool = [(tokens, j) for tokens, fix, kind, error in viwiki for j in range(1, len(tokens) - 1)
            if not error[j] and tokens[j].lower() in vocab_ids and len(tokens[j]) >= 2 and tokens[j].isalpha()]
    out = []
    for tokens, j in rng.sample(pool, min(count, len(pool))):
        word = tokens[j]
        i = rng.randrange(len(word))
        held = word[:i] + word[i] * rng.choice([2, 3, 4]) + word[i:]
        lo = max(0, j - 4)
        out.append((_Window([d.ENV_TOKEN] + tokens[lo:j] + [held] + [tokens[j + 1]], j - lo + 1), word.lower()))
    return out


held = held_key_windows()
if held:
    probs = predict([(w, False) for w, _ in held])
    for tau in (0.9, 0.99):
        got = [(p, f) for (p, f, _, _) in probs]
        changed = sum(1 for p, _ in got if p >= tau)
        right = sum(1 for (p, f), (_, word) in zip(got, held) if p >= tau and vocab[f - 1] == word)
        print(f"held keys, {len(held)} windows, tau {tau}: changed {100 * changed / len(held):.1f}%, made right {100 * right / len(held):.1f}%", flush=True)
for tokens, pos in ((["con", "vịt", "nguuu", "ngốc"], 3), (["con", "vịt", "nguu", "ngốc"], 3), (["tôi", "đang", "đọcccc", "sách"], 3)):
    row = predict([(_Window([d.ENV_TOKEN] + tokens, pos), False)])[0]
    print(f"{' '.join(tokens)!r}: p_change {row[0]:.3f}, fix {vocab[row[1] - 1]}", flush=True)
json.dump({"buckets": BUCKETS, "ngram_cap": NGRAM_CAP, "d_bucket": D_BUCKET, "d_model": D_MODEL, "heads": HEADS,
           "layers": LAYERS, "ff": FF, "classes": CLASSES, "hash": "crc32 of utf-8 n-gram of '^' + lower(word) + '$'"},
          open(f"{OUT_DIR}/student_config.json", "w"), indent=1)
print("Teacher, 1 word to the right (RESEARCH.md): tau 0.9 -> right-R 39.0% at 1.01 false/1000; tau 0.99 -> 30.6% at 0.22.")
