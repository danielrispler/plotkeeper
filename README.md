# Studio

The control room's app (map #266, spec #328), for macOS. This version's pane follows the Claude Code session you prompted last: its last step, live, and a card on each wait's first card step. A card step is a check; an Edit or Write outside a `.claude/plans/` folder, where plans are kept; or a Bash command that does not start with `git` and changed the worktree, whether it succeeded or failed. A step that did not run, because it was denied or interrupted, is no card step. A check is a command from the Falinks check list: `cargo test|nextest|clippy|check|build`, `ci-checks`, `go test|vet`, run in the foreground; flags or a toolchain may come before the subcommand (`cargo +nightly test`). When the card goes to a check, the pane asks "Will it pass?" as soon as it starts; tap yes or no before its result arrives. The check's card says whether it passed. Your guess is recorded, never shown as a score. A card shows its question; tap it for the answer. The card store keeps the cards, and which ones you opened, across restarts; steps that ran while the app was down get no cards.

To tell whether a Bash command changed the worktree, the app fingerprints the session's git worktree at each prompt, and after each step until the wait has had its card step: the files `git status` lists, with their content. After a restart it also fingerprints the worktrees of the waits still open, so their next Bash step can be compared. Ignored files do not count, and a commit alone changes nothing. A file over 1 MiB is compared by its size and modification time, and the card writer gets one line that names it instead of its diff.

The card writer runs `claude -p` on your subscription, with `ANTHROPIC_API_KEY` and `ANTHROPIC_AUTH_TOKEN` removed from its environment, in `card-writer/` beside the event log. A step's change is untrusted text, so the writer runs with no tools (`--tools ""`) and no MCP servers (`--strict-mcp-config`), whatever your settings allow. It gets at most the first 64 KiB of the change. It also runs without `CLAUDE_CODE_PLUGIN_DIRS`, so its own sessions send no hook events. It writes the card in Hebrew, right to left, with code left to right. A step whose card does not parse gets no card and one line on the app's stderr; `open --stderr <file>` keeps it.

An app opened from Finder or `open` does not get your shell's `PATH`, so at start the app asks your login shell where `claude` is: `$SHELL -lc 'command -v claude'`, with `/bin/zsh` when `SHELL` is unset. Only the path comes back; the writer keeps the app's own environment. A login shell that is not interactive reads `~/.zprofile` but not `~/.zshrc`, so the line that puts `claude` on your `PATH` must be in the first. With no answer, the app runs `claude` from its own `PATH`.

To run the app with a fake writer instead of `claude -p`:

    open --env STUDIO_CARD_WRITER=fake target/release/bundle/macos/Studio.app

The pane also shows the session's state: working, your turn, or needs you, the only one in colour. When a session newly needs you and the window is not focused, macOS shows a notification. macOS asks once for permission to show them.

- `plugin/`: the Claude Code plugin. On twelve hook events, `SubagentStart` and `SubagentStop` among them, it runs `studio-hook`.
- `hook/`: `studio-hook`, which appends each hook payload as one line to the event log, and the log's path.
- `core/`: the studio core, plain Rust with no I/O, and `scrub`, which turns event log lines into test fixtures. The app replays the whole log through it at start, then feeds it each new line live. Only live lines request cards and notifications.
- `understanding/`: the understanding layer's I/O: the card writer interface, the `claude -p` writer and a fake, the worktree fingerprint, the card store, and `weekly`, the command that prints the week's rates.
- `src-tauri/`: the app's Rust shell. `ui/`: its TypeScript pane.

## Build

Needs Node and the Rust toolchain.

    cd apps/studio
    npm ci
    npm run tauri build

This builds `target/release/bundle/macos/Studio.app` with an ad-hoc signature (`codesign -dv` shows `Signature=adhoc`), and `target/release/studio-hook`, which the plugin runs. Open the app with `open target/release/bundle/macos/Studio.app`.

`cargo test` runs:

- `core/tests/`: hook events replayed through the core: what the pane shows and the two recorded logs (`replay`), needs you (`needs_you`), checks and guesses (`checks`), and Bash card steps (`bash_steps`);
- `understanding/tests/`: the `claude -p` command, built but never run, the `claude` lookup with a stand-in shell, and the writer's replies (`writer`); the fingerprint against real `git` in temporary repositories (`fingerprint`); the card store (`store`); the weekly rates (`week`);
- the unit tests of the event log's path (`hook`), of what `scrub` keeps (`studio-core`) and of the weekly rates' dates (`understanding`).

