"""Checks for t1_data.py. Plain Python: `python -m unittest tools/kaggle/test_t1_data.py` from the repo root.
Tests that need the raw data (VSEC, Viwiki) are skipped when `data/raw` is not there."""

import os
import random
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
sys.path.insert(0, HERE)

import t1_data as d  # noqa: E402

VOCAB = d.load_vocab(os.path.join(ROOT, "data", "vi_syllables.tsv"))
IDS = {w: i + 1 for i, w in enumerate(VOCAB)}


class Letters(unittest.TestCase):
    def test_tone_operations(self):
        self.assertEqual(d.with_tone("việt", None), "viêt")
        self.assertEqual(d.with_tone("viêt", d.TONE_MARKS[4]), "việt")
        self.assertEqual(d.with_tone("hoa", d.TONE_MARKS[1]), "hòa")     # oa: the second letter
        self.assertEqual(d.with_tone("mai", d.TONE_MARKS[0]), "mái")     # ai: the first
        self.assertEqual(d.with_tone("toan", d.TONE_MARKS[0]), "toán")   # a closed syllable: the second
        self.assertEqual(d.with_tone("được", None), "đươc")
        self.assertEqual(d.drop_vowel_mark("việt"), "viẹt")   # the hat goes, the tone stays
        self.assertEqual(d.with_tone("quy", d.TONE_MARKS[0]), "quý")
        self.assertEqual(d.with_tone("gi", d.TONE_MARKS[1]), "gì")
        self.assertEqual(d.drop_vowel_mark("đi"), "di")
        self.assertIsNone(d.drop_vowel_mark("an"))
        self.assertEqual(d.bare("Việt"), "viet")

    def test_vocabulary(self):
        self.assertGreater(len(VOCAB), 5000)
        for w in ("của", "không", "được", "việt"):
            self.assertIn(w, IDS)


class Mistakes(unittest.TestCase):
    def setUp(self):
        self.c = d.Corruptor(VOCAB, seed=1)

    def test_every_kind_produces_something_different(self):
        # equal weights, so that the rare kinds (a swap is 0.2% of real mistakes) are all seen
        c = d.Corruptor(VOCAB, seed=1, weights={k: 1.0 for k in d.KIND_WEIGHTS})
        seen = set()
        for w in VOCAB[:600]:
            made = c.one(w)
            if made:
                self.assertNotEqual(made[0], w)
                seen.add(made[1])
        self.assertEqual(seen, set(d.KIND_WEIGHTS))

    def test_kinds_follow_the_measured_proportions(self):
        counts = {}
        for w in VOCAB[:3000]:
            made = self.c.one(w)
            if made:
                counts[made[1]] = counts.get(made[1], 0) + 1
        total = sum(counts.values())
        marks = sum(counts.get(k, 0) for k in ("tone", "tone_drop", "tone_add", "tone_place", "mark_drop", "mark_add"))
        # Real mistakes: about 70% are about tone and marks; some kinds do not apply to every syllable, and
        # held keys (`double`) are made on purpose more often than VSEC has them (TRAIN_BOOST).
        self.assertTrue(0.55 < marks / total < 0.85, marks / total)
        keys = sum(counts.get(k, 0) for k in ("neighbour", "double", "swap"))
        self.assertLess(keys / total, 0.10)           # rare among real mistakes (3.9%), a bit more with held keys

    def test_corruption_keeps_labels_aligned(self):
        tokens = d.split_tokens("Hôm nay, trời đẹp quá! Tôi đi học sớm.")
        screen, fix, kind = d.corrupt_tokens(tokens, self.c, IDS, 1.0)
        self.assertEqual(len(screen), len(tokens))
        for t, s, f, k in zip(tokens, screen, fix, kind):
            if f == d.KEEP:
                self.assertEqual(s, t)
                self.assertEqual(k, 0)
            else:
                self.assertEqual(f, IDS[t.lower()])
                self.assertNotEqual(s.lower(), t.lower())
        # capitals survive
        self.assertTrue(screen[0][0].isupper())


