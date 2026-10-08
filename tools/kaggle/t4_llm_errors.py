"""T4: an open LLM proposes typing mistakes for a correct sentence, one rule per request; a program checks each (RESEARCH.md, 5).

A small LLM does one thing well when it is asked one thing. So each rule below has its own prompt (what to do, step by
step) and its own check (the mistake must be of that very kind). The prompts are written for any word and any key, not for
the few cases the writer happened to name:
  * the keyboard is a real layout (rows, offsets, the number row); the keys next to any key are computed from it
    (`adjacent`), and each key-based prompt carries the table of neighbours for the letters of its own sentence;
  * the writer's own habits (the key `t` taken for r, y, g and more for e, u, f, h) are one entry of a table of priors
    (`USER_PARTNERS`) that can hold any other key;
  * the examples in a prompt are not hand-written: each request draws other sentences from a varied pool and makes the
    examples with the rule's own generator (`make_slip`), which are then checked like any proposal.
The same generators make the mistakes by program alone (rule-based augmentation of the key-level rules), no LLM needed.

The answer is always the correct sentence. The LLM returns the sentence as it would stand after typing and lists the words it
changed; `check` keeps a version only if
  * the sentence has the same tokens in the same places, and the listed words are exactly the ones that differ;
  * each changed word passes the check of the rule that was asked (the kind of slip, measured by `t1_data.classify_op`
    or tested here for the key-level rules), and is not teen-code or another word of the sentence.
What the LLM says about meaning is not trusted; what the program cannot verify is dropped.

    python t4_llm_errors.py --show 10                                  (print the prompt of rule 10, no GPU)
    python t4_llm_errors.py --markdown                                 (all rules, for AUGMENT_PROMPT.md)
    python t4_llm_errors.py --sentences clean.txt --out errors.jsonl --model Qwen/Qwen2.5-3B-Instruct --minutes 60
"""

import argparse
import json
import random
import re
import string
import time
import unicodedata
import zlib
from collections import namedtuple

import keyboard as kb
import t1_data as d

HOI, NGA = d.TONE_MARKS[2], d.TONE_MARKS[3]
TEENCODE = {"ko", "k", "dc", "đc", "đk", "dk", "vs", "j", "ntn", "ròi", "hok", "hông", "z"}   # opt-in only, never taught here


def _nfc(s):
    return unicodedata.normalize("NFC", s.lower())


# ----------------------------------------------------------------------------- the keyboard
# The layout itself (rows, key widths, modifiers, punctuation) is `keyboard.py`: a table of the writer's own device, a US
# laptop body. A slip inside a word can only put a letter or a digit there; the keys that would split the word (Space,
# punctuation) or change the case (Shift, Caps Lock) are not candidates at this level.
TYPABLE = set(string.ascii_lowercase) | set("0123456789")

USER_PARTNERS = kb.USER_PARTNERS      # the writer's habits per key, kept with the layout (keyboard.py)


def adjacent(key):
    """Keys that touch `key` on the layout and type a letter or a digit (see `keyboard.adjacent` for all of them)."""
    return kb.adjacent_characters(key)


def key_neighbours(key):
    """Keys the finger may hit instead of, or next to, `key`: the geometry plus the writer's own table."""
    out = adjacent(key) | set(USER_PARTNERS.get(key, ("", ""))[0])
    out.discard(key)
    return out


def neighbour_table(sentence):
    """For the letters of a sentence, the keys next to each (the writer's habits included): a line per letter."""
    letters = sorted({d.bare(c) for c in sentence if d.bare(c).isalpha() and d.bare(c).isascii()})
    return "\n".join(f"{k}: {' '.join(sorted(key_neighbours(k)))}" for k in letters)


# ----------------------------------------------------------------------------- the checks, one per kind of slip
def kind_is(*kinds):
    return lambda right, wrong: d.classify_op(wrong, right) in kinds