It leaves out the shell, which needs `dist/` from `npm run build`.

## Load the plugin

Add the plugin folder to `CLAUDE_CODE_PLUGIN_DIRS` in your shell profile:

    export CLAUDE_CODE_PLUGIN_DIRS=/Users/danielrispler/work/Falinks/apps/studio/plugin

Each `claude` session then loads the plugin in place, as `falinks-studio@inline` in `claude plugin list`. Nothing is written to `~/.claude/settings.json`. `claude plugin install` would write `enabledPlugins` and `extraKnownMarketplaces` there.

The hooks run the `studio-hook` built in this checkout, so build before you start a session. Edits to the plugin take effect at the next session start or `/reload-plugins`.

## The event log

`~/Library/Application Support/dev.falinks.studio/events.jsonl`, one hook payload per line, with two added fields:

- `studio_terminal_id`: the session's `STUDIO_TERMINAL_ID`, when set;
- `studio_received_ms`: when the hook received the event, in Unix milliseconds.

The hooks are async, so Claude Code never waits for them, and they append whether the app runs or not. Two events that fire within a few milliseconds can land in either order.

The log is never rotated. It holds tool inputs and results, file contents included.

## Recorded logs

Two fixtures in `core/tests/fixtures/` are real event logs, scrubbed by `scrub` (`core/src/bin/scrub.rs`). Each event keeps its shape and loses its content: text becomes `…`, and lists and objects become empty, except for what stays. What stays: ids, event names, `cwd`, the permission mode, a session's start source and end reason, agent ids and types, the `description` of a spawn or a Bash command, `isolation`, the paths of the files a tool reads or writes, skill names, the types and statuses in tool results, background tasks without their commands, exit codes, numbers, flags and timings. An MCP tool keeps no text. Transcript and output paths become `…`. A prompt keeps only the markup Claude Code adds, with its ids and statuses, so a wake-up stays one; any other tag becomes `<…>`. A last message that asks Daniel something becomes `…?`.

- `checkpoint-day.jsonl`: the events of 2026-10-30, the checkpoint day of #347. Some of its sessions were still open at midnight.
- `subagent-run.jsonl`: a scripted session in a temporary repository that spawns a worktree worker and a read-only helper in the background, then stops the helper.

Re-record the second when Claude Code's version changes. The script runs a nested `claude` on your subscription, so run it yourself, outside auto mode:

    core/tests/record-subagent-run.sh

Quit the studio app first: it would take the session as live and ask for cards. The script builds `studio-hook`, runs the session with this checkout's plugin, and scrubs the session's events from the event log into the fixture. Before you commit the new fixture, run `cargo test` and check that each text it keeps is a label, an id or a path.

## The card store

`cards.json` beside the event log: each card with the step it explains and its record, in Unix milliseconds: `added_at`, when the card arrived; `shown_at`, when the pane first drew its question while the window was visible; and `revealed_at`, when you first opened it. Each is `null` until then, and in stores written before it existed. A check's card also keeps your tap (`request.check.guess`, `null` when you did not tap before the result), `passed` and `outcome` (`right`, `wrong` or `not_counted`). `passed` comes from the exit code when nothing in the command can mask it: the check is the command's last statement (after the last `;` or line break), with no `||`, no leading `!` and no `&` that sends it to the background, and a pipe only after a `set` that turns on `pipefail`. Otherwise it comes from the card writer, which can answer unknown (`null`); an unknown verdict is not counted. A missing store starts empty. The app writes it whole to a temporary file and renames it into place, so a crash mid-write keeps the previous store.

## The weekly rates

The app shows no rate, score or tally. Once a week, print the rates of the cards that arrived in the last 7 days:

    cd apps/studio
    cargo run --bin weekly

- thinking rate: of the cards you opened, the share you opened 3 s or more after their question showed;
- surprise rate: of the guesses that settled, the share that were wrong; no tap and an unknown verdict do not settle;
- unknown rate: of the check cards, the share whose `passed` was unknown. A high rate argues for a `pipefail` instruction to agents.

A rate with nothing to count prints `n/a`. To record your one-line verdict on the questions, add it after `--`:

    cargo run --bin weekly -- the questions made me stop twice, the rest I clicked through

This appends the rates and your verdict to `weekly.txt` beside the event log. If you mostly click through the questions, or call them a chore, they go and plain cards stay. The thresholds are starting guesses.
