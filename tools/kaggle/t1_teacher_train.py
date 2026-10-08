# %% [markdown]
# # T1: the teacher (RESEARCH.md, section 5)
#
# A pretrained multilingual encoder, fine-tuned to read a Vietnamese phrase as it stands on screen and say,
# for each syllable: keep it, or replace it with this syllable of the vocabulary, plus what kind of mistake it is.
#
# * Training windows end where the typist is: the last four positions of a window have 3, 2, 1 and 0 words to
#   their right, as when a word is typed. Whole sentences are mixed in (what a delayed pass sees).
# * Mistakes are made on the fly from clean sentences (proportions of VSEC) and a share of the windows is real
#   VSEC mistakes. Viwiki-Spelling is only for testing.
# * Everything about data and measuring is in `t1_data.py` (tested locally, no torch).
#
# Input: the private dataset `autocorrect-train` (tools/kaggle/prepare_corpus.py). Needs a GPU (T4 is enough).
# Run each `# %%` cell in order. NOT RUN YET: written without a GPU, so expect to fix small things on the first run.

# %% Settings
import glob
import json
import math
import os
import random
import sys
import time

_found = glob.glob("/kaggle/input/**/corpus.txt", recursive=True)
INPUT_DIR = os.path.dirname(_found[0]) if _found else "/kaggle/input/autocorrect-train"
OUT_DIR = "/kaggle/working/t1"
# A multilingual encoder that reads raw text, so a misspelt syllable is just odd pieces. Check that it is available
# (Kaggle needs Internet on to download it) and try others: "vinai/phobert-base-v2" wants word-segmented input.
MODEL_NAME = "FacebookAI/xlm-roberta-base"
TOKENIZER_KWARGS = {}                 # e.g. {"add_prefix_space": True} for a byte-level BPE tokenizer
BATCH = 64
STEPS = 40_000                        # optimizer steps in all; the run stops early at MAX_MINUTES
MAX_MINUTES = 8 * 60                  # leave room in a 9 to 12 hour session
LR, HEAD_LR, WARMUP = 3e-5, 1e-3, 1_000
KIND_WEIGHT = 0.3                     # weight of the kind-of-mistake loss
REAL_FRACTION = 0.15                  # windows taken from real VSEC mistakes
LLM_FRACTION = 0.10                   # windows taken from llm_negatives.jsonl (t2), if the file is there
EVAL_EVERY = 2_000
DEV_WINDOWS = 6_000                   # windows per right-context size in the dev check
VIWIKI_DOCS = 107                     # documents of the final test (all of them)
EXPORT_SENTENCES = 200_000            # sentences of soft labels for the student (0 = skip)
FP16, MAX_LEN, SEED = True, 128, 7
os.makedirs(OUT_DIR, exist_ok=True)
sys.path.insert(0, INPUT_DIR)

# %% Imports and data
import torch
import torch.nn as nn
from torch.utils.data import DataLoader, IterableDataset
from transformers import AutoModel, AutoTokenizer, get_linear_schedule_with_warmup

import t1_data as d

device = "cuda" if torch.cuda.is_available() else "cpu"
print("device:", device, torch.cuda.get_device_name(0) if device == "cuda" else "(no GPU: this will be very slow)")

vocab = [w.strip() for w in open(f"{INPUT_DIR}/vocab.txt", encoding="utf-8") if w.strip()]
vocab_ids = {w: i + 1 for i, w in enumerate(vocab)}
CLASSES, KINDS = len(vocab) + 1, len(d.KINDS)

corpus = [d.split_tokens(l) for l in open(f"{INPUT_DIR}/corpus.txt", encoding="utf-8").read().split("\n") if l.strip()]
vsec_train = [d.vsec_sentence(r, vocab_ids) for r in d.read_jsonl(f"{INPUT_DIR}/vsec_train.jsonl")]
vsec_dev = [d.vsec_sentence(r, vocab_ids) for r in d.read_jsonl(f"{INPUT_DIR}/vsec_dev.jsonl")]
viwiki = [s for doc in d.read_jsonl(f"{INPUT_DIR}/viwiki_test.jsonl")[:VIWIKI_DOCS] for s in d.viwiki_sentences(doc, vocab_ids)]
LLM_PATH = f"{INPUT_DIR}/llm_negatives.jsonl"
llm_negatives = d.read_jsonl(LLM_PATH) if os.path.exists(LLM_PATH) else []
if not llm_negatives:
    LLM_FRACTION = 0.0
print(f"{len(llm_negatives)} LLM-picked mistakes" + ("" if llm_negatives else " (none: this run is without them)"))
print(f"{len(vocab)} syllables, {len(corpus)} clean sentences, VSEC {len(vsec_train)} train / {len(vsec_dev)} dev, Viwiki {len(viwiki)} sentences")

tok = AutoTokenizer.from_pretrained(MODEL_NAME, **TOKENIZER_KWARGS)

