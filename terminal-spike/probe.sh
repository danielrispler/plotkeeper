#!/bin/sh
# Spike for #322: scripted survival check with plain shells, no claude. Crashes, rebuilds and quits the app, and
# shows after each step whether every backend's terminals still run and whether the app reattached.
# Run from a normal terminal: ./probe.sh. It opens the app window a few times.
set -u
cd "$(dirname "$0")"
BIN=target/debug/studio-terminal-spike
SOCK=$HOME/.studio-terminal-spike/daemon.sock
T="tmux -L studio-spike"
COUNTER='i=0; while :; do i=$((i+1)); echo tick $i; sleep 1; done'

ask() { (printf '%s\n' "$1"; sleep 1) | nc -U "$SOCK"; }
launch() { SPIKE_AUTOSPAWN=daemon,tmux,herdr "$BIN" >> app.log 2>&1 & APP=$!; sleep 5; }
state() {
  echo "== $1 (app $(kill -0 "${APP:-0}" 2>/dev/null && echo up || echo down))"
  echo "  daemon pid $(pgrep -f 'studio-terminal-spike --daemon' | tr '\n' ' ')$(ask '{"op":"list"}' | jq -c '[.terms[] | {id, fg: .foreground, exited, bytes: .scrollback_bytes}]')"
  echo "  daemon attaches so far: $(grep -c ': attach ' ~/.studio-terminal-spike/daemon.log)"
  echo "  tmux $($T list-panes -a -F '#{session_name} fg=#{pane_current_command} dead=#{pane_dead}:#{pane_dead_status}' 2>&1 | tr '\n' ' ')clients=$($T list-clients 2>/dev/null | wc -l | tr -d ' ')"
  echo "  herdr $(herdr session list | grep studio-spike | awk '{print $1, $2}')"
}

launch
state "first launch"
D=$(ask '{"op":"list"}' | jq -r '[.terms[] | select(.exited == null) | .id][0]')
S=$($T list-sessions -F '#{session_name}' | head -1)
ask "{\"op\":\"input\",\"id\":\"$D\",\"data\":\"$(printf '%s\r' "$COUNTER" | base64)\"}" > /dev/null
$T send-keys -t "$S" "$COUNTER" Enter
# Terminals that exit while the app is down: code 7 after 3 seconds.
ask "{\"op\":\"spawn\",\"id\":\"exit-probe\",\"argv\":[\"/bin/sh\",\"-c\",\"sleep 3; exit 7\"],\"cwd\":\"/\",\"cols\":80,\"rows\":24}" > /dev/null
$T -f tmux.conf new-session -d -s exit-probe 'sleep 3; exit 7'

kill -9 $APP; sleep 5
state "crash (kill -9), 5 s later"
launch
state "relaunch after the crash"

touch src/app.rs && cargo build -q 2>&1 | tail -3
kill $APP; sleep 2
state "rebuilt the binary, then quit the old app"
launch
state "relaunch on the new binary"

kill $APP; sleep 2
launch
state "quit (SIGTERM) and relaunch"

echo "== memory (RSS)"
for p in $(pgrep -f 'studio-terminal-spike --daemon') $($T display -p '#{pid}') $(pgrep -f 'herdr server'); do
  echo "  $(ps -o rss=,command= -p "$p" | awk '{printf "%.1f MB  ", $1 / 1024; $1 = ""; print}')"
done
echo "== the app stays open; quit it when done"
