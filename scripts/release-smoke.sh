#!/usr/bin/env bash
# Release-candidate test using ONLY freshly generated temporary SQLite data.
set -euo pipefail
if (($# > 1)); then
    echo "Usage: scripts/release-smoke.sh [PATH-TO-POS-BINARY]" >&2
    exit 2
fi
repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
binary="${1:-$repo_root/target/release/pos}"
if [[ "$binary" != /* ]]; then
    binary="$(realpath -e -- "$binary")"
fi
[[ -x "$binary" ]] || {
    echo "Release smoke: executable not found: $binary" >&2
    exit 1
}
[[ "$("$binary" --version)" == "POS 0.1.0" ]] || {
    echo "Release smoke: expected POS 0.1.0; got $("$binary" --version)" >&2
    exit 1
}
umask 077
workdir="$(mktemp -d)"
trap 'rm -rf -- "$workdir"' EXIT
export HOME="$workdir/home"
export XDG_DATA_HOME="$workdir/xdg"
unset POS_DB_PATH
mkdir -p -- "$HOME"
db="$XDG_DATA_HOME/pos/pos.sqlite3"
[[ ! -e "$db" ]] || { echo "Release smoke: unexpected database" >&2; exit 1; }
[[ "$("$binary" tasks list --json)" == "[]" ]] || {
    echo "Release smoke: first-run database should be empty" >&2
    exit 1
}
[[ -f "$db" ]] || { echo "Release smoke: first run did not create DB" >&2; exit 1; }
"$binary" data check >/dev/null
"$binary" projects add "Release Test" >/dev/null
"$binary" tasks add "Recover me" --project release-test \
    --priority urgent --status in-progress \
    --description "Important release smoke data" --due 2026-12-17 >/dev/null
"$binary" tasks list --search "smoke data" --priority urgent \
    --status in-progress --project release-test --json |
    python3 -c 'import json,sys; d=json.load(sys.stdin); assert len(d)==1 and d[0]["project_id"]==1'
"$binary" tasks "done" 1 >/dev/null
"$binary" tasks list --status completed --json |
    python3 -c 'import json,sys; d=json.load(sys.stdin); assert len(d)==1 and d[0]["status"]=="completed"'
"$binary" tasks reopen 1 >/dev/null
before="$workdir/before.json"
backup="$workdir/backup.sqlite3"
recovered="$workdir/recovered.sqlite3"
after="$workdir/after.json"
"$binary" data export --output "$before" 2>/dev/null
python3 - "$before" <<'PY'
import json
import sys
with open(sys.argv[1], encoding="utf-8") as handle:
    data = json.load(handle)
assert data["format_version"] == 2
assert len(data["tasks"]) == len(data["projects"]) == 1
assert data["tasks"][0]["priority"] == "urgent"
assert data["tasks"][0]["status"] == "todo"
PY
"$binary" data backup "$backup" >/dev/null
"$binary" tasks add "Newer task" >/dev/null
"$binary" data restore "$backup" --output "$recovered" >/dev/null
"$binary" --db "$recovered" data check >/dev/null
"$binary" --db "$recovered" data export --output "$after" 2>/dev/null
cmp -s "$before" "$after" || {
    echo "Release smoke: restored data differs from backup" >&2
    exit 1
}
"$binary" tasks list --all --json |
    python3 -c 'import json,sys; assert len(json.load(sys.stdin))==2'
if "$binary" data backup "$backup" >/dev/null 2>&1; then
    echo "Release smoke: backup overwrote an existing file" >&2
    exit 1
fi
if "$binary" data restore "$backup" --output "$recovered" >/dev/null 2>&1; then
    echo "Release smoke: restore overwrote an existing file" >&2
    exit 1
fi
printf 'not-sqlite' > "$workdir/corrupted.sqlite3"
if "$binary" data restore "$workdir/corrupted.sqlite3" \
    --output "$workdir/invalid-output.sqlite3" >/dev/null 2>&1; then
    echo "Release smoke: corrupt backup accepted" >&2
    exit 1
fi
[[ ! -e "$workdir/invalid-output.sqlite3" ]] || {
    echo "Release smoke: invalid destination was published" >&2
    exit 1
}
# Exercise the actual interactive terminal in a PTY, verify the dynamic
# version label, and require terminal restoration after normal exit.
python3 - "$binary" <<'PYPTY'
import fcntl
import os
import pty
import select
import struct
import subprocess
import sys
import termios
import time

master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 28, 110, 0, 0))
environment = {**os.environ, "TERM": "xterm-256color"}
process = subprocess.Popen(
    [sys.argv[1]], stdin=slave, stdout=slave, stderr=slave,
    env=environment, start_new_session=True,
)
os.close(slave)
output = bytearray()

def drain(seconds):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        ready, _, _ = select.select([master], [], [], max(0, deadline - time.monotonic()))
        if ready:
            try:
                data = os.read(master, 65536)
            except OSError:
                break
            if data:
                output.extend(data)

try:
    drain(0.4)
    os.write(master, b"q")
    drain(0.2)
    process.wait(timeout=5)
finally:
    if process.poll() is None:
        process.kill()
        process.wait(timeout=5)
    os.close(master)
assert process.returncode == 0, f"TUI exited with {process.returncode}"
assert b"v0.1.0" in output, "TUI did not render the release version"
assert b"\x1b[?1049l" in output, "TUI did not restore the terminal screen"
PYPTY

printf '%s release smoke: PASS (first run, Tasks, Projects, recovery, TUI)\n' \
    "$("$binary" --version)"
