# %% [markdown]
# # M1: LLM teacher scores the corrector's candidates
#
# Input: the JSONL written by `ac-bench --export` (public corpora only, no user data).
# For every sample and every candidate this computes two log-likelihoods with a causal LLM:
#
# * `ll_left`: the text up to and including the candidate (what the app can see when it decides)
# * `ll_full`: the same plus the words that follow (what only a delayed correction could see)
#
# Output: `teacher_scores.jsonl`, one line per sample with `ll_left` and `ll_full` lists in the
# same order as the sample's candidates. Read it locally with `tools/kaggle/analyze_m1.py`.
#
# Run each `# %%` block as a notebook cell. Needs a GPU (T4 / P100 is enough for 1-3B models).

# %% Settings
import glob
# Kaggle mounts datasets under a path that has changed over time, so search for the file.
_found = glob.glob("/kaggle/input/**/export.jsonl", recursive=True)
EXPORT_PATH = _found[0] if _found else "/kaggle/input/autocorrect-export/export.jsonl"
OUT_PATH = "/kaggle/working/teacher_scores.jsonl"
MODEL_NAME = "Qwen/Qwen2.5-1.5B"    # a base (not chat) model; verify it is available and handles Vietnamese
LOAD_4BIT = False                    # True for 7B models on a 16 GB GPU (needs bitsandbytes)
MAX_PER_GROUP = 3000                 # samples per (set, class); None = all
BATCH = 32                           # sequences per forward pass; lowered automatically on out-of-memory
SEED = 7

# %% Imports
import json, math, random, time
from collections import defaultdict

import torch
from transformers import AutoModelForCausalLM, AutoTokenizer

# %% Load samples, stratified by (set, class)
random.seed(SEED)
groups = defaultdict(list)
with open(EXPORT_PATH, encoding="utf-8") as f:
    for line in f:
        sample = json.loads(line)
        groups[(sample["set"], sample["class"])].append(sample)
samples = []
for key, rows in sorted(groups.items()):
    if MAX_PER_GROUP and len(rows) > MAX_PER_GROUP:
        rows = random.sample(rows, MAX_PER_GROUP)
    samples.extend(rows)
    print(f"{key[0]:3} {key[1]:10} {len(rows):6} samples")
print("total", len(samples))

# %% Texts to score (deduplicated: many samples share a context and candidate)
def left_text(sample, candidate):
    return " ".join(sample["context"] + [candidate["t"]])

def full_text(sample, candidate):
    return " ".join(sample["context"] + [candidate["t"]] + sample["right"])

texts = {}
for sample in samples:
    for candidate in sample["candidates"]:
        texts[left_text(sample, candidate)] = None
        texts[full_text(sample, candidate)] = None
print(len(texts), "distinct texts to score")

# %% Model
tok = AutoTokenizer.from_pretrained(MODEL_NAME)
kwargs = {"device_map": "auto"}
if LOAD_4BIT:
    from transformers import BitsAndBytesConfig
    kwargs["quantization_config"] = BitsAndBytesConfig(load_in_4bit=True, bnb_4bit_compute_dtype=torch.float16)
else:
    kwargs["torch_dtype"] = torch.float16
model = AutoModelForCausalLM.from_pretrained(MODEL_NAME, **kwargs).eval()
start_id = tok.bos_token_id if tok.bos_token_id is not None else tok.eos_token_id
pad_id = tok.pad_token_id if tok.pad_token_id is not None else start_id
device = next(model.parameters()).device

# %% Scoring: log P(text | start token), summed over the text's tokens
@torch.no_grad()
def score_batch(batch):
    ids = [[start_id] + tok(t, add_special_tokens=False).input_ids for t in batch]
    width = max(len(i) for i in ids)
    input_ids = torch.tensor([i + [pad_id] * (width - len(i)) for i in ids], device=device)
    mask = torch.tensor([[1] * len(i) + [0] * (width - len(i)) for i in ids], device=device)
    logits = model(input_ids=input_ids, attention_mask=mask).logits[:, :-1].float()
    logp = torch.log_softmax(logits, dim=-1).gather(-1, input_ids[:, 1:, None]).squeeze(-1)
    return (logp * mask[:, 1:]).sum(dim=1).tolist()

order = sorted(texts, key=len)
scores, i, batch_size, started, last_report = {}, 0, BATCH, time.time(), 0
while i < len(order):
    batch = order[i : i + batch_size]
    try:
        for text, value in zip(batch, score_batch(batch)):
            scores[text] = value
        i += len(batch)
    except torch.cuda.OutOfMemoryError:
        torch.cuda.empty_cache()
        batch_size = max(1, batch_size // 2)
        print("out of memory, batch size now", batch_size)
        continue
    if i - last_report >= 5000 or i == len(order):
        last_report = i
        rate = i / (time.time() - started)
        print(f"{i}/{len(order)} texts, {rate:.0f}/s, about {(len(order) - i) / rate / 60:.1f} min left")
print("done in", round(time.time() - started), "s")

# %% Write the scores next to each sample's candidates
with open(OUT_PATH, "w", encoding="utf-8") as out:
    for sample in samples:
        row = {
            "id": sample["id"],
            "model": MODEL_NAME,
            "ll_left": [scores[left_text(sample, c)] for c in sample["candidates"]],
            "ll_full": [scores[full_text(sample, c)] for c in sample["candidates"]],
        }
        out.write(json.dumps(row, ensure_ascii=False) + "\n")
print("wrote", OUT_PATH, "-> download it and run tools/kaggle/analyze_m1.py locally")