# %% Training stream: fresh mistakes every time a sentence is read
class Stream(IterableDataset):
    def __iter__(self):
        info = torch.utils.data.get_worker_info()
        seed = SEED * 1000 + (info.id if info else 0)
        rng, corruptor = random.Random(seed), d.Corruptor(vocab, seed=seed)
        while True:
            r = rng.random()
            if r < REAL_FRACTION:
                tokens, fix, kind, _ = rng.choice(vsec_train)
                n = len(tokens)
                whole = n <= 4 or rng.random() < 0.4
                yield d.make_window(tokens, fix, kind, n if n <= 3 else rng.randint(3, n), whole, rng)
            elif r < REAL_FRACTION + LLM_FRACTION:
                yield d.llm_negative_example(rng.choice(llm_negatives), vocab_ids, rng)
            else:
                yield d.synthetic_example(rng.choice(corpus), corruptor, vocab_ids, rng)


def collate(examples):
    """Subword batch plus, per sentence, where the first piece of every supervised word sits."""
    enc = tok([e.tokens for e in examples], is_split_into_words=True, truncation=True, max_length=MAX_LEN,
              padding=True, return_tensors="pt")
    B, L = enc["input_ids"].shape
    fix = torch.full((B, L), d.IGNORE, dtype=torch.long)
    kind = torch.full((B, L), d.IGNORE, dtype=torch.long)
    target = torch.zeros((B, L), dtype=torch.bool)
    for b, e in enumerate(examples):
        supervised, previous = set(e.supervised), None
        for pos, word in enumerate(enc.word_ids(b)):
            if word is not None and word != previous and word in supervised:
                fix[b, pos], kind[b, pos], target[b, pos] = e.fix[word], e.kind[word], True
            previous = word
    return enc["input_ids"], enc["attention_mask"], fix, kind, target


# %% Model: the encoder and two linear heads
class Teacher(nn.Module):
    def __init__(self):
        super().__init__()
        self.encoder = AutoModel.from_pretrained(MODEL_NAME)
        hidden = self.encoder.config.hidden_size
        self.fix_head = nn.Linear(hidden, CLASSES)     # class 0 keeps the syllable
        self.kind_head = nn.Linear(hidden, KINDS)

    def forward(self, ids, mask):
        h = self.encoder(input_ids=ids, attention_mask=mask).last_hidden_state
        return self.fix_head(h), self.kind_head(h)


model = Teacher().to(device)
ce = nn.CrossEntropyLoss(ignore_index=d.IGNORE)
print(f"{sum(p.numel() for p in model.parameters()) / 1e6:.0f}M parameters")

# %% Measuring: the same curve as `ac-sim` and `ac-bench --viwiki`
@torch.no_grad()
def predict(pairs, batch=256):
    """(p_change, predicted fix, gold fix, is_error) for (Example, is_error) pairs with one supervised word each."""
    model.eval()
    rows = []
    for i in range(0, len(pairs), batch):
        chunk = pairs[i:i + batch]
        ids, mask, fix, kind, target = collate([e for e, _ in chunk])
        with torch.autocast(device_type=device, dtype=torch.float16, enabled=FP16 and device == "cuda"):
            logits, _ = model(ids.to(device), mask.to(device))
        probs = torch.softmax(logits.float(), dim=-1).cpu()
        for b, (ex, is_error) in enumerate(chunk):
            where = target[b].nonzero()
            if len(where) == 0:          # the word fell outside the truncated window
                continue
            p = probs[b, where[0, 0]]
            gold = ex.fix[ex.supervised[0]]
            rows.append((1.0 - p[d.KEEP].item(), int(p[1:].argmax().item()) + 1, gold, is_error))
    model.train()
    return rows


def pairs_of(sentences, right, limit=None, seed=0):
    out = []
    for tokens, fix, kind, error in sentences:
        out += d.eval_examples(tokens, fix, kind, error, right)
    if limit and len(out) > limit:
        # keep every mistake, sample the rest, so the curve has something to say
        rng = random.Random(seed)
        errors = [p for p in out if p[1]]
        rest = rng.sample([p for p in out if not p[1]], max(0, limit - len(errors)))
        out = errors + rest
    return out


def right_f1(rows, tau=0.9):
    c = {x["threshold"]: x for x in d.curve(rows)}[tau]
    p, r = c["right_precision"], c["right_recall"]
    return 2 * p * r / (p + r) if p + r else 0.0


# %% Train
groups = [
    {"params": list(model.encoder.parameters()), "lr": LR},
    {"params": list(model.fix_head.parameters()) + list(model.kind_head.parameters()), "lr": HEAD_LR},
]
opt = torch.optim.AdamW(groups, weight_decay=0.01)
sched = get_linear_schedule_with_warmup(opt, WARMUP, STEPS)
scaler = torch.amp.GradScaler(enabled=FP16 and device == "cuda")
loader = DataLoader(Stream(), batch_size=BATCH, collate_fn=collate, num_workers=3, prefetch_factor=4)