class Windows(unittest.TestCase):
    def test_prefix_window_supervises_the_last_four_positions(self):
        tokens = [f"w{i}" for i in range(60)]
        fix = [d.KEEP] * 60
        ex = d.make_window(tokens, fix, [0] * 60, 50, False, random.Random(0))
        self.assertEqual(ex.tokens[0], d.ENV_TOKEN)
        self.assertEqual(len(ex.tokens), d.WINDOW + 1)
        self.assertEqual(ex.tokens[-1], "w49")
        self.assertEqual(ex.supervised, [d.WINDOW - 3, d.WINDOW - 2, d.WINDOW - 1, d.WINDOW])
        short = d.make_window(tokens, fix, [0] * 60, 3, False, random.Random(0))
        self.assertEqual(short.supervised, [1, 2, 3])
        whole = d.make_window(tokens[:10], fix[:10], [0] * 10, 10, True, random.Random(0))
        self.assertEqual(whole.supervised, list(range(1, 11)))

    def test_eval_examples_have_exactly_the_right_context(self):
        tokens = ["a", "b", "c", "d"]
        pairs = d.eval_examples(tokens, [0] * 4, [0] * 4, [False, True, False, False], 1)
        self.assertEqual(len(pairs), 3)
        ex, error = pairs[1]
        self.assertTrue(error)
        self.assertEqual(ex.tokens, [d.ENV_TOKEN, "a", "b", "c"])
        self.assertEqual(ex.supervised, [2])
        self.assertEqual(ex.tokens[ex.supervised[0]], "b")

    def test_synthetic_example_is_well_formed(self):
        c = d.Corruptor(VOCAB, seed=2)
        rng = random.Random(3)
        tokens = d.split_tokens("Thông qua công tác tuyên truyền, vận động này, phụ huynh học sinh hiểu hơn.")
        for _ in range(200):
            ex = d.synthetic_example(tokens, c, IDS, rng)
            self.assertEqual(len(ex.tokens), len(ex.fix))
            self.assertEqual(len(ex.tokens), len(ex.kind))
            self.assertTrue(all(0 < p < len(ex.tokens) for p in ex.supervised))
            self.assertLessEqual(len(ex.tokens), d.WINDOW + 1)


