#!/usr/bin/env python3
"""mknod /dev/input/eventN for the e2e test devices listed in
/proc/bus/input/devices (containers have uinput but no udev). Only the
feeder keyboard and the daemon's virtual keyboard get a node, so a real
keyboard is never exposed to the daemon under test.
Optionally watch: pass --watch to rescan until interrupted."""
import os
import sys
import time


TEST_DEVICES = {"UVie Test Keyboard", "uvie virtual keyboard"}


def sync_nodes():
    made = []
    os.makedirs("/dev/input", exist_ok=True)
    data = open("/proc/bus/input/devices").read()
    name = ""
    for line in data.splitlines():
        if line.startswith("N: Name="):
            name = line.split("=", 1)[1].strip('"')
        if not line.startswith("H: Handlers=") or name not in TEST_DEVICES:
            continue
        for h in line.split("=", 1)[1].split():
            if not h.startswith("event"):
                continue
            path = "/dev/input/" + h
            n = int(h[len("event"):])
            if not os.path.exists(path):
                os.mknod(path, 0o660 | 0o20000, os.makedev(13, 64 + n))
                made.append(path)
    return made


if __name__ == "__main__":
    if "--watch" in sys.argv:
        while True:
            for p in sync_nodes():
                print("[mknodes] created", p, flush=True)
            time.sleep(0.5)
    else:
        for p in sync_nodes():
            print("[mknodes] created", p)