def key_neighbour(right, wrong):
    """One key replaced by a key next to it (a letter or a digit)."""
    if len(right) != len(wrong):
        return False
    diff = [i for i in range(len(right)) if right[i] != wrong[i]]
    if len(diff) != 1:
        return False
    a, b = d.bare(right[diff[0]]), d.bare(wrong[diff[0]])
    return len(a) == 1 and len(b) == 1 and a != b and b in key_neighbours(a)


def held_key(right, wrong):
    """One letter typed more times in a row, up to five in all."""
    return d._repeat_of(wrong, right) and max(n for _, n in d._runs(wrong)) <= 5


def inserted_block(low, high):
    """`low` to `high` keys put in front of, inside or after the word, each next to a letter beside it or to the key before it."""
    def check(right, wrong):
        n = len(wrong) - len(right)
        if not low <= n <= high:
            return False
        for i in range(len(right) + 1):
            if wrong[:i] + wrong[i + n:] != right:
                continue
            left = d.bare(right[i - 1]) if i > 0 else ""
            after = d.bare(right[i]) if i < len(right) else ""
            previous, ok = "", True
            for ch in wrong[i:i + n]:
                c = d.bare(ch)
                allowed = set()
                for side in (left, after, previous):
                    if len(side) == 1:
                        allowed |= key_neighbours(side)
                if len(c) != 1 or c not in allowed:
                    ok = False
                    break
                previous = c
            if ok:
                return True
        return False
    return check


def regional(right, wrong):
    """The habits of a region: hỏi and ngã mixed up, n and l at the start of a word, t and c at the end of one."""
    if len(right) == len(wrong) and d.bare(right) == d.bare(wrong):
        tr, tw = d.tone_of(d.groups_of(right)), d.tone_of(d.groups_of(wrong))
        if tr and tw and {tr[1], tw[1]} == {HOI, NGA} and d._untoned(d.groups_of(right)) == d._untoned(d.groups_of(wrong)):
            return True
    if len(right) == len(wrong) and len(right) > 1:
        if right[1:] == wrong[1:] and {right[0], wrong[0]} == {"n", "l"}:
            return True
        if right[:-1] == wrong[:-1] and {right[-1], wrong[-1]} == {"t", "c"}:
            return True
    return False


# ----------------------------------------------------------------------------- the generators, one per rule
_CORRUPTOR = d.Corruptor([], seed=0)


def _measured(*kinds):
    """A generator that uses the measured kinds of `t1_data.Corruptor` (one of `kinds`, drawn at random)."""
    def make(word, rng):
        _CORRUPTOR.rng = rng
        made = _CORRUPTOR.make(word.lower(), rng.choice(kinds))
        return d.match_case(word, made) if made and made != word.lower() else None
    return make


def _weighted_neighbour(key, rng):
    pool = sorted(key_neighbours(key))
    heavy = set(USER_PARTNERS.get(key, ("", ""))[1])
    return rng.choices(pool, [2 if k in heavy else 1 for k in pool])[0] if pool else None


def make_neighbour(word, rng):
    spots = [i for i, c in enumerate(word) if c.lower() in TYPABLE]
    if not spots:
        return None
    i = rng.choice(spots)
    new = _weighted_neighbour(word[i].lower(), rng)
    return word[:i] + (new.upper() if word[i].isupper() else new) + word[i + 1:] if new else None


def make_held(word, rng):
    spots = [i for i, c in enumerate(word) if c.isalpha()]
    if not spots:
        return None
    i = rng.choice(spots)
    return word[:i] + word[i] * rng.choice([2, 3, 4]) + word[i + 1:]


def _insert_keys(word, rng, low, high):
    i = rng.randrange(len(word) + 1)
    left = d.bare(word[i - 1]) if i > 0 else ""
    after = d.bare(word[i]) if i < len(word) else ""
    allowed = sorted((key_neighbours(left) if len(left) == 1 else set()) | (key_neighbours(after) if len(after) == 1 else set()))
    if not allowed:
        return None
    block, previous = "", ""
    for _ in range(rng.randint(low, high)):
        pool = sorted(set(allowed) | (key_neighbours(previous) if previous else set()))
        previous = rng.choice(pool)
        block += previous
    return word[:i] + block + word[i:]


