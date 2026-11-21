#!/usr/bin/env python3
"""Decode input_event payloads from an strace -xx -e write -y log of
uvie-inputd. Each write() to a /dev/uinput fd carries 24-byte
InputEventRaw records; print EV_KEY entries as code/value lines."""
import re
import struct
import sys

path = sys.argv[1]
out = []
for line in open(path, errors="replace"):
    if "write(" not in line:
        continue
    # strace -xx hex-escapes the fd path too; match 24-byte-multiple writes
    m = re.search(r'write\(\d+<[^>]*>, "((?:\\x[0-9a-fA-F]{2}|\\.|[^"])*)", (\d+)\)', line)
    if not m:
        continue
    raw = m.group(1).encode().decode("unicode_escape").encode("latin-1")
    recs = [struct.unpack("qqHHi", raw[o:o + 24])
            for o in range(0, len(raw) - len(raw) % 24, 24)]
    # Text writes that happen to be 24-byte multiples decode as garbage;
    # real input_events have small type codes and a sane epoch.
    if not recs or not all(1_500_000_000 < r[0] < 2_000_000_000 and r[2] <= 4 for r in recs):
        continue
    for sec, usec, etype, code, val in recs:
        if etype == 0x01:
            out.append(f"{sec}.{usec:06d} EV_KEY code={code} value={val}")
        elif etype != 0:
            out.append(f"{sec}.{usec:06d} type={etype} code={code} value={val}")
print("\n".join(out))
