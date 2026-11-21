#!/usr/bin/env python3
"""Create a uinput test keyboard that satisfies is_keyboard() and feed it
key tokens read from stdin (one whitespace-separated token per key press).

Tokens: a-z, 0-9, SPACE, ENTER, BS, TAB, ESC, LEFTSHIFT, LEFTCTRL, LEFTALT,
+shift / -shift etc. (+X holds the key, -X releases it)."""
import fcntl
import os
import struct
import sys

UINPUT = "/dev/uinput"
UI_SET_EVBIT = 0x40045564
UI_SET_KEYBIT = 0x40045565
UI_DEV_SETUP = 0x40585503
UI_DEV_CREATE = 0x5501
UI_DEV_DESTROY = 0x5502
EV_KEY = 0x01
EV_SYN = 0x00
BUS_USB = 0x03

KEYCODES = {
    "ESC": 1,
    **{str(d): c for d, c in zip("1234567890", range(2, 12))},
    "BS": 14, "TAB": 15,
    **{ch: c for ch, c in zip("qwertyuiop", range(16, 26))},
    "ENTER": 28, "LEFTCTRL": 29,
    **{ch: c for ch, c in zip("asdfghjkl", range(30, 39))},
    "LEFTSHIFT": 42,
    **{ch: c for ch, c in zip("zxcvbnm", range(44, 51))},
    "LEFTALT": 56, "SPACE": 57,
}
ALIAS = {"shift": "LEFTSHIFT", "ctrl": "LEFTCTRL", "alt": "LEFTALT"}


def emit(fd, code, value):
    ev = struct.pack("qqHHi", 0, 0, EV_KEY, code, value)
    syn = struct.pack("qqHHi", 0, 0, EV_SYN, 0, 0)
    os.write(fd, ev + syn)


def main():
    fd = os.open(UINPUT, os.O_WRONLY | os.O_NONBLOCK)
    fcntl.ioctl(fd, UI_SET_EVBIT, EV_KEY)
    for code in range(0, 256):
        fcntl.ioctl(fd, UI_SET_KEYBIT, code)
    # Legacy uinput_user_dev path: name[80] + input_id + ff_effects_max +
    # absmax/absmin/absfuzz/absflat[ABS_MAX=64].
    user_dev = (
        b"UVie Test Keyboard".ljust(80, b"\0")
        + struct.pack("HHHH", BUS_USB, 0x1234, 0x5678, 1)
        + struct.pack("i", 0)
        + b"\0" * (64 * 4 * 4)
    )
    os.write(fd, user_dev)
    fcntl.ioctl(fd, UI_DEV_CREATE)
    sys.stderr.write("[feeder] UVie Test Keyboard created\n")
    sys.stderr.flush()

    try:
        while True:
            chunk = os.read(0, 4096)
            if not chunk:
                break
            for tok in chunk.decode(errors="replace").split():
                hold = tok.startswith("+")
                rel = tok.startswith("-")
                raw = tok.lstrip("+-")
                name = ALIAS.get(raw.lower(), raw.upper())
                code = KEYCODES.get(name, KEYCODES.get(raw.lower()))
                if code is None:
                    sys.stderr.write(f"[feeder] unknown token {tok}\n")
                    continue
                if hold:
                    emit(fd, code, 1)
                elif rel:
                    emit(fd, code, 0)
                else:
                    emit(fd, code, 1)
                    emit(fd, code, 0)
    finally:
        fcntl.ioctl(fd, UI_DEV_DESTROY)
        os.close(fd)


if __name__ == "__main__":
    main()
