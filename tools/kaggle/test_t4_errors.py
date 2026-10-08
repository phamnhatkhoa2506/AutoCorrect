"""Tests of the per-rule prompts, generators and checks of t4_llm_errors.py (no GPU, no model).

    cd tools/kaggle && python -m unittest test_t4_errors
"""

import json
import random
import string
import unicodedata
import unittest

import t1_data as d
import t4_llm_errors as t4


def version(typed, *edits):
    return {"typed": typed, "edits": [{"i": i, "from": f, "to": t} for i, f, t in edits]}


def passes(rule_id, right, wrong):
    return t4.BY_ID[rule_id].check(unicodedata.normalize("NFC", right), unicodedata.normalize("NFC", wrong))


POOL_WORDS = sorted({t for s in t4.EXAMPLE_SENTENCES for t in d.split_tokens(s) if t.isalpha() and len(t) >= 2})


class Keyboard(unittest.TestCase):
    def test_the_layout_has_every_letter_and_digit(self):
        self.assertEqual(t4.TYPABLE, set(string.ascii_lowercase) | set("1234567890"))

    def test_neighbours_are_mutual_and_every_key_has_some(self):
        for key in t4.TYPABLE:
            near = t4.adjacent(key)
            self.assertGreaterEqual(len(near), 2, key)           # a corner key such as 1 touches only 2 and q here
            self.assertLessEqual(len(near), 8, key)
            for other in near:
                self.assertIn(key, t4.adjacent(other), (key, other))

    def test_known_neighbourhoods_follow_a_real_keyboard(self):
        self.assertEqual(t4.adjacent("t"), set("ry56fg"))
        self.assertEqual(t4.adjacent("a"), set("qwsz"))
        self.assertTrue({"k", "o", "p"} <= t4.adjacent("l"))
        self.assertEqual(t4.adjacent("q"), set("12wa"))               # no Tab here: it is not a character inside a word
        self.assertNotIn("e", t4.adjacent("t"))

    def test_the_writers_habits_for_t_are_in_a_table_not_in_the_geometry(self):
        for partner in "rygeufh":
            self.assertIn(partner, t4.key_neighbours("t"))
        self.assertNotIn("e", t4.adjacent("t"))
        self.assertEqual(t4.key_neighbours("m"), t4.adjacent("m"))          # no table entry for m: the geometry alone

    def test_the_table_in_a_prompt_has_every_letter_of_its_sentence(self):
        text = t4.neighbour_table("Mình thử query json")
        for letter in "minhtuqeryjso":
            self.assertIn(f"\n{letter}: " if letter != "e" else "e: ", "\n" + text)


class Generators(unittest.TestCase):
    def test_every_rule_makes_checked_mistakes_on_many_words(self):
        rng = random.Random(3)
        needs = {1: 0.08, 2: 0.08, 3: 0.9, 4: 0.05, 5: 0.05, 6: 0.9, 7: 0.3, 8: 0.9, 9: 0.002, 10: 0.5, 11: 0.9, 12: 0.8, 13: 0.8, 14: 0.7, 15: 0.01}
        for rule in t4.RULES:
            made = 0
            for _ in range(2000):
                word = rng.choice(POOL_WORDS)
                wrong = t4.make_slip(rule, word, rng)
                if wrong:
                    made += 1
                    self.assertTrue(rule.check(t4._nfc(word), t4._nfc(wrong)), (rule.id, word, wrong))
                    self.assertNotEqual(wrong, word)
            self.assertGreater(made / 2000, needs[rule.id] * 0.5, (rule.id, made))

    def test_any_letter_and_digit_can_be_hit_instead_of_a_neighbour(self):
        rng = random.Random(1)
        for key in string.ascii_lowercase + "1234567890":
            word = key * 3
            seen = set()
            for _ in range(60):
                wrong = t4.make_neighbour(word, rng)
                if wrong:
                    self.assertTrue(passes(10, word, wrong), (key, wrong))
                    seen.update(set(wrong) - set(word))
            self.assertTrue(seen, key)
            self.assertTrue(seen <= t4.key_neighbours(key), (key, seen))

    def test_a_digit_next_to_a_letter_is_a_believable_slip(self):
        self.assertTrue(passes(10, "tôi", "t0i"))                           # o touches 9 and 0
        self.assertTrue(passes(10, "tôi", "t9i"))
        self.assertFalse(passes(10, "bạn", "bạ6"))                          # n is far from every digit
        self.assertFalse(passes(10, "tôi", "t5i"))

    def test_a_slide_goes_along_the_keyboard_whatever_the_word(self):
        rng = random.Random(2)
        for word in ("xin", "chào", "ghế", "quyển", "phở"):
            made = [t4.make_slip(t4.BY_ID[12], word, rng) for _ in range(40)]
            self.assertTrue(any(made), word)