def make_slide(word, rng):
    return _insert_keys(word, rng, 2, 3)


def make_multi(word, rng):
    return _insert_keys(word, rng, 1, 2)


def make_swap(word, rng):
    spots = [i for i in range(len(word) - 1) if word[i].isalpha() and word[i + 1].isalpha() and word[i].lower() != word[i + 1].lower()]
    if not spots:
        return None
    i = rng.choice(spots)
    return word[:i] + word[i + 1] + word[i] + word[i + 2:]


def make_regional(word, rng):
    low = _nfc(word)
    options = []
    if len(low) > 2 and low[-1] in "tc":
        options.append(low[:-1] + ("c" if low[-1] == "t" else "t"))
    if len(low) > 1 and low[0] in "nl":
        options.append(("l" if low[0] == "n" else "n") + low[1:])
    tone = d.tone_of(d.groups_of(low))
    if tone and tone[1] in (HOI, NGA):
        options.append(d.with_tone(low, NGA if tone[1] == HOI else HOI))
    if not options:
        return None
    return d.match_case(word, unicodedata.normalize("NFC", rng.choice(options)))


# ----------------------------------------------------------------------------- the rules
Rule = namedtuple("Rule", "id name share source ask edits english_ok keys make check")

RULES = [
    Rule(1, "thiếu dấu thanh", 27, "đo trên lỗi thật (VSEC)",
         "Chọn một từ có dấu thanh (sắc, huyền, hỏi, ngã, nặng) và viết lại từ đó KHÔNG CÒN dấu thanh, giữ nguyên các chữ và dấu mũ, móc khác.",
         (1, 2), False, False, _measured("tone_drop"), kind_is("tone_drop")),
    Rule(2, "sai dấu thanh sang dấu khác", 24, "đo trên lỗi thật (VSEC)",
         "Chọn một từ có dấu thanh và đổi dấu thanh đó thành MỘT DẤU THANH KHÁC (sắc, huyền, hỏi, ngã, nặng), giữ nguyên chữ cái.",
         (1, 2), False, False, _measured("tone"), kind_is("tone")),
    Rule(3, "thiếu một chữ cái", 15, "đo trên lỗi thật (VSEC)",
         "Chọn một từ và BỎ ĐI đúng một chữ cái của nó (chữ nào cũng được, đầu, giữa hay cuối từ), các chữ còn lại giữ nguyên thứ tự và dấu.",
         (1, 2), True, False, _measured("omit"), kind_is("omit")),
    Rule(4, "thiếu dấu mũ, móc hay nét của đ", 10, "đo trên lỗi thật (VSEC)",
         "Chọn một từ có chữ đ, â, ê, ô, ă, ơ hoặc ư (từ chưa có dấu thanh) và viết lại KHÔNG CÒN dấu mũ, móc, nét ngang hay dấu trăng của chữ đó (đ thành d, ê thành e, ư thành u...).",
         (1, 2), False, False, _measured("mark_drop"), kind_is("mark_drop")),
    Rule(5, "thêm dấu thanh vào chữ không có thanh", 6, "đo trên lỗi thật (VSEC)",
         "Chọn một từ KHÔNG có dấu thanh và gắn thêm một dấu thanh (sắc, huyền, hỏi, ngã hoặc nặng) vào nguyên âm của nó.",
         (1, 2), False, False, _measured("tone_add"), kind_is("tone_add")),
    Rule(6, "thừa một chữ lạ hoặc chữ ở phím kề", 5, "đo trên lỗi thật (VSEC)",
         "Chọn một từ và CHÈN THÊM đúng một chữ cái vào nó (ở đầu, giữa hay cuối từ). Chữ thêm không phải là chữ lặp lại chữ bên cạnh. Có thể là một phím nằm sát bên (xem bảng phím sát nhau) hoặc một chữ bất kỳ.",
         (1, 2), False, True, _measured("extra", "insert"), kind_is("insert", "extra")),
    Rule(7, "thêm dấu mũ hoặc móc sai", 3, "đo trên lỗi thật (VSEC)",
         "Chọn một từ có nguyên âm a, o, e, u hoặc d chưa có dấu mũ, móc hay nét và gắn thêm một dấu như vậy (a thành â hay ă, o thành ô hay ơ, e thành ê, u thành ư, d thành đ).",
         (1, 2), False, False, _measured("mark_add"), kind_is("mark_add")),
    Rule(8, "thay chữ bằng chữ ở xa trên bàn phím", 3, "đo trên lỗi thật (VSEC)",
         "Chọn một từ và thay đúng một chữ cái bằng một chữ cái KHÁC Ở XA nó trên bàn phím: một phím KHÔNG nằm trong bảng phím sát nhau của chữ đó.",
         (1, 2), False, True, _measured("substitute"), kind_is("substitute")),
    Rule(9, "đặt dấu thanh sai nguyên âm", 3, "đo trên lỗi thật (VSEC)",
         "Chọn một từ có từ hai nguyên âm trở lên và có dấu thanh, rồi chuyển dấu thanh sang NGUYÊN ÂM KHÁC của cùng từ.",
         (1, 2), False, False, _measured("tone_place"), kind_is("tone_place")),
    Rule(10, "nhầm sang một phím sát bên trên bàn phím", 3, "Bạn (chưa đo)",
         "Chọn một từ và thay đúng MỘT phím (chữ cái bất kỳ trong từ, hoặc cả chữ số) bằng MỘT PHÍM NẰM SÁT BÊN nó trên bàn phím, lấy từ bảng phím sát nhau bên dưới. "
         "Bảng đã gồm cả những phím người gõ hay nhầm theo thói quen riêng. Mọi chữ cái trong từ đều có thể bị nhầm, không riêng chữ nào. Giữ nguyên các chữ khác.",
         (1, 2), True, True, make_neighbour, key_neighbour),
    Rule(11, "giữ phím hay bật nảy: lặp một chữ 2 đến 4 lần", 4, "Mình, từ nhật ký của bạn (chưa đo)",
         "Chọn một từ và LẶP MỘT CHỮ CÁI bất kỳ của nó thêm 1 đến 3 lần liên tiếp (tổng cộng chữ đó xuất hiện 2 đến 4 lần liền nhau); chỉ một chỗ lặp, các chữ khác giữ nguyên.",
         (1, 2), True, False, make_held, held_key),
    Rule(12, "trượt tay: chèn 2 đến 3 phím sát nhau liên tiếp", 2, "Bạn (chưa đo)",
         "Tay bị trượt khi gõ một chữ của từ (đầu, giữa hay cuối): chèn thêm 2 hoặc 3 phím LIỀN NHAU, mỗi phím nằm sát chữ bên cạnh chỗ chèn hoặc sát phím vừa chèn, lấy từ bảng phím sát nhau.",
         (1, 2), False, True, make_slide, inserted_block(2, 3)),
    Rule(13, "một ngón đè cùng lúc 2 đến 3 phím", 2, "Bạn (chưa đo)",
         "Một ngón đè trúng cùng lúc phím đúng và 1 đến 2 phím sát bên: chèn thêm 1 hoặc 2 chữ vào ngay TRƯỚC hoặc SAU một chữ bất kỳ của từ, là phím nằm sát chữ đó theo bảng phím sát nhau.",
         (1, 2), False, True, make_multi, inserted_block(1, 2)),
    Rule(14, "đảo hai chữ liền nhau khi gõ nhanh", 2, "Bạn (VSEC chỉ có 0,2%)",
         "Gõ nhanh nên HAI CHỮ CÁI LIỀN NHAU (ở chỗ nào trong từ cũng được) bị gõ ngược thứ tự: chọn một từ và đảo vị trí hai chữ cái đứng cạnh nhau. Các chữ khác giữ nguyên.",
         (1, 2), True, False, make_swap, kind_is("swap")),
    Rule(15, "nhầm theo thói quen vùng miền: t/c cuối vần, n/l đầu từ, hỏi/ngã", 3, "VSEC (đã lẫn trong các luật 1 đến 9)",
         "Viết nhầm theo thói quen phát âm vùng miền, chỉ MỘT trong ba kiểu: (a) đổi t và c ở cuối vần; (b) đổi n và l ở đầu từ; (c) đổi dấu hỏi và dấu ngã cho nhau. Chọn từ nào áp dụng được cũng được.",
         (1, 2), False, False, make_regional, regional),
]
BY_ID = {r.id: r for r in RULES}

