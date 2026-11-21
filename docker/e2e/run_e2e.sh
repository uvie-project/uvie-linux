#!/usr/bin/env bash
# Headless e2e smoke for uvie-inputd inside a privileged container.
# Creates a uinput test keyboard, lets the daemon grab it, feeds Telex
# keystrokes, and verifies the daemon emits "chào việt được " on its
# virtual keyboard.
#
# Environment quirks handled here:
# - No udev: event nodes are mknod'd by mknodes.py.
# - This VM's kernel only allows opening evdev minor 64 (event0); every
#   higher minor returns EPERM, so a second reader cannot attach to the
#   daemon's vdev node. We strace the daemon instead and decode the raw
#   input_event structs it writes to /dev/uinput.
# - Each word ends with SPACE: without the separator the engine treats
#   the letters as one word and restores raw keystrokes.
set -u
cd /work

OUTDIR=/work/docker/e2e/out
FEED_FIFO=/tmp/uvie-feed.fifo
OUT=$OUTDIR/events.log
TRACE=$OUTDIR/trace.log
DAEMON_LOG=$OUTDIR/daemon.log
EXPECTED=$(printf 'ch\xc3\xa0o vi\xe1\xbb\x87t \xc4\x91\xc6\xb0\xe1\xbb\xa3c ')

rm -rf "$OUTDIR"
mkdir -p "$OUTDIR"
# Safety: the daemon grabs every keyboard it can open and this script traces
# what it emits. Refuse to run where real input devices are visible (a
# privileged container on a native Linux host), so physical keystrokes are
# never grabbed or logged.
if grep -q 'Handlers=.*event' /proc/bus/input/devices; then
    echo "refusing to run: the kernel already lists input devices (real keyboards may be visible)" >&2
    exit 2
fi

rm -f "$FEED_FIFO"
mkfifo "$FEED_FIFO"

# No udev in the container: mknod event nodes as devices appear.
python3 docker/e2e/mknodes.py --watch > "$OUTDIR/mknodes.log" 2>&1 &
MKNODES=$!

# Feeder keeps the uinput fd open; fd 3 keeps the fifo writer side open.
python3 docker/e2e/kbd_feeder.py < "$FEED_FIFO" 2>"$OUTDIR/feeder.log" &
FEEDER=$!
exec 3>"$FEED_FIFO"
sleep 1.5
echo "== test keyboard =="
grep -A2 'UVie Test Keyboard' /proc/bus/input/devices || echo "TEST KEYBOARD MISSING"
ls -l /dev/input/ || true

echo "== starting daemon (under strace) =="
strace -f -y -e trace=write -s 512 -xx -o "$TRACE" \
    /cargo-target/release/uvie-inputd >"$DAEMON_LOG" 2>&1 &
DAEMON=$!
for i in $(seq 1 20); do
    grep -q 'uvie virtual keyboard' /proc/bus/input/devices && break
    sleep 0.5
done
grep -B2 -A6 'uvie virtual keyboard' /proc/bus/input/devices || echo "VDEV MISSING"
sleep 1

feed() {
    local word=$1
    for ((i = 0; i < ${#word}; i++)); do
        printf '%s ' "${word:i:1}" >&3
        sleep 0.08
    done
    printf 'SPACE ' >&3
    sleep 0.3
}

echo "== feeding chaof SPACE / vieetj SPACE / dduwowcj SPACE =="
feed chaof
feed vieetj
feed dduwowcj
sleep 2

kill $DAEMON $FEEDER $MKNODES 2>/dev/null
exec 3>&-

echo "== daemon log =="
cat "$DAEMON_LOG"
python3 docker/e2e/decode_strace.py "$TRACE" > "$OUT"
echo "== vdev events ($OUT) =="
cat "$OUT"
echo "== verification =="
python3 docker/e2e/check_typed.py "$OUT" "$EXPECTED"