class Prompts(unittest.TestCase):
    def test_each_prompt_is_about_one_rule_only(self):
        for rule in t4.RULES:
            text = t4.make_prompt("Mình thử query json", rule, versions=3)[1]["content"]
            self.assertIn(f"KIỂU LỖI: {rule.name}", text)
            self.assertIn(rule.ask, text)
            self.assertIn("3 phiên bản", text)
            self.assertEqual(text.count("KIỂU LỖI:"), 1)
            for other in t4.RULES:
                if other.id != rule.id:
                    self.assertNotIn(f"KIỂU LỖI: {other.name}", text)
            self.assertTrue(text.rstrip().endswith("Câu đúng: Mình thử query json"))

    def test_only_the_key_based_rules_carry_the_neighbour_table(self):
        for rule in t4.RULES:
            text = t4.make_prompt("Mình thử query json", rule)[1]["content"]
            self.assertEqual("BẢNG PHÍM SÁT NHAU" in text, rule.keys, rule.id)

    def test_the_examples_change_from_request_to_request(self):
        rule = t4.BY_ID[10]
        seen = set()
        for sentence in t4.EXAMPLE_SENTENCES[:15]:
            text = t4.make_prompt(sentence, rule)[1]["content"]
            seen |= {line for line in text.split("\n") if line.startswith("Câu đúng:")} - {f"Câu đúng: {sentence}"}
        self.assertGreater(len(seen), 12)

    def test_the_same_request_gives_the_same_prompt(self):
        rule = t4.BY_ID[3]
        self.assertEqual(t4.make_prompt("Mình thử query json", rule), t4.make_prompt("Mình thử query json", rule))

    def test_the_prompt_does_not_name_a_single_key_as_the_rule(self):
        # the writer's example of t is one entry of a table, not the rule: no prompt text singles out a key
        for rule in t4.RULES:
            self.assertNotIn("phím t hay nhầm", rule.name + rule.ask)

    def test_english_words_are_kept_unless_the_rule_may_touch_them(self):
        keeps = t4.make_prompt("x y z", t4.BY_ID[1])[1]["content"]
        allowed = t4.make_prompt("x y z", t4.BY_ID[11])[1]["content"]
        self.assertIn("Giữ NGUYÊN từ tiếng Anh", keeps)
        self.assertIn("được phép đổi", allowed)

    def test_there_are_fifteen_rules_and_the_writers_rules_b_are_all_there(self):
        self.assertEqual([r.id for r in t4.RULES], list(range(1, 16)))
        names = " ".join(r.name for r in t4.RULES)
        for text in ("phím sát bên", "trượt tay", "đè cùng lúc", "đảo hai chữ", "giữ phím"):
            self.assertIn(text, names)


class Examples(unittest.TestCase):
    def test_every_example_in_a_prompt_passes_the_check_of_its_rule(self):
        for rule in t4.RULES:
            rng = random.Random(rule.id)
            for _ in range(10):
                for sentence, answers in t4.rule_examples(rule, rng, 3):
                    for answer in answers:
                        record = t4.check(sentence, answer, rule)
                        self.assertIsNotNone(record, (rule.id, sentence, answer))
                        self.assertEqual(record["rule"], rule.id)

    def test_every_rule_finds_examples(self):
        for rule in t4.RULES:
            self.assertGreaterEqual(len(t4.rule_examples(rule, random.Random(1), 3)), 2, rule.id)

    def test_the_checks_tell_the_kinds_apart(self):
        rng = random.Random(5)
        for rule in t4.RULES:
            if rule.id in (6, 12, 13, 15):         # keys put in overlap by design; rule 15 is a far-letter replacement
                continue
            for _ in range(80):
                word = rng.choice(POOL_WORDS)
                wrong = t4.make_slip(rule, word, rng)
                if not wrong:
                    continue
                for other in (3, 11, 14):
                    if other != rule.id:
                        self.assertFalse(passes(other, word, wrong), (rule.id, other, word, wrong))


class Checks(unittest.TestCase):
    def test_tone_kinds(self):
        self.assertTrue(passes(1, "mình", "minh"))
        self.assertFalse(passes(1, "mình", "mịnh"))
        self.assertTrue(passes(2, "mình", "mịnh"))
        self.assertTrue(passes(5, "đang", "đáng"))
        self.assertTrue(passes(9, "của", "cuả"))
        self.assertFalse(passes(9, "thỏa", "thoả"))           # a convention, not a mistake

    def test_letter_kinds(self):
        self.assertTrue(passes(3, "kaggle", "kagle"))
        self.assertTrue(passes(4, "đang", "dang"))
        self.assertFalse(passes(4, "mình", "minh"))
        self.assertTrue(passes(7, "cho", "chô"))
        self.assertTrue(passes(8, "file", "fire"))
        self.assertFalse(passes(8, "model", "nodel"))         # m and n are next to each other: rule 10

    def test_keys_next_to_each_other(self):
        self.assertTrue(passes(10, "model", "nodel"))
        self.assertTrue(passes(10, "train", "yrain"))
        for partner in "rygeufh":
            self.assertTrue(passes(10, "thể", partner + "hể"), partner)
        self.assertFalse(passes(10, "thể", "mhể"))
        self.assertFalse(passes(10, "model", "modem"))
        self.assertFalse(passes(10, "model", "nodem"))        # two letters changed

    def test_held_key(self):
        self.assertTrue(passes(11, "ngu", "nguuu"))
        self.assertTrue(passes(11, "cho", "chooo"))
        self.assertFalse(passes(11, "ngu", "ngunn"))
        self.assertFalse(passes(11, "ngu", "ngu" + "u" * 6))

    def test_slide_and_several_keys_at_once(self):
        self.assertTrue(passes(12, "model", "modelkp"))
        self.assertTrue(passes(12, "bạn", "bạnmh"))
        self.assertFalse(passes(12, "bạn", "bạnm"))
        self.assertFalse(passes(12, "model", "modelzz"))
        self.assertTrue(passes(13, "bạn", "bnạn"))
        self.assertTrue(passes(13, "file", "fiole"))
        self.assertFalse(passes(13, "file", "fiqle"))

    def test_swap_and_regional_habits(self):
        self.assertTrue(passes(14, "train", "trian"))
        self.assertTrue(passes(14, "train", "tarin"))
        self.assertFalse(passes(14, "train", "triin"))
        self.assertFalse(passes(14, "train", "niart"))
        self.assertTrue(passes(15, "việc", "việt"))
        self.assertTrue(passes(15, "nên", "lên"))
        self.assertTrue(passes(15, "nghĩ", "nghỉ"))
        self.assertTrue(passes(15, "thử", "thữ"))
        self.assertFalse(passes(15, "nên", "tên"))


