#!/bin/sh
# Proves that Claude Code's real events have the shape spec #202's tests assume (#214, seam 5): a
# short scripted Claude Code session in a temporary repository, with this checkout's plugin, spawns
# one background subagent that is not isolated. From the session's folder the subagent makes a
# worktree with `git worktree add`, writes a 42-line file in it through Bash and commits, as the
# workers of #186's diagnosis do. The script then reads the session's events in the event log and the
# offers record, and prints pass or fail for each of its two checks with the line that decided it:
#
#   - events: the subagent's events carry the worktree in `studio_touched_roots`, some event carries
#     a start tree and a later one an end tree for it, and the diff of those trees holds the edit;
#   - offers: `offers.jsonl` gained a line for that worker with a card offered.
#
# Studio must be open on the build under test: the app, not the hook, writes the offers record, so
# with Studio closed or on a build from before #202 the offers check fails. The session lands in the
# event log like any other, so it adds one throwaway session to your list in Studio. Nothing is copied
# into the repository: a recorded run cannot be a fixture here, since `scrub` replaces every command's
# text. It runs a nested `claude` on your subscription, so run it yourself, outside auto mode, from
# any folder, and keep its output as the run's evidence:
#
#     core/tests/run-unisolated-worker.sh | tee run.txt
#
# It removes its temporary repository, and with it the worktree inside it, when it ends.
set -eu

studio=$(cd "$(dirname "$0")/../.." && pwd)
data="$HOME/Library/Application Support/dev.falinks.studio"
session=$(uuidgen | tr '[:upper:]' '[:lower:]')
# The real path, as `git worktree list` gives it: `mktemp` answers under the `/var` link.
repo=$(cd "$(mktemp -d)" && pwd -P)
worktree="$repo/.claude/worktrees/issue-1-shapes"
trap 'git -C "$repo" worktree remove --force "$worktree" 2>/dev/null || true; rm -rf "$repo"' EXIT

# The plugin runs the studio-hook built in this checkout.
cargo build -q --release --manifest-path "$studio/Cargo.toml" -p studio-hook
git -C "$repo" init -q
echo "# Fixture" >"$repo/README.md"
echo ".claude/" >"$repo/.gitignore"
git -C "$repo" add README.md .gitignore
git -C "$repo" commit -qm "Start"

# `-p` waits for background subagents before it exits, so the worker's events all land. Without
# `Edit` and `Write` the worker edits through Bash, as the workers that are not isolated do. The
# prompt is not quoted, so it names `$worktree`; it holds no other `$` or backtick unescaped.
cd "$repo"
CLAUDE_CODE_PLUGIN_DIRS="$studio/plugin" claude -p --session-id "$session" \
  --allowedTools "Bash" --disallowedTools "Edit" "Write" "NotebookEdit" <<PROMPT
This session is a scripted test. Follow these steps exactly and keep every reply short.