class LlmNegatives(unittest.TestCase):
    """The selection side of t2_lm_negatives.py (the scoring itself needs a model and a GPU)."""

    def setUp(self):
        import t2_lm_negatives as t2
        self.t2 = t2
        self.siblings = d.Siblings(VOCAB)
        self.corruptor = d.Corruptor(VOCAB, seed=5)
        self.confusions = {"của": {"cuả": 24, "cùa": 12, "cua": 8}, "được": {"đươc": 31, "dược": 25}}

    def test_candidates_come_from_real_confusions_first(self):
        rng = random.Random(1)
        seen_any = set()
        for _ in range(40):
            options = self.t2.candidates_for("của", self.siblings, self.confusions, self.corruptor, rng)
            self.assertLessEqual(len(options), 8)
            self.assertEqual(len({w for w, _, _ in options}), len(options))     # no duplicates
            self.assertNotIn("của", [w for w, _, _ in options])
            for wrong, kind, seen in options:
                self.assertIn(kind, d.KINDS)
                self.assertEqual(seen, wrong in self.confusions["của"])
                if seen:
                    seen_any.add(wrong)
        self.assertEqual(seen_any, {"cuả", "cùa", "cua"})

    def test_job_changes_syllables_in_the_original_text(self):
        rng = random.Random(1)
        text = "Hôm nay, trời đẹp quá! Tôi đi học sớm."
        for _ in range(40):
            job = self.t2.make_job(text, self.siblings, IDS, self.confusions, self.corruptor, rng, positions=3)
            self.assertIsNotNone(job)
            original, toks, chosen = job
            self.assertEqual(original, text)
            self.assertLessEqual(len(chosen), 3)
            self.assertEqual(len({i for i, _ in chosen}), len(chosen))           # distinct positions
            for i, options in chosen:
                for wrong, kind, seen in options:
                    changed = self.t2.changed_text(original, toks, i, wrong)
                    self.assertNotEqual(changed, original)
                    # nothing else moved: punctuation and spacing are those of the original
                    self.assertEqual(changed.count(","), 1)
                    self.assertEqual(changed[:toks[i][1]], original[:toks[i][1]])

    def test_pick_errors_never_keeps_a_candidate_the_model_prefers(self):
        rng = random.Random(2)
        # (position, margin, seen): nothing reaches the margin, so nothing is kept
        self.assertEqual(self.t2.pick_errors([(0, -3.0, False), (1, 0.2, False)], rng, min_margin=0.5), [])
        picks = []
        for _ in range(300):
            picks += self.t2.pick_errors([(0, 0.2, False), (1, 1.5, False), (2, 12.0, False)], rng, per_sentence=1)
        self.assertNotIn(0, picks)                                # margin below the floor: dropped
        self.assertGreater(picks.count(1), 5 * picks.count(2))    # near the original is far likelier
        # at most one per position, and at most per_sentence of them
        for _ in range(100):
            got = self.t2.pick_errors([(0, 2.0, False), (0, 3.0, False), (1, 2.5, False), (2, 9.0, True)], rng, per_sentence=2)
            self.assertLessEqual(len(got), 2)
            self.assertEqual(len({[0, 0, 1, 2][i] for i in got}), len(got))
        # a confusion seen in real mistakes is preferred over an unseen one at the same margin
        wins = [self.t2.pick_errors([(0, 2.0, True), (1, 2.0, False)], rng, per_sentence=1)[0] for _ in range(400)]
        self.assertGreater(wins.count(0), 1.8 * wins.count(1))

    def test_pick_errors_valid_words_need_a_bigger_margin_and_kinds_are_balanced(self):
        rng = random.Random(4)
        # a valid syllable that loses by only 2 nats is left alone; a non-word with the same margin is kept
        self.assertEqual(self.t2.pick_errors([(0, 2.0, False, "tone", True)], rng), [])
        self.assertEqual(self.t2.pick_errors([(0, 2.0, False, "tone", False)], rng), [0])
        self.assertEqual(self.t2.pick_errors([(0, 3.5, False, "tone", True)], rng), [0])
        # nine "omit" candidates and one "tone_drop" at the same margin: kinds are weighted to their shares
        pool = [(i, 2.0, False, "omit", False) for i in range(9)] + [(9, 2.0, False, "tone_drop", False)]
        got = [self.t2.pick_errors(pool, rng, per_sentence=1)[0] for _ in range(600)]
        expected = d.KIND_WEIGHTS["tone_drop"] / (d.KIND_WEIGHTS["tone_drop"] + d.KIND_WEIGHTS["omit"])
        self.assertLess(abs(got.count(9) / 600 - expected), 0.07)

    def test_a_held_key_is_a_double_and_the_corruptor_makes_runs(self):
        for wrong, right in (("nguuu", "ngu"), ("nguu", "ngu"), ("ngooong", "ngong"), ("thhhì", "thì")):
            self.assertEqual(d.classify_op(wrong, right), "double", (wrong, right))
        # not a held key: another letter changed, or a run shorter than before
        self.assertNotEqual(d.classify_op("nguuy", "ngu"), "double")
        self.assertIsNone(d.classify_op("ngu", "nguuu"))
        c = d.Corruptor(VOCAB, seed=3, weights={"double": 1.0})
        lengths = set()
        for _ in range(400):
            made = c.make("ngu", "double")
            self.assertEqual(d.classify_op(made, "ngu"), "double", made)
            lengths.add(len(made) - 3)
        self.assertEqual(lengths, {1, 2, 3, 4})                    # one to four extra letters

    def test_pick_errors_keeps_junk_but_less_often(self):
        rng = random.Random(5)
        # same kind and margin; one is a non-word never seen (junk), one was seen in real mistakes
        pool = [(0, 2.0, False, "omit", False), (1, 2.0, True, "omit", False)]
        got = [self.t2.pick_errors(pool, rng, per_sentence=1, junk_weight=0.3)[0] for _ in range(600)]
        self.assertGreater(got.count(0), 30)                      # still taught
        self.assertGreater(got.count(1), 4 * got.count(0))       # but far less than the main cases

    def test_record_turns_into_a_training_window(self):
        rng = random.Random(3)
        _, toks = self.t2.spans("Chúng tôi nhận được ốm đau.")
        rec = self.t2.make_record(toks, 3, "đước", "tone", 2.0, -30.0)
        self.assertEqual(rec["tokens"][3], "đước")
        self.assertEqual(rec["gold"], "được")
        for _ in range(50):
            ex = d.llm_negative_example(rec, IDS, rng)
            positions = [p for p in ex.supervised if ex.fix[p] > 0]
            if positions:                                    # the window may be cropped away from it
                self.assertEqual(ex.fix[positions[0]], IDS["được"])
                self.assertEqual(ex.tokens[positions[0]], "đước")
                self.assertEqual(d.KINDS[ex.kind[positions[0]]], "tone")


