#!/usr/bin/env bash
# Every step round 5 needs (danielrispler/Falinks#517) that cannot run in auto mode, in one run: real model calls, and
# steps that launch and kill an app build. Daniel runs it outside auto mode, from any checkout:
# `real-calls.sh`. Each step's output goes to one folder in `local-evidence/`, with the
# script's own terminal output in `run.log`; a step whose code is not built yet is skipped and says
# so. The model calls use Daniel's subscription.
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
example() { [ -f "understanding/examples/$1.rs" ]; }

# danielrispler/Falinks#518: 5 timed cards in safe mode, beside danielrispler/Falinks#471's median 20.1 s and slowest 24.4 s.
step "danielrispler/Falinks#518 writer timing" 518-writer-timing.json cargo run -q -p studio-understanding --example writer_timing -- --runs 5
# danielrispler/Falinks#518: real cards, explain more and a brief; check their text carries no plugin footer.
real_card=0
step "danielrispler/Falinks#518 real card" 518-real-card.txt cargo run -q -p studio-understanding --example real_card || real_card=$?
# danielrispler/Falinks#526: turn summaries on 3 real turns, for Daniel to judge.
if example summary_trial; then
  step "danielrispler/Falinks#526 summaries" 526-summaries.md cargo run -q -p studio-understanding --example summary_trial -- --turns 3
else
  echo "== danielrispler/Falinks#526 summaries: skipped, summary_trial is not built yet"
fi

# Lines a plugin footer would start with, so a footer in a card stands out. A failed step leaves an
# empty or partial file, so the check runs only on a finished one.
if [ "$real_card" -ne 0 ]; then
  echo "== footer check: could not run, the real card step failed"
elif grep -n -i -E "what i (did not|didn't) check|^not checked|ponytail|caveman" "$out"/518-real-card.txt; then
  echo "   footer found in 518-real-card.txt"
else
  echo "== no footer in 518-real-card.txt"
fi

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
