"""T2: hard, realistic mistakes for the teacher, picked by an open language model (RESEARCH.md, section 5).

For a clean sentence the answer is known: the sentence itself. The model never decides what is right.
It only helps choose WHICH mistake to make, among candidates that real writers produce:

  * confusions SEEN in real mistakes (VSEC training split): for "của", what people wrote instead
    ("cuả", "cùa", "cua"...), weighted by how often;
  * mistakes made with the operations and proportions MEASURED on those real mistakes (t1_data.Corruptor);
  * a couple of valid syllables with the same letters and other marks (the hard ones).

Several positions of the sentence are tried. Each substituted sentence is scored with a causal language
model, and a candidate is kept only if it is worse than the original by at least `--min-margin` nats
(otherwise nobody can say the original is the right one). Among those, the ones the model finds nearly as
good as the original are likelier to be picked: the mistakes that need the context to be told apart.

    python t2_lm_negatives.py --corpus corpus.txt --confusions vsec_train.jsonl --out negatives.jsonl \\
        --model Qwen/Qwen2.5-3B --shard 0 --shards 2      # one process per GPU, CUDA_VISIBLE_DEVICES outside

Output, one JSON line per kept mistake (up to `--per-sentence` per sentence, at different positions):
  tokens   the sentence as the writer would have left it, split like t1_data
  pos      index of the changed token;  gold: the original syllable (lower case);  wrong: what stands there now
  kind     the operation, one of t1_data.KINDS (what a writer did to the syllable)
  seen     true if this exact confusion was seen in the real mistakes
  margin   log P(original sentence) - log P(changed sentence), in nats: how easy the mistake is to see
  lp_orig  log P(original sentence)

The selection logic is plain Python (tested in test_t1_data.py); torch is only imported to score.
"""

import argparse
import collections
import json
import math
import os
import random
import sys
import time
import unicodedata

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import t1_data as d  # noqa: E402

SEEN_PRIOR = 3.0         # a confusion seen in real mistakes is this much likelier to be picked


def spans(text):
    """(text, [(token, start, end), ...]): the NFC text and its tokens, the way t1_data splits sentences."""
    text = unicodedata.normalize("NFC", text)
    return text, [(m.group(), m.start(), m.end()) for m in d.TOKEN.finditer(text)]


def candidates_for(gold, siblings, confusions, corruptor, rng, limit=8):
    """[(wrong, kind, seen)] for a lower-case syllable: real confusions first, then measured operations,
    then a couple of valid syllables with the same letters."""
    out, taken = [], {gold}

    def add(wrong, kind, seen):
        wrong = unicodedata.normalize("NFC", wrong)
        if wrong and wrong not in taken and kind != "tone_convention":
            taken.add(wrong)
            out.append((wrong, kind, seen))

    real = confusions.get(gold, {})
    if real:
        words, counts = list(real), list(real.values())
        for _ in range(min(4, len(words))):
            wrong = rng.choices(words, counts)[0]
            add(wrong, d.classify_op(wrong, gold) or "substitute", True)
    for _ in range(12):
        if len(out) >= limit - 2:
            break
        made = corruptor.one(gold)
        if made:
            add(made[0], made[1], made[0] in real)
    same_letters = siblings.bare(gold)
    for sib in rng.sample(same_letters, min(2, len(same_letters))):
        add(sib, d.classify_op(sib, gold) or "substitute", sib in real)
    return out[:limit]


def make_job(text, siblings, vocab_ids, confusions, corruptor, rng, positions=4, limit=8):
    """(text, tokens, [(index, [(wrong, kind, seen), ...]), ...]) or None: up to `positions` syllables
    of the sentence, each with candidate mistakes."""
    text, toks = spans(text)
    eligible = [i for i, (tok, _, _) in enumerate(toks) if tok.lower() in vocab_ids and len(tok) >= 2]
    rng.shuffle(eligible)
    chosen = []
    for i in eligible:
        options = candidates_for(toks[i][0].lower(), siblings, confusions, corruptor, rng, limit)
        if options:
            chosen.append((i, options))
        if len(chosen) >= positions:
            break
    return (text, toks, chosen) if chosen else None


def changed_text(text, toks, i, candidate):
    tok, start, end = toks[i]
    return text[:start] + d.match_case(tok, candidate) + text[end:]


