#!/bin/sh
# Records scripts/snap/terminal-recording.txt (danielrispler/plotkeeper#150): the raw terminal output of a short scripted Claude
# Code session in a temporary repository, which `npm run snap -- recordedTerminal <out.png>` replays into the window's terminal.
# Record it again when Claude Code's look changes. It runs a nested `claude` on your subscription (one turn), so run it yourself,
# outside auto mode, from the repository's root, in a terminal of your own:
#
#     scripts/snap/record-terminal.sh
#
# It records at 110 x 32 (the size scripts/snap/fixtures.mjs replays it at), takes the folder-trust question by answering it,
# and stops once the turn is done and the screen has been still for a while. Before it writes the file it searches the recording
# for your login name, your home folder, and your git name and email; it writes nothing when one is there.
# Search the file yourself too, for anything the script cannot know, before committing it.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
out="$here/terminal-recording.txt"
repo=$(mktemp -d)
raw=$(mktemp)
trap 'rm -rf "$repo" "$raw"' EXIT

git -C "$repo" init -q
git -C "$repo" config user.name "Demo"
git -C "$repo" config user.email "demo@example.com"
echo "# Fixture" >"$repo/README.md"
git -C "$repo" add README.md
git -C "$repo" commit -qm "Start"
cd "$repo"

PROMPT='Run `git log --oneline` and tell me in two short lines what this repository holds.' RAW="$raw" python3 -I - <<'PY'
import fcntl, os, pty, select, struct, subprocess, termios, time

master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 32, 110, 0, 0))
env = {**os.environ, "TERM": "xterm-256color", "COLUMNS": "110", "LINES": "32"}
child = subprocess.Popen(
    ["claude", "--permission-mode", "acceptEdits", "--allowedTools", "Bash(git *)", os.environ["PROMPT"]],
    stdin=slave, stdout=slave, stderr=slave, env=env, close_fds=True, start_new_session=True,
)
os.close(slave)
data, trusted, start, last = b"", False, time.time(), time.time()
while time.time() - start < 240 and child.poll() is None:
    ready, _, _ = select.select([master], [], [], 0.5)
    if ready:
        try:
            chunk = os.read(master, 65536)
        except OSError:
            break
        data += chunk
        last = time.time()
        # The folder-trust question of a new folder: its first option is "No, exit", so move down to "Yes" before Enter.
        if not trusted and b"Enter" in data and b"trust this folder" in data:
            trusted = True
            time.sleep(1)
            os.write(master, b"\x1b[B")
            time.sleep(0.5)
            os.write(master, b"\r")
    # The turn is done when the answer is on the screen and nothing has moved for 20 s.
    elif time.time() - last > 20 and time.time() - start > 30:
        break
child.terminate()
open(os.environ["RAW"], "wb").write(data)
PY

# Nothing personal: refuse a recording that holds the login name, the home folder, the git name or email.
for needle in "$(id -un)" "$HOME" "$(git config --global user.name || true)" "$(git config --global user.email || true)"; do
  if [ -n "$needle" ] && grep -qF -- "$needle" "$raw"; then
    echo "The recording holds \"$needle\"; nothing was written." >&2
    exit 1
  fi
done
[ -s "$raw" ] || { echo "The recording is empty; nothing was written." >&2; exit 1; }
cp "$raw" "$out"
echo "Recorded $(wc -c <"$out") bytes with Claude Code $(claude --version) into $out"
