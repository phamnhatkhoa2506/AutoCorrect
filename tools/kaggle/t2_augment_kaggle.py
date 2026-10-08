# %% [markdown]
# # T2: hard mistakes picked by an open LM, on Kaggle with two T4s
#
# Runs `t2_lm_negatives.py` (see its docstring) as two independent processes, one per GPU, each on half of the
# sentences, then merges the shards into `llm_negatives.jsonl`. Nothing is shared between the GPUs, so two T4s give
# about twice the throughput of one. Public sentences only; nothing of yours is in the dataset.
#
# Input: the private dataset `autocorrect-train` (tools/kaggle/prepare_corpus.py) with GPU = "T4 x2" and Internet on.
# Run each `# %%` cell in order. NOT RUN YET: written without a GPU. Speeds are not known: the probe cell prints them.

# %% Settings
import glob
import json
import os
import subprocess
import sys
import time

_found = glob.glob("/kaggle/input/**/t2_lm_negatives.py", recursive=True)
INPUT_DIR = os.path.dirname(_found[0]) if _found else "/kaggle/input/autocorrect-train"
OUT_DIR = "/kaggle/working/t2"
# A base (not chat) model, scored as text: any 2B to 3B causal model that handles Vietnamese. Check that it is
# available and what its licence says; try more than one with the probe below and keep the one whose
# mistakes read most like a real writer's.
MODEL = "Qwen/Qwen2.5-3B"
CORPUS = "corpus.txt"          # sentences to put mistakes in: "corpus.txt" (news), or "in_domain_formal.txt" (written by T3)
SENTENCES_PER_GPU = 60_000     # sentences to try in each shard
POSITIONS = 4                  # syllables of a sentence tried
CANDIDATES = 8                 # mistakes scored per syllable (real confusions first, see t2_lm_negatives.py)
PER_SENTENCE = 2               # mistakes kept per sentence, at different positions
MIN_MARGIN = 0.5               # keep a mistake only if the original beats it by this many nats
TEMPERATURE = 1.5              # lower: more of the hard ones (near the original)
MAX_MINUTES = 6 * 60           # per process; each stops by itself
os.makedirs(OUT_DIR, exist_ok=True)

# %% The GPUs
import torch
GPUS = torch.cuda.device_count()
print("GPUs:", GPUS, [torch.cuda.get_device_name(i) for i in range(GPUS)])
assert GPUS >= 1, "turn on a GPU (Settings > Accelerator)"

# %% Probe: 300 sentences on the first GPU, to look at before spending hours
# Read the samples. A good mistake is a real word that a writer could have meant to type and that is wrong here;
# if most look like noise (a rare word, a different meaning altogether), try another MODEL or a higher MIN_MARGIN.
def run(args, gpu, log):
    env = dict(os.environ, CUDA_VISIBLE_DEVICES=str(gpu))
    return subprocess.Popen([sys.executable, f"{INPUT_DIR}/t2_lm_negatives.py", "--corpus", f"{INPUT_DIR}/{CORPUS}",
                             "--confusions", f"{INPUT_DIR}/vsec_train.jsonl", "--model", MODEL,
                             "--positions", str(POSITIONS), "--candidates", str(CANDIDATES), "--per-sentence", str(PER_SENTENCE),
                             "--min-margin", str(MIN_MARGIN), "--temperature", str(TEMPERATURE)] + args,
                            env=env, stdout=open(log, "w"), stderr=subprocess.STDOUT)

probe = run(["--out", f"{OUT_DIR}/probe.jsonl", "--limit", "300"], 0, f"{OUT_DIR}/probe.log")
started = time.time()
probe.wait()
print(open(f"{OUT_DIR}/probe.log").read()[-600:])
rows = [json.loads(l) for l in open(f"{OUT_DIR}/probe.jsonl", encoding="utf-8")]
print(f"{len(rows)} kept of 300 in {time.time() - started:.0f} s (this includes loading the model)")
kinds = {}
for r in rows:
    kinds[r["kind"]] = kinds.get(r["kind"], 0) + 1
print("kinds:", kinds, "| median margin:", sorted(r["margin"] for r in rows)[len(rows) // 2],
      "| seen in real mistakes:", sum(r["seen"] for r in rows), "of", len(rows))
for r in rows[:25]:
    shown = list(r["tokens"])
    shown[r["pos"]] = f"[{r['wrong']} <- {r['gold']}]"
    print(f"  margin {r['margin']:5.1f} {r['kind']:10} {'SEEN' if r['seen'] else '    '} " + " ".join(shown))

# %% Run: one process per GPU
procs = []
for gpu in range(GPUS):
    procs.append(run(["--out", f"{OUT_DIR}/shard{gpu}.jsonl", "--shard", str(gpu), "--shards", str(GPUS),
                      "--limit", str(SENTENCES_PER_GPU), "--minutes", str(MAX_MINUTES)], gpu, f"{OUT_DIR}/shard{gpu}.log"))
print("started", len(procs), "processes; progress is in", OUT_DIR, "/shard*.log")

# %% Wait, showing progress
while any(p.poll() is None for p in procs):
    time.sleep(120)
    print(time.strftime("%H:%M"), *[open(f"{OUT_DIR}/shard{g}.log").read().strip().split("\n")[-1] for g in range(GPUS)], sep="\n  ")
print("exit codes:", [p.returncode for p in procs])

# %% Merge
total = 0
with open(f"{OUT_DIR}/llm_negatives.jsonl", "w", encoding="utf-8") as out:
    for g in range(GPUS):
        for line in open(f"{OUT_DIR}/shard{g}.jsonl", encoding="utf-8"):
            out.write(line)
            total += 1
print(total, "records in", f"{OUT_DIR}/llm_negatives.jsonl")
print("Download it, put it in the dataset next to corpus.txt (kaggle datasets version), and the T1 notebook picks it up: "
      "LLM_FRACTION in t1_teacher_train.py. Then compare a run with and without it on the Viwiki curve.")
