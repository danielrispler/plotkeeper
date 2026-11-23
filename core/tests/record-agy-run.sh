#!/bin/sh
# Records fixtures/agy/ (danielrispler/plotkeeper#47): short `agy -p` runs in a temporary repository,
# with probe hooks in `.agents/hooks.json` that append each hook payload to a log, as the studio's
# plugin would deliver it to `plotkeeper-hook`. The runs are a turn with tools, an edit and a check, a
# second prompt, and a subagent that reports back after its parent's turn ended (a wake-up). The
# script scrubs the hook lines and the conversations' transcripts and `subagents/` files into
# `core/tests/fixtures/agy/`, and keeps the agy version in `version.txt`. It runs agy's model on
# your account, so run it yourself, outside auto mode, from any folder:
#
#     core/tests/record-agy-run.sh
#
# It needs `expect` (to trust the temporary folder, which agy asks about once) and `python3`. Set
# SCRATCH to a folder to record in, else a temporary one is used. It never installs an agy plugin
# and never reads agy's credentials: only the conversations' own `brain/<id>` folders.
set -eu

studio=$(cd "$(dirname "$0")/../.." && pwd)
out="$studio/core/tests/fixtures/agy"
brain="$HOME/.gemini/antigravity-cli/brain"
repo=${SCRATCH:-$(mktemp -d)}
mkdir -p "$repo"
repo=$(cd "$repo" && pwd -P)
log="$repo/events.log"
trap '[ -n "${SCRATCH:-}" ] || rm -rf "$repo"' EXIT

git -C "$repo" init -q
echo "# Fixture" >"$repo/README.md"
git -C "$repo" add README.md
git -C "$repo" -c user.name=fixture -c user.email=fixture@example.com commit -qm "Start"
mkdir -p "$repo/.agents"

# One handler for the six events: it appends `<Unix ms> <payload on one line>` and exits 0 with no
# output, so it can never block agy. `SessionStart`, `PreInvocation`, `PostInvocation` and `Stop`
# take their handlers flat; the tool events wrap them in a `matcher` group.
python3 -I - "$repo" "$log" <<'PY'
import json, sys
repo, log = sys.argv[1:3]
command = (
    "p=$(cat); (printf '%s %s\\n' \"$(perl -MTime::HiRes=time -e 'printf \"%d\",time*1000')\""
    " \"$(printf %s \"$p\" | tr -d '\\n')\" >> '" + log + "' 2>/dev/null &) ; exit 0"
)
handler = {"type": "command", "command": command, "timeout": 5}
hooks = {event: [handler] for event in ["SessionStart", "PreInvocation", "PostInvocation", "Stop"]}
for event in ["PreToolUse", "PostToolUse"]:
    hooks[event] = [{"matcher": "*", "hooks": [handler]}]
json.dump({"studio-probe": hooks}, open(repo + "/.agents/hooks.json", "w"), indent=1)
PY

# agy asks once whether to trust a folder, and loads a folder's hooks only after that.
trust=$(mktemp)
cat >"$trust" <<'EXPECT'
set timeout 60
spawn agy
expect "trust this folder"
send "\r"
sleep 3
send "\003"
sleep 1
send "\003"
expect eof
EXPECT
(cd "$repo" && expect "$trust" >/dev/null 2>&1) || true
rm -f "$trust"

cd "$repo"
PLOTKEEPER_TERMINAL_ID=fixture-terminal agy --mode auto -p "Create hello.txt holding the line hello with a file tool, then run 'cat hello.txt' with the shell. Reply in one short sentence." >/dev/null
conversation=$(python3 -I -c '
import json, sys
print(json.loads(open(sys.argv[1]).readline().split(" ", 1)[1])["conversationId"])' "$log")
PLOTKEEPER_TERMINAL_ID=fixture-terminal agy --mode auto --conversation "$conversation" -p="Reply with the word ok." >/dev/null
# `-p` waits for the subagent, whose report wakes the conversation up after the turn ended.
PLOTKEEPER_TERMINAL_ID=fixture-terminal agy --mode auto --conversation "$conversation" -p="Start one subagent in the background to run 'sleep 8; echo done' and report back. After starting it, end your turn right away with one short sentence, without waiting for it." >/dev/null
# The hooks are backgrounded, so the last events may land just after agy exits.
sleep 3

version=$(agy --version)
rm -rf "$out"
mkdir -p "$out"
python3 -I - "$log" "$out" "$brain" "$repo" "$HOME" "$version" <<'PY'
import json, os, re, sys

log, out, brain, repo, home, version = sys.argv[1:7]
placeholder = "@BRAIN@"

def clean(text):
    # No home folder, user name or scratch folder stays; the brain folder becomes a placeholder
    # the tests point at the fixture's own copy.
    text = text.replace(brain, placeholder)
    text = text.replace(repo, "/tmp/agy-fixture")
    text = text.replace(home, "/home/user")
    return text.replace(os.path.basename(home), "user")

conversations = []
lines = []
for raw in open(log):
    at, payload = raw.rstrip("\n").split(" ", 1)
    event = json.loads(payload)
    event.pop("artifactDirectoryPath", None)
    event["studio_received_ms"] = int(at)
    event["studio_terminal_id"] = "fixture-terminal"
    if event["conversationId"] not in conversations:
        conversations.append(event["conversationId"])
    lines.append(clean(json.dumps(event, sort_keys=True)))
open(out + "/run.jsonl", "w").write("\n".join(lines) + "\n")
open(out + "/version.txt", "w").write(version.strip() + "\n")

for conversation in conversations:
    source = f"{brain}/{conversation}/.system_generated"
    target = f"{out}/brain/{conversation}/.system_generated"
    os.makedirs(target + "/logs", exist_ok=True)
    kept = []
    for raw in open(source + "/logs/transcript_full.jsonl"):
        record = json.loads(raw)
        record.pop("thinking", None)
        kept.append(clean(json.dumps(record, sort_keys=True)))
    open(target + "/logs/transcript_full.jsonl", "w").write("\n".join(kept) + "\n")
    if os.path.isdir(source + "/subagents"):
        os.makedirs(target + "/subagents", exist_ok=True)
        for name in os.listdir(source + "/subagents"):
            text = open(f"{source}/subagents/{name}").read()
            open(f"{target}/subagents/{name}", "w").write(clean(text))
PY

echo "Recorded $(wc -l <"$out/run.jsonl") hook lines with agy $version into core/tests/fixtures/agy"
