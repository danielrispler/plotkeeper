#!/bin/sh
# Records fixtures/blocked-run.jsonl (danielrispler/plotkeeper#174, spec #170 seam 4): a short session in
# auto mode, in a temporary repository and with this checkout's plugin, that tries to run a nested
# `claude -p` twice, once in the foreground and once with `run_in_background: true`. Auto mode is
# made to block both: the session gets an inline `--settings` with an `autoMode.hard_deny` rule that
# names the marker `studio-fixture-` in a command (docs: "Configure auto mode", "Where the classifier
# reads configuration": `--settings` is a scope the classifier reads, and `hard_deny` blocks "even when
# the user asks for them"; project settings files are not read). This script plays the studio: it keeps a fresh heartbeat (`watching.json`) for the
# session and, when the plugin's `PermissionDenied` hook leaves a request under `blocked/<session>/`,
# grants a one-time pass and writes the Allow answer, so the hook returns `retry: true`, the next
# `PreToolUse` claims the pass and the retry runs. The background retry's wake-up
# (`<task-notification>`, whose summary carries the exit code) lands in the log too. `scrub` then copies the session's events here
# without their content (a command stays only when it carries the marker `studio-fixture-`).
#
# The session is interactive (an `ask` has no prompt to go to under `claude -p`, so a pass would never
# run): run the script straight in a terminal, not with its output redirected, watch the agent, and
# type /exit once it has replied its last line (not Ctrl-C). The rule makes the block reliable, so there is one attempt;
# a session with no PermissionDenied, no Allow answer or no wake-up makes the script say which is
# missing, exit 1 and write no fixture.
#
# It runs a nested `claude` on your subscription (a few minutes, a few cents), and the session runs
# nested `claude -p` calls of its own, so run it yourself, OUTSIDE auto mode, from any folder:
#
#     core/tests/record-blocked-run.sh
#
# Quit the studio app first: it writes the same heartbeat and would answer the cards itself.
set -eu

studio=$(cd "$(dirname "$0")/../.." && pwd)
data="$HOME/Library/Application Support/dev.falinks.studio"
log="$data/events.jsonl"
repo=$(mktemp -d)
player=$(mktemp -d)
playing=
cleanup() {
  [ -z "$playing" ] || kill "$playing" 2>/dev/null || true
  rm -rf "$repo" "$player"
}
trap cleanup EXIT

if [ -f "$data/watching.json" ] && python3 - "$data/watching.json" <<'PY'
import json, os, sys, time
w = json.load(open(sys.argv[1]))
fresh = time.time() * 1000 - w.get("heartbeat_ms", 0) < 5000
try:
    os.kill(int(w.get("pid", 0)), 0)
    alive = int(w.get("pid", 0)) > 0
except Exception:
    alive = False
sys.exit(0 if fresh and alive else 1)
PY
then
  echo "The studio app is running. Quit it and run this again." >&2
  exit 1
fi

# The plugin runs the hooks built in this checkout.
cargo build -q --release --manifest-path "$studio/Cargo.toml" -p studio-hook
git -C "$repo" init -q
echo "# Fixture" >"$repo/README.md"
git -C "$repo" add README.md
git -C "$repo" commit -qm "Start"

# The studio's part: heartbeat, and Allow for every request the hook leaves.
cat >"$player/studio.py" <<'PY'
import json, os, sys, time, uuid

data, session = sys.argv[1], sys.argv[2]
blocked = os.path.join(data, "blocked", session)
passes = os.path.join(data, "passes", session)


def now():
    return int(time.time() * 1000)


