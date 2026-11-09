#!/usr/bin/env bash
# Every real model call round 5 needs (#517), in one run. Daniel runs it outside auto mode, from any
# checkout: `apps/studio/real-calls.sh`. Each call's output goes to one folder in `local-evidence/`;
# a step whose code is not built yet is skipped and says so. The calls use Daniel's subscription.
set -uo pipefail

studio=$(cd "$(dirname "$0")" && pwd)
main=$(cd "$(git -C "$studio" rev-parse --path-format=absolute --git-common-dir)/.." && pwd)
out="$main/local-evidence/2026-11-09-round-5/real-calls-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$out"
cd "$studio"
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

# #518: 5 timed cards in safe mode, beside #471's median 20.1 s and slowest 24.4 s.
step "#518 writer timing" 518-writer-timing.json cargo run -q -p studio-understanding --example writer_timing -- --runs 5
# #518: real cards, explain more and a brief; check their text carries no plugin footer.
real_card=0
step "#518 real card" 518-real-card.txt cargo run -q -p studio-understanding --example real_card || real_card=$?
# #526: turn summaries on 3 real turns, for Daniel to judge.
if example summary_trial; then
  step "#526 summaries" 526-summaries.md cargo run -q -p studio-understanding --example summary_trial -- --turns 3
else
  echo "== #526 summaries: skipped, summary_trial is not built yet"
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
