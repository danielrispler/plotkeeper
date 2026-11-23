#!/usr/bin/env bash
# Every step round 5 needs (danielrispler/Falinks#517) that cannot run in auto mode, in one run: real model calls, and
# steps that launch and kill an app build. Daniel runs it outside auto mode, from any checkout:
# `real-calls.sh`. Each step's output goes to one folder in `local-evidence/`, with the
# script's own terminal output in `run.log`. The card steps and the summary trial are retired (danielrispler/plotkeeper#348).
set -uo pipefail

studio=$(cd "$(dirname "$0")" && pwd)
main=$(cd "$(git -C "$studio" rev-parse --path-format=absolute --git-common-dir)/.." && pwd)
out="$main/local-evidence/2026-11-09-round-5/real-calls-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$out"
exec > >(tee "$out/run.log") 2>&1
cd "$studio" || exit 1
echo "commit $(git rev-parse --short HEAD), output $out"

# One step: its name, its file, then the command. A failure is logged and the next step runs.
step() {
  local name=$1 file=$2
  shift 2
  echo "== $name"
  local status=0
  "$@" >"$out/$file" 2>"$out/$file.log" || status=$?
  if [ "$status" -eq 0 ]; then echo "   ok: $file"; else echo "   failed: see $file.log"; fi
  return "$status"
}

# danielrispler/Falinks#543: can macOS kill the terminal daemon when the app quits, is force quit or crashes? A separate
# build, `StudioRepro`, with its own identifier, home, socket and data folder, so the live app,
# daemon and terminals are never touched. A crash may show macOS's "quit unexpectedly" dialog.
repro_app="$studio/target/release/bundle/macos/StudioRepro.app"
build_repro() {
  { [ -d node_modules ] || npm ci; } &&
    npx tauri build --config '{"identifier":"dev.falinks.studio-repro","productName":"StudioRepro"}'
}
if step "danielrispler/Falinks#543 repro build" 543-build.txt build_repro; then
  step "danielrispler/Falinks#543 daemon kill" 543-daemon-kill.json cargo run -q -p studio-terminal-daemon --example kill_repro -- --app "$repro_app" --out "$out"
else
  echo "== danielrispler/Falinks#543 daemon kill: skipped, the repro build failed"
fi