# Varied sentences to draw examples from: everyday talk, work, technical, with names, English words and numbers.
EXAMPLE_SENTENCES = [
    "mình đang train model trên Kaggle", "hôm nay trời đẹp quá, đi chơi nhé", "bạn có thể gửi file cho mình không",
    "tối nay mình rảnh, đi ăn gì đó nhé", "đây là file của mình", "mỗi lần mình thử đều lỗi", "việc này làm nhanh nên nhớ nhé",
    "mình nghĩ là nên thử lại", "cuối tuần này cả nhà về quê thăm ông bà", "em muốn đặt một phòng đôi cho hai đêm",
    "báo cáo tháng này cần nộp trước thứ sáu", "cái máy giặt nhà mình bị kêu to lắm", "anh Nam hẹn gặp ở quán cà phê lúc tám giờ",
    "bạn nào biết sửa lỗi này thì chỉ giúp mình với", "hệ thống báo lỗi kết nối khi gửi dữ liệu lên server",
    "chị Lan mới chuyển sang làm việc ở Hà Nội", "tuần sau lớp mình đi dã ngoại ở Đà Lạt", "hôm qua mưa to nên trận bóng bị hoãn",
    "mình vừa cài lại Windows, máy chạy nhanh hơn nhiều", "tiền điện tháng này tăng gần hai trăm nghìn", "nhớ mang theo giấy tờ khi đi làm thủ tục nhé",
    "cô giáo dặn cả lớp ôn kỹ phần lịch sử trước kỳ thi", "mình thử query json bằng script mới mà vẫn chưa ra kết quả",
    "ngày mai họp nhóm lúc chín giờ, đừng đến muộn nhé", "quán phở đầu ngõ ngon và giá rất hợp lý", "bạn đã commit code lên branch chính chưa",
    "dạo này giá xăng tăng nên mình đi xe buýt nhiều hơn", "tôi muốn hỏi về thủ tục đăng ký tạm trú", "cảm ơn bạn đã giúp mình hoàn thành bài tập",
    "mùa này ở miền Trung hay có bão và mưa lớn",
]


