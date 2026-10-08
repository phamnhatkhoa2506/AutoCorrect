"""The physical keyboard: which keys touch which (RESEARCH.md, section 5; AUGMENT_PROMPT.md).

A typing slip is a finger landing on the wrong key, so the model of slips starts from the layout of the device.
The layout is a table of the writer's own keyboard, a Dell Vostro 3405: a US (ANSI) body with island keys, measured on a
photo of the keyboard (2026-10-08, accurate to about 0.1 key width). Each key has the start and end of its footprint in
key widths (x) and in rows (y; the arrow keys are half a row high). Two keys are neighbours when their footprints touch
side by side, or one above the other, and overlap by at least `MIN_OVERLAP` of a key. Wide keys come out right by
themselves: Space covers c v b n m and touches Alt on each side; Left Shift touches z, a and Caps Lock.

What the photo shows and this table keeps: Enter horizontal (ANSI); Backspace 1.5 wide and \\ 1 wide, so every main row is 14.5
wide; Caps Lock 1.65 (A is only 0.15 to the right of Q); no right Windows key; a small right Ctrl; and, below Right Shift, the
arrow keys in two half-height rows (Pg Up, Up, Pg Dn above; Left, Down, Right below). The function row (Esc, F1 to F12, Insert,
Delete) is not modelled: a slip from the number row into it types nothing in a word.

Keys are named by what they type (`a`, `1`, `;`) or by their name (`space`, `lshift`, `caps`, `tab`, `enter`, `backspace`,
`lctrl`, `fn`, `win`, `lalt`, `ralt`, `rctrl`, `pgup`, `up`, `pgdn`, `left`, `down`, `right`). The table is exported as JSON for
the Rust typing simulator:

    python keyboard.py --export layouts/dell_vostro_3405.json

Another body (ISO, a desktop keyboard) is a change of this table: the rest of the code only asks `adjacent`.
"""

import argparse
import json

MIN_OVERLAP = 0.15            # key widths that two keys must share along their common edge to touch
EPS = 1e-9

# What the writer says about particular keys, on top of the geometry: key -> (keys also taken for it, of which heavier).
# Only `t` so far, as an example of the table; more keys can be added here (or measured, AUGMENT_RULES.md, C).
USER_PARTNERS = {"t": ("rygeufh", "eufh")}


def _row(*keys):
    """[(name, width)] from names with an optional width: "a" or ("tab", 1.5)."""
    return [(k, 1.0) if isinstance(k, str) else k for k in keys]


ROWS = [
    _row(*"`1234567890-=", ("backspace", 1.5)),
    _row(("tab", 1.5), *"qwertyuiop[]", "\\"),
    _row(("caps", 1.65), *"asdfghjkl;'", ("enter", 1.85)),
    _row(("lshift", 2.15), *"zxcvbnm,./", ("rshift", 2.35)),
    _row(("lctrl", 1.15), "fn", "win", "lalt", ("space", 5.1), "ralt", "rctrl"),
]
ARROW_WIDTH = (14.5 - 11.25) / 3
ARROWS = [("pgup", 0.0, 0.5), ("up", 0.0, 0.5), ("pgdn", 0.0, 0.5), ("left", 0.5, 1.0), ("down", 0.5, 1.0), ("right", 0.5, 1.0)]

MODIFIERS = {"backspace", "tab", "caps", "enter", "lshift", "rshift", "lctrl", "fn", "win", "lalt", "space", "ralt", "rctrl",
             "pgup", "up", "pgdn", "left", "down", "right"}


def layout():
    """{key: (y0, y1, x0, x1)}"""
    out = {}
    for r, row in enumerate(ROWS):
        x = 0.0
        for name, width in row:
            out[name] = (float(r), r + 1.0, x, x + width)
            x += width
    x0 = sum(w for _, w in ROWS[4])
    for i, (name, top, bottom) in enumerate(ARROWS):
        col = i % 3
        out[name] = (4.0 + top, 4.0 + bottom, x0 + col * ARROW_WIDTH, x0 + (col + 1) * ARROW_WIDTH)
    return out


LAYOUT = layout()


def kind(key):
    """`char` for a key that types one character, `modifier` for the others."""
    return "modifier" if key in MODIFIERS else "char"


def char_keys():
    return [k for k in LAYOUT if kind(k) == "char"]


def _overlap(a0, a1, b0, b1):
    return min(a1, b1) - max(a0, b0)


def adjacent(key):
    """Every key whose footprint touches the footprint of `key` (modifiers and punctuation included)."""
    if key not in LAYOUT:
        return set()
    y0, y1, x0, x1 = LAYOUT[key]
    out = set()
    for other, (v0, v1, w0, w1) in LAYOUT.items():
        if other == key:
            continue
        beside = abs(w0 - x1) < EPS or abs(w1 - x0) < EPS
        stacked = abs(v0 - y1) < EPS or abs(v1 - y0) < EPS
        if beside and _overlap(y0, y1, v0, v1) >= MIN_OVERLAP - EPS:
            out.add(other)
        elif stacked and _overlap(x0, x1, w0, w1) >= MIN_OVERLAP - EPS:
            out.add(other)
    return out


def adjacent_characters(key):
    """The neighbours that type a letter or a digit: what a slip inside a word can put there without splitting it."""
    return {k for k in adjacent(key) if len(k) == 1 and k.isalnum() and k.isascii()}


def export(path):
    rows = [{"name": k, "y0": y0, "y1": y1, "x0": round(x0, 4), "x1": round(x1, 4), "kind": kind(k)} for k, (y0, y1, x0, x1) in LAYOUT.items()]
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        habits = {k: {"partners": p, "heavy": h} for k, (p, h) in USER_PARTNERS.items()}
        json.dump({"name": "Dell Vostro 3405 (US, ANSI, island keys)", "min_overlap": MIN_OVERLAP, "habits": habits, "keys": rows}, f, indent=1)
        f.write("\n")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--export", help="write the layout as JSON for the Rust simulator")
    ap.add_argument("--show", nargs="*", help="print the neighbours of these keys")
    args = ap.parse_args()
    if args.export:
        export(args.export)
        print("written to", args.export)
    for key in args.show or []:
        print(f"{key}: {' '.join(sorted(adjacent(key)))}")


if __name__ == "__main__":
    main()