def pick_errors(pool, rng, per_sentence=2, min_margin=0.5, temperature=1.5, valid_margin=3.0, junk_weight=0.3):
    """Which candidates to keep. `pool` is [(position, margin, seen[, kind[, valid]]), ...] with margin =
    log P(original) - log P(candidate). Candidates the model likes about as much as the original or better
    are never kept; a candidate that is itself a valid syllable ("valid") must lose by `valid_margin`, since a
    valid word in that place may simply be right. Among the rest, the closer to the original (and the more it
    was seen in real mistakes) the likelier, and when kinds are given each is weighted by its measured share
    (`KIND_WEIGHTS`) over its share in the pool, so easy kinds do not crowd out the others.
    At most one per position. Returns indices into `pool`."""
    def floor(e):
        return valid_margin if len(e) > 4 and e[4] else min_margin
    alive = {i: (e[1], e[2]) for i, e in enumerate(pool) if e[1] >= floor(e)}
    kinds = collections.Counter(pool[i][3] for i in alive if len(pool[i]) > 3 and pool[i][3])

    def balance(i):
        kind = pool[i][3] if len(pool[i]) > 3 else None
        return d.KIND_WEIGHTS.get(kind, 0.01) / kinds[kind] if kind else 1.0
    def junk(i):                          # not a word and never seen in real mistakes: kept, but less often
        e = pool[i]
        return junk_weight if len(e) > 4 and not e[4] and not e[2] else 1.0
    picked, used = [], set()
    for _ in range(per_sentence):
        ok = [i for i in alive if pool[i][0] not in used]
        if not ok:
            break
        weights = [balance(i) * junk(i) * (SEEN_PRIOR if alive[i][1] else 1.0) * math.exp(-alive[i][0] / temperature) for i in ok]
        i = rng.choices(ok, weights)[0]
        picked.append(i)
        used.add(pool[i][0])
    return picked


def make_record(toks, i, candidate, kind, margin, lp_orig, seen=False):
    tokens = [t for t, _, _ in toks]
    tokens[i] = d.match_case(tokens[i], candidate)
    return {"tokens": tokens, "pos": i, "gold": toks[i][0].lower(), "wrong": candidate, "kind": kind,
            "seen": bool(seen), "margin": round(margin, 3), "lp_orig": round(lp_orig, 3)}


# ----------------------------------------------------------------------------- scoring (needs torch)

