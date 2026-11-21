#!/usr/bin/env python3
"""Reconstruct typed text from a decoded EV_KEY stream (decode_strace.py
output) and compare it with the expected string.

Rules: key press (value=1/2) only; Backspace pops; Ctrl+Shift+U starts a
hex sequence ended by Enter (-> the code point); other keys map through a
QWERTY evdev table. Usage: check_typed.py events.log 'expected text'"""
import re
import sys

KEYMAP = {
    **{c: ch for c, ch in zip(range(16, 26), "qwertyuiop")},
    **{c: ch for c, ch in zip(range(30, 39), "asdfghjkl")},
    **{c: ch for c, ch in zip(range(44, 51), "zxcvbnm")},
    **{c: d for c, d in zip(range(2, 12), "1234567890")},
    57: " ",
    15: "\t",
    28: "\n",
}
CTRL = {29, 97}
SHIFT = {42, 54}
HEXCHARS = set("0123456789abcdef")


def reconstruct(path):
    screen = []
    ctrl = shift = False
    hexbuf = None
    for line in open(path, errors="replace"):
        m = re.search(r"EV_KEY code=(\d+) value=(\d+)", line)
        if not m:
            continue
        code, value = int(m.group(1)), int(m.group(2))
        if code in CTRL:
            ctrl = value != 0
            continue
        if code in SHIFT:
            shift = value != 0
            continue
        if value == 0:
            continue  # releases don't type
        if hexbuf is not None:
            if code == 28:  # Enter ends the sequence
                screen.append(chr(int(hexbuf, 16)))
                hexbuf = None
            elif code in KEYMAP and KEYMAP[code] in HEXCHARS:
                hexbuf += KEYMAP[code]
            else:
                hexbuf = None
            continue
        if ctrl and shift and code == 22:  # Ctrl+Shift+U
            hexbuf = ""
            continue
        if code == 14:
            if screen:
                screen.pop()
            continue
        if code in KEYMAP:
            screen.append(KEYMAP[code])
    return "".join(screen)


if __name__ == "__main__":
    got = reconstruct(sys.argv[1])
    want = sys.argv[2]
    print(f"reconstructed: {got!r}")
    print(f"expected:      {want!r}")
    sys.exit(0 if got == want else 1)