def make_slip(rule, word, rng):
    """A typed version of `word` by `rule`, checked, or None if the rule does not apply to this word."""
    if not any(c.isalpha() for c in word):
        return None
    made = rule.make(word, rng)
    if not made or made == word:
        return None
    made = unicodedata.normalize("NFC", made)
    return made if made.isalnum() and rule.check(_nfc(word), _nfc(made)) else None


def make_example(sentence, rule, rng):
    """A typed version of `sentence` by `rule` with its edits (index, word, typed word), or None."""
    tokens = d.split_tokens(sentence)
    words = [i for i, t in enumerate(tokens) if t.isalpha() and len(t) >= 2]
    rng.shuffle(words)
    wanted = rng.randint(*rule.edits)
    edits = []
    for i in words:
        wrong = make_slip(rule, tokens[i], rng)
        if wrong and wrong.lower() not in {t.lower() for t in tokens if t != tokens[i]}:
            edits.append((i, tokens[i], wrong))
        if len(edits) == wanted:
            break
    if not edits:
        return None
    typed = sentence
    for _, word, wrong in sorted(edits):
        typed = re.sub(rf"(?<!\w){re.escape(word)}(?!\w)", wrong, typed, count=1)
    return {"typed": typed, "edits": [{"i": i, "from": w, "to": t} for i, w, t in sorted(edits)]}


