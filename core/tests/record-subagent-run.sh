#!/bin/sh
# Records fixtures/subagent-run.jsonl (#370): a short scripted Claude Code session in a temporary
# repository, with the studio plugin loaded, that spawns a worktree worker and a read-only helper in
# the background and stops the helper mid-step. Its events land in the event log, and `scrub` copies
# them here without their content. Quit the studio app first, or it takes the session as live and
# asks for cards. It runs a nested `claude` on your subscription, so run it yourself, outside auto
# mode, from any folder:
#
#     apps/studio/core/tests/record-subagent-run.sh
set -eu

studio=$(cd "$(dirname "$0")/../.." && pwd)
log="$HOME/Library/Application Support/dev.falinks.studio/events.jsonl"
session=$(uuidgen | tr '[:upper:]' '[:lower:]')
repo=$(mktemp -d)
trap 'rm -rf "$repo"' EXIT

# The plugin runs the studio-hook built in this checkout.
cargo build -q --release --manifest-path "$studio/Cargo.toml" -p studio-hook
git -C "$repo" init -q
echo "# Fixture" >"$repo/README.md"
git -C "$repo" add README.md
git -C "$repo" commit -qm "Start"

# `-p` waits for background subagents before it exits, so their returns reach the session.
cd "$repo"
CLAUDE_CODE_PLUGIN_DIRS="$studio/plugin" claude -p --session-id "$session" \
  --permission-mode acceptEdits --allowedTools "Bash(git *)" "Bash(sleep *)" <<'PROMPT'
This session records a test fixture. Follow these steps exactly and keep every reply short.

1. In one message, start two subagents with the Agent tool, both in the background:
   - a worker: description "Add hello file", subagent_type "general-purpose", isolation "worktree". Its task: create hello.txt holding the line "hello", commit it with `git add hello.txt && git commit -m "Add hello"`, then reply "committed".
   - a helper: description "Wait and report", subagent_type "general-purpose", no isolation. Its task: run `sleep 100` with Bash, then reply "done". It must not change any file.
2. Run `sleep 15` with Bash, so the helper's own step has started, then stop the helper with the TaskStop tool.
3. Wait for the worker to finish, then reply with one line on what each subagent did.
PROMPT

# The hooks are async, so the last events may land just after `claude` exits.
sleep 2
cd "$studio"
cargo run -q -p studio-core --bin scrub -- "$session" <"$log" >core/tests/fixtures/subagent-run.jsonl
echo "Recorded session $session with Claude Code $(claude --version)"
