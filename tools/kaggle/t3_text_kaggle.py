# %% [markdown]
# # T3: clean text of the writer's own domain, written by an open LLM (two T4s)
#
# Runs `t3_domain_text.py` (see its docstring) as two independent processes, one per GPU, then filters the sentences
# with the vocabulary. The LLM writes plain sentences only; it decides nothing about what is a mistake.
#
# Input: the private dataset `autocorrect-train` (tools/kaggle/prepare_corpus.py), GPU = "T4 x2", Internet on.
# Read the probe samples first. If the sentences are too samey or not natural enough, try MODEL = a 7B instruct model
# with LOAD_4BIT = True (about 15 GB of weights do not fit a T4 in float16).

# %% Settings
import glob
import json
import os
import subprocess
import sys
import time

_found = glob.glob("/kaggle/input/**/t3_domain_text.py", recursive=True)
INPUT_DIR = os.path.dirname(_found[0]) if _found else "/kaggle/input/autocorrect-train"
OUT_DIR = "/kaggle/working/t3"
MODEL = "Qwen/Qwen2.5-3B-Instruct"
LOAD_4BIT = False                # True for a 7B model
STYLE = "casual"                 # "formal": sentences of any register (first run); "casual": short chat messages
SENTENCES_PER_CALL = 10
BATCH = 24                       # calls generated together on one GPU
CALLS_PER_GPU = 5_000            # about 50 thousand raw sentences per GPU
MAX_MINUTES = 90                 # per process; each stops by itself
os.makedirs(OUT_DIR, exist_ok=True)

# %% The GPUs
import torch
GPUS = torch.cuda.device_count()
print("GPUs:", GPUS, [torch.cuda.get_device_name(i) for i in range(GPUS)])
assert GPUS >= 1, "turn on a GPU (Settings > Accelerator)"


def run(args, gpu, log):
    env = dict(os.environ, CUDA_VISIBLE_DEVICES=str(gpu))
    cmd = [sys.executable, f"{INPUT_DIR}/t3_domain_text.py", "--model", MODEL, "--sentences", str(SENTENCES_PER_CALL),
           "--batch", str(BATCH), "--style", STYLE] + (["--load-4bit"] if LOAD_4BIT else []) + args
    return subprocess.Popen(cmd, env=env, stdout=open(log, "w"), stderr=subprocess.STDOUT)


# %% Probe: 48 calls on the first GPU, to read before spending an hour
started = time.time()
probe = run(["--out", f"{OUT_DIR}/probe.jsonl", "--calls", "48", "--seed", "99"], 0, f"{OUT_DIR}/probe.log")
probe.wait()
print(open(f"{OUT_DIR}/probe.log").read()[-700:])
rows = [json.loads(l) for l in open(f"{OUT_DIR}/probe.jsonl", encoding="utf-8")]
print(f"{len(rows)} sentences in {time.time() - started:.0f} s (this includes loading the model)")
import random
for r in random.Random(1).sample(rows, min(40, len(rows))):
    print(f"  [{r['topic'][:28]:28}] {r['text']}")
print("distinct openings (first 3 words):", len({' '.join(r['text'].lower().split()[:3]) for r in rows}), "of", len(rows))

# %% Run: one process per GPU
procs = [run(["--out", f"{OUT_DIR}/sentences_{g}.jsonl", "--shard", str(g), "--shards", str(GPUS), "--calls", str(CALLS_PER_GPU),
              "--minutes", str(MAX_MINUTES), "--seed", "7"], g, f"{OUT_DIR}/shard{g}.log") for g in range(GPUS)]
print("started", len(procs), "processes; progress is in", OUT_DIR, "/shard*.log")

# %% Wait, showing progress
while any(p.poll() is None for p in procs):
    time.sleep(120)
    print(time.strftime("%H:%M"), *[open(f"{OUT_DIR}/shard{g}.log").read().strip().split("\n")[-1] for g in range(GPUS)], sep="\n  ")
print("exit codes:", [p.returncode for p in procs])

# %% Merge and filter
with open(f"{OUT_DIR}/sentences_all.jsonl", "w", encoding="utf-8") as out:
    for g in range(GPUS):
        for line in open(f"{OUT_DIR}/sentences_{g}.jsonl", encoding="utf-8"):
            out.write(line)
subprocess.run([sys.executable, f"{INPUT_DIR}/t3_domain_text.py", "--filter", f"{OUT_DIR}/sentences_all.jsonl",
                "--vocab", f"{INPUT_DIR}/vocab.txt", "--style", STYLE, "--out", f"{OUT_DIR}/in_domain.txt"], check=True)
print("Download", f"{OUT_DIR}/in_domain.txt", "(and sentences_all.jsonl for the topics). Next: tools/kaggle/README.md, T3.")