def rule_examples(rule, rng, count=3, avoid=""):
    """`count` sentences from the pool, each with two typed versions made by the rule's generator (checked)."""
    pool = [s for s in EXAMPLE_SENTENCES if s != avoid]
    rng.shuffle(pool)
    out = []
    for sentence in pool:
        versions = [make_example(sentence, rule, rng) for _ in range(6)]
        versions = [v for v in versions if v]
        unique = list({v["typed"]: v for v in versions}.values())[:2]
        if unique:
            out.append((sentence, unique))
        if len(out) == count:
            break
    return out


def make_prompt(clean, rule, versions=3, rng=None):
    """Chat messages asking for `versions` typed versions of one correct sentence, by ONE rule."""
    rng = rng or random.Random(zlib.crc32((clean + str(rule.id)).encode("utf-8")))
    low, high = rule.edits
    shots = []
    for sentence, answers in rule_examples(rule, rng, 3, avoid=clean):
        shots.append(f"Câu đúng: {sentence}\n" + "\n".join(json.dumps(a, ensure_ascii=False) for a in answers))
    english = ("Từ tiếng Anh và tên riêng được phép đổi theo kiểu lỗi này." if rule.english_ok
               else "Giữ NGUYÊN từ tiếng Anh, tên riêng, số và dấu câu.")
    keyboard = ""
    if rule.keys:
        keyboard = (
            "\nBỐ CỤC BÀN PHÍM (QWERTY): hàng số 1234567890; hàng trên qwertyuiop; hàng giữa asdfghjkl; hàng dưới zxcvbnm; mỗi hàng lệch nhẹ so với hàng trên nó. "
            "Hai phím là SÁT NHAU khi cùng hàng và liền nhau, hoặc ở hàng ngay trên hay ngay dưới và lệch nhau chưa tới một phím.\n"
            "BẢNG PHÍM SÁT NHAU của các chữ trong câu này (đã tính sẵn, gồm cả thói quen riêng của người gõ; chỉ dùng phím trong bảng khi luật cần phím sát bên):\n"
            f"{neighbour_table(clean)}\n"
        )
    user = (
        f"Nhiệm vụ: từ một câu ĐÚNG, viết {versions} phiên bản khác nhau như thể người gõ bị lỗi. Mỗi phiên bản có từ {low} đến {high} từ "
        "bị gõ sai THEO ĐÚNG MỘT KIỂU LỖI dưới đây, và KHÔNG làm gì khác.\n\n"
        f"KIỂU LỖI: {rule.name}.\n"
        f"CÁCH LÀM: {rule.ask}\n"
        f"{keyboard}\n"
        "Quy tắc chung:\n"
        "- Áp dụng cho BẤT KỲ từ nào trong câu mà kiểu lỗi làm được, không chỉ những từ giống ví dụ; chọn từ khác nhau giữa các phiên bản.\n"
        "- Chỉ đổi những từ ghi trong edits; mọi từ khác, dấu câu và khoảng trắng giữ NGUYÊN. Số từ của câu không đổi.\n"
        "- Vị trí i bắt đầu từ 0 và đếm cả dấu câu như một mục riêng (dấu phẩy là một mục).\n"
        f"- {english}\n"
        "- Không dùng viết tắt hay chữ lóng (ko, dc, k...). Không đổi cả từ thành từ khác nghĩa.\n"
        "- Mỗi phiên bản một dòng JSON, không giải thích: "
        '{"typed": "câu sau khi gõ sai", "edits": [{"i": vị trí, "from": "từ đúng", "to": "từ gõ sai"}]}\n\n'
        "Ví dụ cho kiểu lỗi này (các câu và từ khác nhau mỗi lần; đừng chép lại):\n" + "\n\n".join(shots) + f"\n\nCâu đúng: {clean}\n"
    )
    return [
        {"role": "system", "content": "Bạn là công cụ tạo dữ liệu kiểm thử gõ sai tiếng Việt. Chỉ trả về các dòng JSON được yêu cầu."},
        {"role": "user", "content": user},
    ]


