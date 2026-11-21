#!/usr/bin/env python3
"""mknod /dev/input/eventN for every event handler in
/proc/bus/input/devices (containers have uinput but no udev).
Optionally watch: pass --watch to rescan until interrupted."""
import os
import sys
import time


def sync_nodes():
    made = []
    os.makedirs("/dev/input", exist_ok=True)
    data = open("/proc/bus/input/devices").read()
    for line in data.splitlines():
        if not line.startswith("H: Handlers="):
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