class RealMistakes(unittest.TestCase):
    def test_classify_op_on_known_cases(self):
        c = d.classify_op
        self.assertEqual(c("hê", "hệ"), "tone_drop")       # the nang tone is missing, the hat is there
        self.assertEqual(c("hế", "hệ"), "tone")            # another tone
        self.assertEqual(c("viêt", "việt"), "tone_drop")   # the tone is missing
        self.assertEqual(c("viết", "viêt"), "tone_add")
        self.assertEqual(c("đươc", "được"), "tone_drop")
        self.assertEqual(c("dược", "được"), "mark_drop")   # the stroke of đ is missing
        self.assertEqual(c("thoả", "thỏa"), "tone_convention")   # oa, oe, uy: either place is in use
        self.assertEqual(c("cuả", "của"), "tone_place")          # a real mistake: the tone on the wrong vowel
        self.assertEqual(c("hoà", "hòa"), "tone_convention")
        self.assertEqual(c("tranhh", "tranh"), "double")
        self.assertEqual(c("iên", "nhiên"), None)          # two letters missing: not one slip
        self.assertEqual(c("nhiê", "nhiên"), "omit")
        self.assertEqual(c("hang", "hàng"), "tone_drop")
        self.assertEqual(c("cahc", "cách"), None)
        self.assertEqual(c("cach", "cách"), "tone_drop")
        self.assertIsNone(c("việt", "việt"))

    def test_add_vowel_mark(self):
        r = random.Random(1)
        self.assertIn(d.add_vowel_mark("ban", r), ("bân", "băn"))
        self.assertIn(d.add_vowel_mark("dan", r), ("đan", "dân", "dăn"))
        self.assertIsNone(d.add_vowel_mark("việt", r))      # nothing left to add

    def test_move_tone(self):
        r = random.Random(1)
        self.assertEqual(d.move_tone("của", r), "cuả")
        self.assertIsNone(d.move_tone("an", r))             # one vowel: nowhere to move it
        self.assertIsNone(d.move_tone("ba", r))             # no tone to move

    def test_a_convention_is_not_a_mistake_and_other_mistakes_get_a_real_kind(self):
        rec = {"annotations": [
            {"current_syllable": "thoả", "is_correct": False, "alternative_syllables": ["thỏa"], "id": 1},
            {"current_syllable": "mãn", "is_correct": True, "alternative_syllables": [], "id": 2},
            {"current_syllable": "hê", "is_correct": False, "alternative_syllables": ["hệ"], "id": 3},
        ]}
        tokens, fix, kind, error = d.vsec_sentence(rec, IDS)
        self.assertEqual(error, [False, False, True])
        self.assertEqual(fix[0], d.IGNORE)
        self.assertEqual(d.KINDS[kind[2]], "tone_drop")

    @unittest.skipUnless(os.path.exists(os.path.join(ROOT, "data", "raw", "vsec", "VSEC.jsonl")), "VSEC not downloaded")
    def test_mined_proportions_are_the_ones_in_the_module(self):
        records = d.read_jsonl(os.path.join(ROOT, "data", "raw", "vsec", "VSEC.jsonl"))
        ops, valid, conf = d.mine_operations(records, IDS)
        total = sum(ops.get(k, 0) for k in d.KIND_COUNTS)
        # Within a few points of KIND_COUNTS (those were mined on the training split only).
        for k in ("tone", "tone_drop", "omit"):
            self.assertLess(abs(ops[k] / total - d.KIND_WEIGHTS[k]), 0.04, k)
        self.assertIn("của", conf)
        self.assertIn("cuả", conf["của"])


class Measuring(unittest.TestCase):
    def test_curve(self):
        rows = [(0.99, 5, 5, True), (0.95, 6, 7, True), (0.9, 3, 3, False), (0.2, 1, 1, True), (0.1, 0, 0, False)]
        c = {x["threshold"]: x for x in d.curve(rows)}
        self.assertEqual(c[0.9]["changed"], 3)
        self.assertAlmostEqual(c[0.9]["detect_precision"], 2 / 3)
        self.assertAlmostEqual(c[0.9]["right_precision"], 1 / 3)
        self.assertAlmostEqual(c[0.9]["detect_recall"], 2 / 3)
        self.assertAlmostEqual(c[0.9]["false_per_1000"], 1000 * 1 / 2)


@unittest.skipUnless(os.path.exists(os.path.join(ROOT, "data", "raw", "vsec", "VSEC.jsonl")), "VSEC not downloaded")
class Vsec(unittest.TestCase):
    def test_parses_real_records(self):
        records = d.read_jsonl(os.path.join(ROOT, "data", "raw", "vsec", "VSEC.jsonl"), limit=500)
        errors = known = 0
        for r in records:
            tokens, fix, kind, error = d.vsec_sentence(r, IDS)
            self.assertEqual(len(tokens), len(fix))
            self.assertEqual(len(tokens), len(error))
            errors += sum(error)
            known += sum(1 for f, e in zip(fix, error) if e and f != d.IGNORE)
        self.assertGreater(errors, 400)
        self.assertGreater(known / errors, 0.6, "most real mistakes should have a fix inside the vocabulary")


@unittest.skipUnless(os.path.exists(os.path.join(ROOT, "data", "raw", "viwiki_spelling", "spelling_test.json")), "Viwiki not downloaded")
class Viwiki(unittest.TestCase):
    def test_every_annotation_lands_on_its_word(self):
        docs = d.read_jsonl(os.path.join(ROOT, "data", "raw", "viwiki_spelling", "spelling_test.json"), limit=5)
        mistakes = errors = 0
        for doc in docs:
            mistakes += len(doc["mistakes"])
            for tokens, fix, kind, error in d.viwiki_sentences(doc, IDS):
                self.assertEqual(len(tokens), len(error))
                errors += sum(error)
        self.assertGreater(mistakes, 0)
        self.assertGreater(errors / mistakes, 0.95)


if __name__ == "__main__":
    unittest.main()