def score_texts(model, tok, texts, device, bos_id, micro_batch=32):
    """Sum of log P(token | previous tokens) over each text, the first token conditioned on `bos_id`."""
    import torch
    out = []
    for start in range(0, len(texts), micro_batch):
        chunk = texts[start:start + micro_batch]
        ids = [[bos_id] + tok.encode(t, add_special_tokens=False) for t in chunk]
        width = max(len(s) for s in ids)
        pad = tok.pad_token_id if tok.pad_token_id is not None else bos_id
        x = torch.full((len(ids), width), pad, dtype=torch.long)
        mask = torch.zeros((len(ids), width), dtype=torch.long)
        for row, s in enumerate(ids):
            x[row, :len(s)] = torch.tensor(s)
            mask[row, :len(s)] = 1
        x, mask = x.to(device), mask.to(device)
        with torch.no_grad():
            logits = model(input_ids=x, attention_mask=mask).logits[:, :-1].float()
            logp = torch.log_softmax(logits, dim=-1).gather(-1, x[:, 1:, None])[..., 0]
        out += (logp * mask[:, 1:]).sum(1).tolist()
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--corpus", required=True)
    ap.add_argument("--confusions", default=None, help="VSEC training records: the real mistakes to learn confusions from")
    ap.add_argument("--vocab", default=None, help="vocab.txt (default: next to the corpus)")
    ap.add_argument("--out", required=True)
    ap.add_argument("--model", default="Qwen/Qwen2.5-3B")
    ap.add_argument("--shard", type=int, default=0)
    ap.add_argument("--shards", type=int, default=1)
    ap.add_argument("--limit", type=int, default=0, help="sentences to try in this shard (0: all)")
    ap.add_argument("--positions", type=int, default=4, help="syllables of a sentence tried")
    ap.add_argument("--candidates", type=int, default=8, help="mistakes scored per syllable")
    ap.add_argument("--per-sentence", type=int, default=2, help="mistakes kept per sentence, at different positions")
    ap.add_argument("--min-margin", type=float, default=0.5)
    ap.add_argument("--temperature", type=float, default=1.5)
    ap.add_argument("--sentences-per-batch", type=int, default=2)
    ap.add_argument("--minutes", type=float, default=0, help="stop after this long (0: no limit)")
    ap.add_argument("--seed", type=int, default=7)
    args = ap.parse_args()

    import torch
    from transformers import AutoModelForCausalLM, AutoTokenizer

    vocab_path = args.vocab or os.path.join(os.path.dirname(os.path.abspath(args.corpus)), "vocab.txt")
    vocab = [w.strip() for w in open(vocab_path, encoding="utf-8") if w.strip()]
    vocab_ids = {w: i + 1 for i, w in enumerate(vocab)}
    siblings = d.Siblings(vocab)
    confusions = {}
    if args.confusions:
        ops, valid, confusions = d.mine_operations(d.read_jsonl(args.confusions), vocab_ids)
        print(f"real confusions: {sum(len(v) for v in confusions.values())} pairs for {len(confusions)} syllables, "
              f"{sum(ops.values())} mistakes", flush=True)
    corruptor = d.Corruptor(vocab, seed=args.seed * 100 + args.shard)
    sentences = [l for l in open(args.corpus, encoding="utf-8").read().split("\n") if l.strip()][args.shard::args.shards]
    if args.limit:
        sentences = sentences[: args.limit]
    rng = random.Random(args.seed * 100 + args.shard)

    device = "cuda" if torch.cuda.is_available() else "cpu"
    tok = AutoTokenizer.from_pretrained(args.model)
    model = AutoModelForCausalLM.from_pretrained(args.model, torch_dtype=torch.float16 if device == "cuda" else torch.float32).to(device).eval()
    bos = tok.bos_token_id if tok.bos_token_id is not None else tok.eos_token_id
    print(f"shard {args.shard}/{args.shards}: {len(sentences)} sentences, model {args.model} on {device}", flush=True)

    started, tried, kept, margins = time.time(), 0, 0, []
    with open(args.out, "w", encoding="utf-8") as out:
        for at in range(0, len(sentences), args.sentences_per_batch):
            if args.minutes and (time.time() - started) / 60 > args.minutes:
                print("time is up", flush=True)
                break
            batch = sentences[at:at + args.sentences_per_batch]
            jobs = [j for j in (make_job(s, siblings, vocab_ids, confusions, corruptor, rng, args.positions, args.candidates) for s in batch) if j]
            tried += len(batch)
            if not jobs:
                continue
            texts = []
            for text, toks, chosen in jobs:
                texts.append(text)
                for i, options in chosen:
                    texts += [changed_text(text, toks, i, w) for w, _, _ in options]
            try:
                scores = score_texts(model, tok, texts, device, bos)
            except torch.cuda.OutOfMemoryError:
                torch.cuda.empty_cache()
                print("out of memory on a batch: skipped (lower --sentences-per-batch, --positions or --candidates)", flush=True)
                continue
            cursor = 0
            for text, toks, chosen in jobs:
                lp0 = scores[cursor]
                cursor += 1
                pool, where = [], []
                for i, options in chosen:
                    for w, kind, seen in options:
                        pool.append((i, lp0 - scores[cursor], seen, kind, w in vocab_ids))
                        where.append((i, w, kind, seen))
                        cursor += 1
                for k in pick_errors(pool, rng, args.per_sentence, args.min_margin, args.temperature):
                    i, w, kind, seen = where[k]
                    rec = make_record(toks, i, w, kind, pool[k][1], lp0, seen)
                    out.write(json.dumps(rec, ensure_ascii=False) + "\n")
                    kept += 1
                    margins.append(rec["margin"])
            if (at // args.sentences_per_batch) % 50 == 0:
                rate = tried / max(1e-9, time.time() - started)
                print(f"  tried {tried}, kept {kept}, {rate:.1f} sentences/s", flush=True)
                out.flush()
    margins.sort()
    mid = margins[len(margins) // 2] if margins else float("nan")
    print(f"done: tried {tried}, kept {kept} ({kept / max(1, tried):.2f} per sentence), median margin {mid:.2f} nats, {(time.time() - started) / 60:.1f} min", flush=True)


if __name__ == "__main__":
    main()