class Versions(unittest.TestCase):
    SENTENCE = "mình đang train model trên Kaggle"

    def test_listed_words_must_be_exactly_the_changed_ones(self):
        rule = t4.BY_ID[14]
        self.assertIsNotNone(t4.check(self.SENTENCE, version("mình đang trian model trên Kaggle", (2, "train", "trian")), rule))
        self.assertIsNone(t4.check(self.SENTENCE, version("minh đang trian model trên Kaggle", (2, "train", "trian")), rule))
        self.assertIsNone(t4.check(self.SENTENCE, version("mình đang trian model trên Kaggle"), rule))
        self.assertIsNone(t4.check(self.SENTENCE, version("mình đang trian model trên Kaggle", (3, "train", "trian")), rule))

    def test_a_slip_of_another_kind_is_dropped(self):
        self.assertIsNone(t4.check(self.SENTENCE, version("minh đang train model trên Kaggle", (0, "mình", "minh")), t4.BY_ID[14]))
        self.assertIsNotNone(t4.check(self.SENTENCE, version("minh đang train model trên Kaggle", (0, "mình", "minh")), t4.BY_ID[1]))

    def test_a_word_with_a_digit_from_a_neighbouring_key_is_kept_for_rule_10(self):
        record = t4.check("tôi đi học", version("t0i đi học", (0, "tôi", "t0i")), t4.BY_ID[10])
        self.assertIsNotNone(record)
        self.assertIsNone(t4.check("tôi đi học", version("t0i đi học", (0, "tôi", "t0i")), t4.BY_ID[1]))

    def test_length_teencode_and_numbers(self):
        rule = t4.BY_ID[1]
        self.assertIsNone(t4.check(self.SENTENCE, version("minh đang train model", (0, "mình", "minh")), rule))
        self.assertIsNone(t4.check("tôi không biết", version("tôi ko biết", (1, "không", "ko")), t4.BY_ID[3]))
        self.assertIsNone(t4.check("giá 2016 đồng", version("giá 2017 đồng", (1, "2016", "2017")), t4.BY_ID[8]))
        self.assertIsNone(t4.check("xin chào, bạn", version("xin chào. bạn", (2, ",", ".")), t4.BY_ID[8]))

    def test_at_most_the_rules_number_of_edits(self):
        rule = t4.BY_ID[1]
        many = "mình có thể gửi file cho mình không"
        self.assertIsNone(t4.check(many, version("minh co thể gửi file cho minh không", (0, "mình", "minh"), (1, "có", "co"), (5, "mình", "minh")), rule))


class Parsing(unittest.TestCase):
    def test_only_json_lines_with_typed_and_edits_are_kept(self):
        good = json.dumps(version("minh đang", (0, "mình", "minh")), ensure_ascii=False)
        reply = f"Đây là kết quả:\n{good}\nkhông phải json\n{{\"typed\": 3}}\n```{good}```"
        self.assertEqual(len(t4.parse_versions(reply)), 2)


class Choosing(unittest.TestCase):
    def test_rules_are_different_and_follow_their_share(self):
        rng = random.Random(1)
        first = [t4.choose_rules(rng, 2)[0].id for _ in range(3000)]
        self.assertGreater(first.count(1), first.count(10) * 3)
        self.assertTrue(all(len({r.id for r in t4.choose_rules(rng, 3)}) == 3 for _ in range(50)))
        self.assertIn(15, first)


class Markdown(unittest.TestCase):
    def test_the_document_has_every_rule(self):
        text = t4.markdown()
        for rule in t4.RULES:
            self.assertIn(f"### Luật {rule.id}. {rule.name}", text)
            self.assertIn(rule.ask, text)


if __name__ == "__main__":
    unittest.main()