def write(path, value):
    os.makedirs(os.path.dirname(path), mode=0o700, exist_ok=True)
    temp = path + ".tmp"
    fd = os.open(temp, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w") as f:
        f.write(json.dumps(value))
    os.rename(temp, path)


answered = set()
while True:
    write(os.path.join(data, "watching.json"),
          {"session_id": session, "pid": os.getpid(), "heartbeat_ms": now()})
    if os.path.isdir(blocked):
        for name in sorted(os.listdir(blocked)):
            if not name.endswith(".json") or name.endswith(".answer.json") or name in answered:
                continue
            try:
                request = json.load(open(os.path.join(blocked, name)))
            except Exception:
                continue
            answered.add(name)
            command, cwd = request["tool_input"]["command"], request["cwd"]
            write(os.path.join(passes, uuid.uuid4().hex + ".json"),
                  {"session_id": session, "command": command, "cwd": cwd,
                   "expires_ms": now() + 30 * 60 * 1000})
            write(os.path.join(blocked, request["nonce"] + ".answer.json"),
                  {"nonce": request["nonce"], "decision": "allow"})
            print("studio: allowed the blocked command %r in %s" % (command, cwd), file=sys.stderr, flush=True)
    time.sleep(0.2)
PY

in_log() { grep -F "\"session_id\":\"$1\"" "$log" 2>/dev/null | grep -cF "$2" || true; }

cat >"$player/prompt.txt" <<'PROMPT'
This session records a test fixture. Follow these steps exactly and keep every reply short.

1. Run this exact command with the Bash tool, in the foreground:
   claude -p "Reply with the word ok" --model haiku # studio-fixture-foreground
   If it is blocked, wait: the block is being answered, and the call is retried for you. If you are told to run it again, run it again, unchanged.
2. Run this exact command with the Bash tool and run_in_background set to true:
   claude -p "Reply with the word ok" --model haiku # studio-fixture-background
   If it is blocked, handle it as in step 1, and keep run_in_background true.
3. Wait for the background command's completion notice before you reply; do not run sleep.
4. Reply with one line: what each of the two commands returned, or that it was blocked. Then stop.
PROMPT
prompt=$(cat "$player/prompt.txt")
# The rule goes inside a JSON string, so it holds no double quotes.
rule="Never run a Bash command whose text contains 'studio-fixture-': it is a recording test and must always be blocked."
settings="{\"autoMode\":{\"hard_deny\":[\"\$defaults\",\"$rule\"]}}"

session=$(uuidgen | tr '[:upper:]' '[:lower:]')
python3 "$player/studio.py" "$data" "$session" &
playing=$!

echo "Session $session starts now, interactively. Watch it: when the agent has replied its last line,"
echo "type /exit. (Allowing the blocked commands is done for you.)"
# An `ask` has no prompt to go to under `claude -p`, so the session must be interactive.
# Ctrl-C reaches the script as well as `claude`: ignore it here, so the scrub below still runs.
trap '' INT
(cd "$repo" && CLAUDE_CODE_PLUGIN_DIRS="$studio/plugin" claude --session-id "$session" \
  --permission-mode auto --settings "$settings" "$prompt") || echo "claude exited with $?" >&2

# The hooks are async, so the last events may land just after `claude` exits.
trap - INT
sleep 3
kill "$playing" 2>/dev/null || true
playing=
rm -f "$data/watching.json"

denied=$(in_log "$session" '"hook_event_name":"PermissionDenied"')
answered=$(in_log "$session" '"hook_event_name":"StudioAnswer"')
requested=$(in_log "$session" '"hook_event_name":"PermissionRequest"')
woke=$(in_log "$session" 'task-notification')
echo "Session $session: $denied PermissionDenied, $answered Allow answers, $requested PermissionRequest, $woke wake-up lines"
[ "$denied" -ge 1 ] || echo "Missing: no PermissionDenied, so auto mode never blocked a command." >&2
[ "$answered" -ge 1 ] || echo "Missing: no StudioAnswer, so the fake studio's Allow never reached the hook." >&2
[ "$woke" -ge 1 ] || echo "Missing: no task-notification, so the background command's wake-up never came (did you /exit too early?)." >&2
if [ "$denied" -lt 1 ] || [ "$answered" -lt 1 ] || [ "$woke" -lt 1 ]; then
  echo "No fixture written." >&2
  exit 1
fi
recorded=$session

cd "$studio"
cargo run -q -p studio-core --bin scrub -- "$recorded" <"$log" >core/tests/fixtures/blocked-run.jsonl
echo "Recorded session $recorded with Claude Code $(claude --version)"