best, started, log = -1.0, time.time(), []
model.train()
for step, (ids, mask, fix, kind, _) in enumerate(loader, 1):
    ids, mask, fix, kind = ids.to(device), mask.to(device), fix.to(device), kind.to(device)
    with torch.autocast(device_type=device, dtype=torch.float16, enabled=FP16 and device == "cuda"):
        fix_logits, kind_logits = model(ids, mask)
        loss_fix = ce(fix_logits.float().view(-1, CLASSES), fix.view(-1))
        loss_kind = ce(kind_logits.float().view(-1, KINDS), kind.view(-1))
        loss = loss_fix + KIND_WEIGHT * loss_kind
    opt.zero_grad(set_to_none=True)
    scaler.scale(loss).backward()
    scaler.unscale_(opt)
    nn.utils.clip_grad_norm_(model.parameters(), 1.0)
    scaler.step(opt)
    scaler.update()
    sched.step()
    if step % 100 == 0:
        print(f"step {step:6d}  loss {loss.item():.4f}  (fix {loss_fix.item():.4f}, kind {loss_kind.item():.4f})  {(time.time() - started) / 60:.1f} min", flush=True)
    out_of_time = (time.time() - started) / 60 > MAX_MINUTES
    if step % EVAL_EVERY == 0 or step == STEPS or out_of_time:
        scores = {}
        for right in (0, 1):
            rows = predict(pairs_of(vsec_dev, right, DEV_WINDOWS))
            scores[right] = right_f1(rows)
            print(d.format_curve(f"VSEC dev, {right} word(s) to the right", rows), flush=True)
        score = (scores[0] + scores[1]) / 2
        log.append({"step": step, "dev_right_f1_at_0.9": scores})
        if score > best:
            best = score
            torch.save(model.state_dict(), f"{OUT_DIR}/teacher.pt")
            print(f"saved (dev right-F1 at tau 0.9: {score:.4f})", flush=True)
        json.dump(log, open(f"{OUT_DIR}/train_log.json", "w"), indent=1)
    if step >= STEPS or out_of_time:
        break

# %% Final test on Viwiki-Spelling (real mistakes in text written by others; never seen in training)
model.load_state_dict(torch.load(f"{OUT_DIR}/teacher.pt", map_location=device))
report = {}
for right in (0, 1):
    rows = predict(pairs_of(viwiki, right))
    print(d.format_curve(f"Viwiki, {right} word(s) to the right", rows), flush=True)
    report[f"viwiki_right_{right}"] = d.curve(rows)
json.dump(report, open(f"{OUT_DIR}/viwiki_curves.json", "w"), indent=1)
print("Compare with `cargo run -p ac-bench --release -- --viwiki` (RESEARCH.md, 3.3) at the same false/1000.")

# %% Soft labels for the student: the teacher's probabilities on fresh windows
# Written in the contract the student must match: tokens, the positions that count, the top fix probabilities,
# and the kind probabilities. The student is trained on these, not on hard labels.
if EXPORT_SENTENCES:
    model.eval()
    rng, corruptor = random.Random(SEED + 1), d.Corruptor(vocab, seed=SEED + 1)
    TOP, examples = 8, []
    sentences = corpus[:EXPORT_SENTENCES]
    with open(f"{OUT_DIR}/soft_labels.jsonl", "w", encoding="utf-8") as out, torch.no_grad():
        for n, sentence in enumerate(sentences, 1):
            examples.append(d.synthetic_example(sentence, corruptor, vocab_ids, rng))
            if len(examples) < 256 and n < len(sentences):
                continue
            ids, mask, fix, kind, target = collate(examples)
            with torch.autocast(device_type=device, dtype=torch.float16, enabled=FP16 and device == "cuda"):
                fix_logits, kind_logits = model(ids.to(device), mask.to(device))
            fp = torch.softmax(fix_logits.float(), -1).cpu()
            kp = torch.softmax(kind_logits.float(), -1).cpu()
            for b, ex in enumerate(examples):
                positions = target[b].nonzero().flatten().tolist()
                if not positions:
                    continue
                top = fp[b, positions].topk(TOP, dim=-1)
                out.write(json.dumps({
                    "tokens": ex.tokens,
                    "positions": ex.supervised[: len(positions)],
                    "fix_top_ids": top.indices.tolist(),
                    "fix_top_probs": [[round(x, 5) for x in r] for r in top.values.tolist()],
                    "kind_probs": [[round(x, 5) for x in r] for r in kp[b, positions].tolist()],
                    "hard_fix": [ex.fix[p] for p in ex.supervised[: len(positions)]],
                }, ensure_ascii=False) + "\n")
            examples = []
    print("soft labels written to", f"{OUT_DIR}/soft_labels.jsonl")