1. Start one subagent with the Agent tool: description "Add shapes module", subagent_type "general-purpose", run_in_background true, and no isolation (leave the isolation parameter out). Give it this task, word for word:

   Work only through the Bash tool and never change folder in a command of its own. Run these steps in order:
   a. git worktree add -b issue-1-shapes $worktree
   b. One Bash command that starts with \`cd $worktree &&\` and writes the file shapes.py with a heredoc (\`cat > shapes.py <<'EOF'\` ... \`EOF\`), holding exactly these lines:
      import math


      class Shape:
          def area(self):
              raise NotImplementedError

          def perimeter(self):
              raise NotImplementedError


      class Circle(Shape):
          def __init__(self, radius):
              if radius < 0:
                  raise ValueError("radius must not be negative")
              self.radius = radius

          def area(self):
              return math.pi * self.radius ** 2

          def perimeter(self):
              return 2 * math.pi * self.radius


      class Rectangle(Shape):
          def __init__(self, width, height):
              if width < 0 or height < 0:
                  raise ValueError("sides must not be negative")
              self.width = width
              self.height = height

          def area(self):
              return self.width * self.height

          def perimeter(self):
              return 2 * (self.width + self.height)


      def largest(shapes):
          if not shapes:
              return None
          return max(shapes, key=lambda shape: shape.area())
   c. cd $worktree && git add shapes.py && git commit -m "Add shapes"
   d. Reply "committed".
2. Wait for the subagent's completion notice; do not run sleep. Then reply with one line on what it did.
PROMPT

# The hooks are async, so the last events may land just after `claude` exits.
sleep 3
echo "Session $session with Claude Code $(claude --version)"
cd "$studio"
python3 - "$data/events.jsonl" "$data/offers.jsonl" "$session" "$repo" "$worktree" <<'PY'
import json, subprocess, sys, time

log, offers, session, repo, worktree = sys.argv[1:]
WAIT = 300


def shown(event):
    keys = ("hook_event_name", "agent_id", "tool_name", "studio_touched_roots",
            "studio_turn_start", "studio_turn_end")
    return json.dumps({k: event[k] for k in keys if k in event})


events = []
with open(log) as f:
    for line in f:
        if session not in line:
            continue
        try:
            event = json.loads(line)
        except ValueError:
            continue
        if event.get("session_id") == session:
            events.append(event)


def events_check():
    """The worker's agent id, and why the check failed or None."""
    touched = [e for e in events
               if e.get("agent_id") and worktree in e.get("studio_touched_roots", [])]
    if not touched:
        mine = [e for e in events if e.get("agent_id")]
        last = shown(mine[-1]) if mine else "no subagent event at all"
        return None, f"no subagent event carries {worktree} in studio_touched_roots; the subagent's last: {last}"
    agent = touched[0]["agent_id"]
    print(f"events: subagent {agent} touches the worktree: {shown(touched[0])}")
    starts = [i for i, e in enumerate(events) if worktree in e.get("studio_turn_start", {})]
    if not starts:
        return agent, f"no event carries a start tree for {worktree}"
    start = events[starts[0]]
    ends = [e for e in events[starts[0] + 1:] if worktree in e.get("studio_turn_end", {})]
    if not ends:
        return agent, f"no event after the start tree carries an end tree for {worktree}: {shown(start)}"
    end = ends[-1]
    print(f"events: start tree: {shown(start)}")
    print(f"events: end tree: {shown(end)}")
    start_tree, end_tree = start["studio_turn_start"][worktree], end["studio_turn_end"][worktree]
    diff = subprocess.run(["git", "-C", repo, "diff", "--numstat", start_tree, end_tree],
                          capture_output=True, text=True).stdout
    row = next((r for r in diff.splitlines() if r.endswith("\tshapes.py")), None)
    if row is None or int(row.split("\t")[0]) < 30:
        return agent, f"the diff of {start_tree[:12]}..{end_tree[:12]} lacks the 30-line edit of shapes.py: {diff.strip() or 'empty'}"
    print(f"events: the diff of the trees holds the edit: {row}")
    return agent, None


def offers_check(agent):
    """Why the check failed, or None."""
    def lines():
        try:
            text = open(offers).read()
        except OSError:
            return []
        found = []
        for line in text.splitlines():
            try:
                record = json.loads(line)
            except ValueError:
                continue
            record_id = str(record.get("id", ""))
            if (record.get("session") == session and record.get("kind") == "worker"
                    and (agent is None or record_id.startswith(agent + "@"))):
                found.append((line, record))
        return found

    print(f"offers: waiting up to {WAIT} s for the worker's line in {offers}")
    deadline = time.time() + WAIT
    while True:
        found = lines()
        card = next((line for line, r in found if r.get("offer") == "card"), None)
        if card:
            print(f"offers: the worker's card: {card}")
            return None
        if time.time() > deadline:
            if found:
                return f"the worker's lines offer no card: {' | '.join(line for line, _ in found)}"
            return "no line for the worker; is Studio open on a build with #202?"
        time.sleep(5)


agent, events_failed = events_check()
offers_failed = offers_check(agent)
for name, failed in (("events", events_failed), ("offers", offers_failed)):
    print(f"{name}: {'FAIL: ' + failed if failed else 'pass'}")
sys.exit(1 if events_failed or offers_failed else 0)
PY
