"""Tests of the prompt builder and the filter of t3_domain_text.py (no GPU, no model).

    cd tools/kaggle && python -m unittest test_t3_text
"""

import json
import os
import random
import tempfile
import unittest

import t3_domain_text as t3

VOCAB = {"mình", "đang", "train", "mô", "hình", "trên", "máy", "hôm", "nay", "trời", "đẹp", "quá", "và", "tôi", "muốn", "đi", "chơi"}


class Prompts(unittest.TestCase):
    def test_same_seed_same_prompts_and_they_vary(self):
        a = t3.make_prompt(random.Random(5))[3][1]["content"]
        b = t3.make_prompt(random.Random(5))[3][1]["content"]
        self.assertEqual(a, b)
        rng = random.Random(1)
        contents = {t3.make_prompt(rng)[3][1]["content"] for _ in range(200)}
        self.assertGreater(len(contents), 150)

    def test_prompt_asks_for_plain_lines_and_carries_the_seeds(self):
        rng = random.Random(3)
        for _ in range(50):
            topic, register, seeds, messages = t3.make_prompt(rng, sentences=8)
            text = messages[1]["content"]
            self.assertIn("8 câu", text)
            self.assertIn(topic, text)
            self.assertIn("không đánh số", text)
            for term in seeds:
                self.assertIn(term, text)
            self.assertLessEqual(len(seeds), 4)


class Casual(unittest.TestCase):
    def test_casual_prompt_asks_for_short_chat_and_gives_examples(self):
        rng = random.Random(2)
        for _ in range(40):
            topic, register, seeds, messages = t3.make_prompt(rng, sentences=6, style="casual")
            text = messages[1]["content"]
            self.assertIn("6 tin nhắn", text)
            self.assertIn(topic, text)
            self.assertIn("Ví dụ về giọng văn", text)
            self.assertEqual(sum(1 for e in t3.CASUAL_EXAMPLES if e in text), 4)
            self.assertEqual(register, "tin nhắn chat thân mật")

    def test_formal_style_is_the_default_and_unchanged(self):
        a = t3.make_prompt(random.Random(8))[3]
        b = t3.make_prompt(random.Random(8), style="formal")[3]
        self.assertEqual(a, b)
        self.assertNotIn("Ví dụ về giọng văn", a[1]["content"])

    def test_short_lowercase_chat_passes_a_lower_minimum(self):
        self.assertFalse(t3.keep_sentence("mình đang train", VOCAB))                # 3 words: too short by default
        self.assertTrue(t3.keep_sentence("mình đang train", VOCAB, min_words=3))
        self.assertTrue(t3.keep_sentence("hôm nay trời đẹp quá nha", VOCAB | {"nha"}))


class Leftovers(unittest.TestCase):
    def test_labels_ai_talk_and_odd_quotes_are_dropped(self):
        self.assertFalse(t3.keep_sentence("Câu 3", VOCAB, min_words=1))
        self.assertFalse(t3.keep_sentence("Câu 7: mình đang train mô hình trên máy", VOCAB))
        self.assertFalse(t3.keep_sentence("Tôi đã được lập trình để trả lời các câu hỏi của bạn", VOCAB | {"tôi", "đã"}))
        self.assertFalse(t3.keep_sentence('Mình nói "mình đang train mô hình trên máy', VOCAB))
        self.assertTrue(t3.keep_sentence('Mình nói "mình đang train mô hình" trên máy', VOCAB))


class Parsing(unittest.TestCase):
    def test_list_marks_and_quotes_are_removed(self):
        reply = "1. Mình đang train mô hình.\n- Hôm nay trời đẹp quá!\n\n  * \"Tôi muốn đi chơi.\"\n(4) Và nữa"
        self.assertEqual(t3.parse_lines(reply), ["Mình đang train mô hình.", "Hôm nay trời đẹp quá!", "Tôi muốn đi chơi.", "Và nữa"])

    def test_text_is_normalised(self):
        decomposed = "Mình đang train"                       # "Mình" with a combining accent
        self.assertEqual(t3.parse_lines(decomposed), ["Mình đang train"])


class Filter(unittest.TestCase):
    def test_keeps_plain_sentences_with_english_terms(self):
        self.assertTrue(t3.keep_sentence("Mình đang train mô hình trên máy", VOCAB))
        self.assertTrue(t3.keep_sentence("Hôm nay mình thử query json trên server", VOCAB | {"thử"}))

    def test_drops_foreign_scripts_markup_and_odd_lengths(self):
        self.assertFalse(t3.keep_sentence("Mình đang 学习 mô hình trên máy", VOCAB))
        self.assertFalse(t3.keep_sentence("Mình đang train **mô hình** trên máy", VOCAB))
        self.assertFalse(t3.keep_sentence("Mình đang", VOCAB))
        self.assertFalse(t3.keep_sentence(" ".join(["mình"] * 60), VOCAB))

    def test_drops_sentences_of_unknown_vietnamese_words(self):
        self.assertFalse(t3.keep_sentence("Mình đàng trấn mỏ hỉnh trện mãy", VOCAB))

    def test_filter_file_removes_duplicates(self):
        with tempfile.TemporaryDirectory() as tmp:
            raw, vocab, out = (os.path.join(tmp, n) for n in ("raw.jsonl", "vocab.txt", "out.txt"))
            with open(raw, "w", encoding="utf-8") as f:
                for text in ["Mình đang train mô hình trên máy", "mình đang train mô hình trên máy!", "Hôm nay trời đẹp quá và tôi muốn đi chơi"]:
                    f.write(json.dumps({"text": text}, ensure_ascii=False) + "\n")
            with open(vocab, "w", encoding="utf-8") as f:
                f.write("\n".join(sorted(VOCAB)) + "\n")
            t3.filter_file(raw, vocab, out)
            self.assertEqual(len(open(out, encoding="utf-8").read().strip().split("\n")), 2)


if __name__ == "__main__":
    unittest.main()
