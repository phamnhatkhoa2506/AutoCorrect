"""Tests of the physical keyboard layout of keyboard.py (the writer's Dell Vostro 3405, measured on a photo).

    cd tools/kaggle && python -m unittest test_keyboard
"""

import json
import os
import string
import tempfile
import unittest

import keyboard as kb


class Layout(unittest.TestCase):
    def test_the_four_main_rows_are_fourteen_and_a_half_key_widths_wide(self):
        for r, row in enumerate(kb.ROWS[:4]):
            self.assertAlmostEqual(sum(w for _, w in row), 14.5, msg=f"row {r}")

    def test_the_bottom_row_leaves_room_for_three_arrow_keys(self):
        self.assertAlmostEqual(sum(w for _, w in kb.ROWS[4]) + 3 * kb.ARROW_WIDTH, 14.5)

    def test_it_has_the_forty_seven_typing_keys_and_the_modifiers(self):
        chars = kb.char_keys()
        self.assertEqual(len(chars), 47)
        self.assertTrue(set(string.ascii_lowercase) | set("0123456789") <= set(chars))
        self.assertTrue(set(";',./[]\\`-=") <= set(chars))
        for name in ("space", "lshift", "rshift", "caps", "tab", "enter", "backspace", "up", "pgdn"):
            self.assertEqual(kb.kind(name), "modifier")

    def test_keys_follow_each_other_without_gaps_in_a_row(self):
        for row in kb.ROWS:
            end = 0.0
            for name, width in row:
                self.assertAlmostEqual(kb.LAYOUT[name][2], end, msg=name)
                end += width

    def test_this_keyboard_has_no_right_windows_key_and_arrows_are_half_height(self):
        self.assertNotIn("rwin", kb.LAYOUT)
        for name in ("pgup", "up", "pgdn", "left", "down", "right"):
            y0, y1, _, _ = kb.LAYOUT[name]
            self.assertAlmostEqual(y1 - y0, 0.5, msg=name)


class Neighbours(unittest.TestCase):
    def test_neighbours_are_mutual_and_every_key_has_some(self):
        for key in kb.LAYOUT:
            near = kb.adjacent(key)
            self.assertGreaterEqual(len(near), 2, key)
            for other in near:
                self.assertIn(key, kb.adjacent(other), (key, other))

    def test_letters_have_the_neighbours_of_a_real_keyboard(self):
        self.assertEqual(kb.adjacent_characters("t"), set("ry56fg"))
        self.assertEqual(kb.adjacent_characters("a"), set("qwsz"))
        self.assertEqual(kb.adjacent_characters("f"), set("rtdgcv"))
        self.assertTrue({"k", "o", "p"} <= kb.adjacent_characters("l"))
        self.assertEqual(kb.adjacent_characters("m"), set("jkn"))

    def test_modifiers_and_punctuation_are_next_to_the_letters_they_touch(self):
        self.assertTrue({"caps", "lshift"} <= kb.adjacent("a"))          # aiming at a and pressing Caps Lock or Shift instead
        self.assertIn("lshift", kb.adjacent("z"))
        self.assertIn(";", kb.adjacent("l"))
        self.assertIn("'", kb.adjacent(";"))
        self.assertIn("enter", kb.adjacent("'"))
        self.assertIn("backspace", kb.adjacent("="))
        self.assertIn("tab", kb.adjacent("q"))
        self.assertNotIn("[", kb.adjacent("o"))                          # p is between them

    def test_space_covers_c_to_m_and_not_the_comma(self):
        space = kb.adjacent("space")
        self.assertTrue(set("cvbnm") <= space)
        self.assertTrue({"lalt", "ralt"} <= space)
        self.assertFalse(space & set("zxa,."))                           # as on the photo: Space ends under M

    def test_shift_keys_and_the_arrow_cluster(self):
        shift = kb.adjacent("lshift")
        self.assertTrue({"z", "a", "caps"} <= shift)
        self.assertFalse(shift & set("xsq"))
        self.assertTrue({"pgup", "up", "pgdn"} <= kb.adjacent("rshift"))  # the arrow keys sit under Right Shift
        self.assertIn("rctrl", kb.adjacent("ralt"))
        self.assertTrue({"down", "pgdn"} <= kb.adjacent("right"))

    def test_corner_keys_have_several_neighbours(self):
        self.assertTrue({"2", "q", "`", "tab"} <= kb.adjacent("1"))

    def test_only_letters_and_digits_count_inside_a_word(self):
        for key in kb.char_keys():
            for other in kb.adjacent_characters(key):
                self.assertTrue(other.isalnum() and other.isascii())


class Export(unittest.TestCase):
    def test_the_json_has_every_key_with_its_footprint(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = os.path.join(tmp, "layout.json")
            kb.export(path)
            with open(path, encoding="utf-8") as f:
                data = json.load(f)
        self.assertEqual(len(data["keys"]), len(kb.LAYOUT))
        by = {k["name"]: k for k in data["keys"]}
        self.assertEqual((by["space"]["y0"], by["space"]["y1"], by["space"]["x0"], by["space"]["x1"]), (4.0, 5.0, 4.15, 9.25))
        self.assertEqual(by["a"]["kind"], "char")
        self.assertEqual(by["caps"]["kind"], "modifier")
        self.assertEqual(by["left"]["y1"] - by["left"]["y0"], 0.5)


if __name__ == "__main__":
    unittest.main()
