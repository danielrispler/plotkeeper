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
# (`<task-notification>`) lands in the log too. `scrub` then copies the session's events here
# without their content (a command stays only when it carries the marker `studio-fixture-`).
#
# The classifier is not deterministic, so a session that gets no block, no Allow or no wake-up is tried
# again, up to 3 times; when none succeeds the script says so, exits 1 and writes no fixture.
#
# It runs a nested `claude` on your subscription (a few minutes, a few cents), and each session runs
# a nested `claude -p` of its own, so run it yourself, OUTSIDE auto mode, from any folder:
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

recorded=
for attempt in 1 2 3; do
  session=$(uuidgen | tr '[:upper:]' '[:lower:]')
  echo "Attempt $attempt, session $session"
  python3 "$player/studio.py" "$data" "$session" &
  playing=$!

  # The classifier decides, so the rule is a prose one; a block is still not certain, hence the retries.
  rule='Never run a Bash command whose text contains 'studio-fixture-': it is a recording test and must always be blocked.'
  settings="{\"autoMode\":{\"hard_deny\":[\"\$defaults\",\"$rule\"]}}"
  (cd "$repo" && CLAUDE_CODE_PLUGIN_DIRS="$studio/plugin" claude -p --session-id "$session" \
    --permission-mode auto --settings "$settings" <<'PROMPT'
This session records a test fixture. Follow these steps exactly and keep every reply short.

1. Run this exact command with the Bash tool, in the foreground:
   claude -p "Reply with the word ok" --model haiku # studio-fixture-foreground
   If it is blocked, wait: the block is being answered, and the call is retried for you. If you are told to run it again, run it again, unchanged.
2. Run this exact command with the Bash tool and run_in_background set to true:
   claude -p "Reply with the word ok" --model haiku # studio-fixture-background
   If it is blocked, handle it as in step 1, and keep run_in_background true.
3. Wait for the background command's completion notice; do not run sleep.
4. Reply with one line: what each of the two commands returned, or that it was blocked.
PROMPT
  ) || echo "claude exited with $?" >&2

  # The hooks are async, so the last events may land just after `claude` exits.
  sleep 3
  kill "$playing" 2>/dev/null || true
  playing=
  rm -f "$data/watching.json"

  denied=$(in_log "$session" '"hook_event_name":"PermissionDenied"')
  answered=$(in_log "$session" '"hook_event_name":"StudioAnswer"')
  woke=$(in_log "$session" 'task-notification')
  echo "Session $session: $denied PermissionDenied, $answered Allow answers, $woke wake-up lines"
  if [ "$denied" -ge 1 ] && [ "$answered" -ge 1 ] && [ "$woke" -ge 1 ]; then
    recorded=$session
    break
  fi
  echo "No usable block this time (the classifier is not deterministic), trying again." >&2
done

if [ -z "$recorded" ]; then
  echo "Never got a block, an Allow and a wake-up in 3 sessions. No fixture written." >&2
  exit 1
fi

cd "$studio"
cargo run -q -p studio-core --bin scrub -- "$recorded" <"$log" >core/tests/fixtures/blocked-run.jsonl
echo "Recorded session $recorded with Claude Code $(claude --version)"