def parse_versions(reply):
    """JSON objects, one per line, from the LLM's reply (anything that is not one is ignored)."""
    out = []
    for line in reply.split("\n"):
        line = line.strip().strip("`").strip()
        if not line.startswith("{"):
            continue
        try:
            obj = json.loads(line)
        except ValueError:
            continue
        if isinstance(obj, dict) and isinstance(obj.get("typed"), str) and isinstance(obj.get("edits"), list):
            out.append(obj)
    return out


def check(clean, version, rule):
    """The checked record {"clean", "typed", "rule", "edits": [{"i", "from", "to"}]}, or None if the version cannot be
    verified as mistakes of this rule. Positions count tokens as `t1_data.split_tokens` does (punctuation is a token)."""
    clean_tokens = d.split_tokens(clean)
    typed_tokens = d.split_tokens(version["typed"])
    if len(clean_tokens) != len(typed_tokens):
        return None
    changed = [i for i, (a, b) in enumerate(zip(clean_tokens, typed_tokens)) if a != b]
    listed = []
    for e in version["edits"]:
        try:
            listed.append((int(e["i"]), str(e["from"]), str(e["to"])))
        except (KeyError, TypeError, ValueError):
            return None
    low, high = rule.edits
    if not low <= len(changed) <= high or sorted(i for i, *_ in listed) != changed:
        return None
    edits = []
    for i, source, typed in listed:
        right, wrong = clean_tokens[i], typed_tokens[i]
        if unicodedata.normalize("NFC", source) != right or unicodedata.normalize("NFC", typed) != wrong:
            return None
        low_right, low_wrong = _nfc(right), _nfc(wrong)
        others = {t.lower() for t in clean_tokens if t != right}
        if not (right.isalpha() and wrong.isalnum()) or low_wrong in TEENCODE or low_wrong in others:
            return None
        if d.classify_op(low_wrong, low_right) == "tone_convention" or not rule.check(low_right, low_wrong):
            return None
        edits.append({"i": i, "from": right, "to": wrong})
    return {"clean": clean, "typed": version["typed"], "rule": rule.id, "edits": edits}


def choose_rules(rng, count):
    """`count` different rules, drawn by their share of the mistakes (a rule with no share gets a small one)."""
    pool = list(RULES)
    weights = [r.share or 3 for r in pool]
    out = []
    for _ in range(min(count, len(pool))):
        pick = rng.choices(range(len(pool)), weights)[0]
        out.append(pool.pop(pick))
        weights.pop(pick)
    return out


def markdown():
    lines = []
    for r in RULES:
        rng = random.Random(r.id)
        lines.append(f"### Luật {r.id}. {r.name}")
        lines.append(f"- Tỉ lệ gần đúng: {r.share or 'đã lẫn trong luật khác'}{'%' if r.share else ''}. Nguồn: {r.source}.")
        lines.append(f"- **Cách làm đưa cho LLM:** {r.ask}")
        lines.append(f"- Số từ đổi mỗi phiên bản: {r.edits[0]} đến {r.edits[1]}. Từ tiếng Anh: {'được phép đổi' if r.english_ok else 'giữ nguyên'}. "
                     f"Bảng phím sát nhau trong prompt: {'có' if r.keys else 'không'}.")
        lines.append("- Hai ví dụ do bộ sinh của luật tạo (mỗi lần gọi là các câu và từ khác nhau):")
        for sentence, answers in rule_examples(r, rng, 2):
            lines.append("  - `" + sentence + "` → `" + answers[0]["typed"] + "` (" + ", ".join(f"{e['from']} → {e['to']}" for e in answers[0]["edits"]) + ")")
        lines.append("")
    return "\n".join(lines)


