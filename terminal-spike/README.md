# Terminal spike (#322), throwaway

A Tauri 2 window with xterm.js tabs over three ways to keep terminals alive outside the app. Like the v0 spike, it is
not the studio's stack: the frontend is plain HTML and JS with no build step, and the hook logger is a shell script
that needs `jq`. Nothing here is a decision.

## Run

    cd apps/studio/terminal-spike
    npm install          # copies xterm.js into dist/vendor
    cargo run

Needs `tmux` and `herdr` on `PATH` (`brew install tmux herdr`). Start it from a normal terminal, not from a Claude
Code session: the app's environment becomes the daemon's.

The buttons:

- **+ daemon shell**: a login shell owned by the background terminal process, the same binary run as `--daemon` (Orca's model).
- **+ claude (daemon)**: the same, running `claude --plugin-dir hook`, then a shell when it exits.
- **+ tmux shell**: a tmux session on a private server (`tmux -L studio-spike`, settings in `tmux.conf`). The app runs its attach client.
- **herdr**: one tab running the Herdr client on its own session (`herdr --session studio-spike`). Herdr's tabs and panes live inside it.
- **+ local shell**: a PTY inside the app, the control case: it dies with the app.

Right-click a tab to close it. On start the app reattaches to every daemon terminal, every tmux session and the Herdr
session. The status line shows replay sizes and times, exits, and any OSC 7501 the terminal receives.

`hook/` is a local Claude Code plugin. It logs 31 of the 33 documented hook events to
`~/.studio-terminal-spike/events.jsonl`. It leaves out `WorktreeCreate` and `WorktreeRemove`, because a command hook on
those replaces worktree creation. Each line carries the `STUDIO_TERMINAL_ID`, `TMUX_PANE` and `HERDR_*` values that
the hook process inherited. The app passes the plugin's path from the checkout it was built in; the status line shows
the `claude --plugin-dir …` command for typing it by hand.

## Scripted check

`./probe.sh` crashes (`kill -9`), rebuilds and quits the app with a shell counter in each backend, and prints after
each step what still runs and whether the app reattached. It uses shells only, no `claude`.

## Reset

    pkill -f 'studio-terminal-spike --daemon'; tmux -L studio-spike kill-server; herdr session stop studio-spike

This kills every spike terminal. The daemon's socket and log, and the hook event log, are in `~/.studio-terminal-spike/`.

`wayfinder-startup --done` refuses a worktree with ignored files other than `target/`, so first run
`rm -rf node_modules dist/vendor app.log` here.
