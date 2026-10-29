#!/bin/sh
# Spike for #322: append each hook payload to ~/.studio-terminal-spike/events.jsonl, with the terminal variables the
# hook process inherited. The log lives outside the repo so it survives removing the worktree.
mkdir -p "$HOME/.studio-terminal-spike"
jq -c --arg t "${STUDIO_TERMINAL_ID-unset}" --arg tmux "${TMUX_PANE-}" --arg herdr "$(env | grep '^HERDR_' | tr '\n' ' ')" \
  '{at: (now | todate), studio_terminal_id: $t, tmux_pane: $tmux, herdr_env: $herdr} + .' >> "$HOME/.studio-terminal-spike/events.jsonl"