def generate(args):
    import torch
    from transformers import AutoModelForCausalLM, AutoTokenizer

    sentences = [l.strip() for l in open(args.sentences, encoding="utf-8") if l.strip()][args.shard::args.shards]
    rng = random.Random(args.seed)
    rng.shuffle(sentences)
    tok = AutoTokenizer.from_pretrained(args.model, padding_side="left")
    kwargs = {"torch_dtype": torch.float16}
    if args.load_4bit:
        from transformers import BitsAndBytesConfig
        kwargs = {"quantization_config": BitsAndBytesConfig(load_in_4bit=True, bnb_4bit_compute_dtype=torch.float16, bnb_4bit_quant_type="nf4")}
    model = AutoModelForCausalLM.from_pretrained(args.model, device_map={"": 0}, **kwargs).eval()
    jobs = [(s, r) for s in sentences for r in choose_rules(rng, args.rules_per_sentence)]
    stats = {r.id: [0, 0] for r in RULES}                       # rule -> [proposed, kept]
    started, done = time.time(), 0
    with open(args.out, "w", encoding="utf-8") as out:
        for at in range(0, len(jobs), args.batch):
            if args.minutes and (time.time() - started) / 60 > args.minutes:
                break
            batch = jobs[at:at + args.batch]
            texts = [tok.apply_chat_template(make_prompt(s, r, args.versions, random.Random(rng.random())), tokenize=False, add_generation_prompt=True)
                     for s, r in batch]
            enc = tok(texts, return_tensors="pt", padding=True).to(model.device)
            with torch.no_grad():
                gen = model.generate(**enc, max_new_tokens=args.max_new_tokens, do_sample=True, temperature=args.temperature, top_p=0.95,
                                     pad_token_id=tok.pad_token_id or tok.eos_token_id)
            for (sentence, rule), reply in zip(batch, tok.batch_decode(gen[:, enc["input_ids"].shape[1]:], skip_special_tokens=True)):
                for version in parse_versions(reply):
                    stats[rule.id][0] += 1
                    record = check(sentence, version, rule)
                    if record:
                        stats[rule.id][1] += 1
                        out.write(json.dumps(record, ensure_ascii=False) + "\n")
            done += len(batch)
            out.flush()
            if (done // args.batch) % 10 == 0:
                print(f"  {done} requests; kept/proposed by rule: " + " ".join(f"{k}:{v[1]}/{v[0]}" for k, v in stats.items()), flush=True)
    print("done", {k: f"{v[1]}/{v[0]}" for k, v in stats.items()}, f"{(time.time() - started) / 60:.1f} min", flush=True)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--show", type=int, help="print the prompt of this rule for one sentence, and stop")
    ap.add_argument("--markdown", action="store_true", help="print every rule with its instruction and examples, and stop")
    ap.add_argument("--sentences")
    ap.add_argument("--out")
    ap.add_argument("--model", default="Qwen/Qwen2.5-3B-Instruct")
    ap.add_argument("--load-4bit", action="store_true")
    ap.add_argument("--shard", type=int, default=0)
    ap.add_argument("--shards", type=int, default=1)
    ap.add_argument("--rules-per-sentence", type=int, default=2)
    ap.add_argument("--versions", type=int, default=3)
    ap.add_argument("--batch", type=int, default=16)
    ap.add_argument("--max-new-tokens", type=int, default=420)
    ap.add_argument("--temperature", type=float, default=0.9)
    ap.add_argument("--minutes", type=float, default=0)
    ap.add_argument("--seed", type=int, default=7)
    args = ap.parse_args()
    if args.markdown:
        print(markdown())
        return
    if args.show:
        for m in make_prompt("Hôm nay mình thử query json trên server của team", BY_ID[args.show]):
            print(f"[{m['role']}]\n{m['content']}\n")
        return
    generate(args)


if __name__ == "__main__":
    main()
