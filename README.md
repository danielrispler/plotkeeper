# Studio

The control room's app (map #266, specs #328 and #369), for macOS. On the left, the session list: every Claude Code session, one line each with its folder, state and what it is doing, grouped by state with "needs you" first. While a session runs, its line is its last step. Once its turn is over, the line is the summary of the card on that turn; the first line of the agent's last message until that card arrives; or the brief's latest line when the turn got no card. Idle and ended sessions collapse at the bottom. On the right, the session you clicked: its last step, live, its brief, its cards, the last 30 minutes' apart from older ones, and its checks. Only your click changes the session shown. At a session's `Stop`, when its worktree changed since its last card, one card explains that change, commits made during the prompt included. A turn that changed nothing gets no card. When the change is structural, the card has an architecture section (ארכיטקטורה). The writer reads the summaries of the earlier cards on your prompt, its wake-ups' cards included, so a structural change spread over several cards still counts.

A worker is explained like a session. It gets a card at each commit in its worktree (its `HEAD` moved, seen after a Bash step) and at its end (`SubagentStop`, or for a killed worker, which has none, the wake-up that reports it), each on its change since its previous card, and its own brief, updated with each. Click its line to see its brief and cards. On any step of a worker, or of a prompt of yours before its turn ends, when 20 minutes or more have passed since the last card on that worktree and it changed, a card is written. At a session's `Stop`, when it changed nothing itself but its workers wrote cards since its last card, its card is written from their summaries, not their diffs: what it split up, what came back and what is still open. When a wake-up opened that turn, there is no card; the brief takes in the workers' cards instead, and the row shows its latest line. A file a helper, or another subagent with no worktree of its own, edits or writes in its parent's worktree shows on the parent's card under that subagent's name.

The brief, at the top of the session's view, is a short account of the session's work that you can resume from. The writer updates it with each card, from the brief so far and the change. Every 5th update rewrites it from scratch, from your prompts in the session (wake-ups left out), all its card summaries and `git diff --stat` since its first card's base, never the raw diff. Past 64 KiB, the oldest prompts are cut first, and every summary stays. When the rewrite fails, the updated brief stays and the app logs one line on stderr. A check is a command from the Falinks check list: `cargo test|nextest|clippy|check|build`, `ci-checks`, `go test|vet`, run in the foreground; flags or a toolchain may come before the subcommand (`cargo +nightly test`). Each check that ran shows as one line: passed or failed when its exit code tells, ran when something in the command can mask it (see "The card store"). A check is never a card. A card opens with its quiz: a multiple-choice question on the change's logic or design (unlike an agent's question, which the session view shows apart). Tap an option: the card says at once whether you were right, shows under each option why it is right or wrong, then the explanation. If you think the writer's key is wrong, tap "the key is wrong" (המפתח שגוי); the flag is kept on the card. Your guesses are recorded, never shown as a score. When the writer's quiz is missing or does not parse, the card shows its explanation with no quiz, and the app logs one line on stderr. Under the explanation, "explain more" (הסבר עוד) asks the writer once more, on the card's change and its explanation, and shows the longer explanation under the card; later taps open and close the kept text with no new call. When the writer gives no text, the app logs one line on stderr and the next tap asks again. Rate the explanation: good, fair or poor (טוב, בינוני, חלש), then tick its reasons: unclear, wrong, too shallow, too easy, and, on a card with a quiz, the key is wrong, which is the same flag as the quiz's. The card store keeps the cards, your guesses, flags, ratings and longer explanations across restarts; turns that ended while the app was down get no cards.

To tell what a prompt changed, the app fingerprints the session's git worktree at its prompt and at its `Stop`: `HEAD`, and the files `git status` lists, with their content. The card covers the difference: the files listed either time, and the files the commits in between changed. Each card's change is the next one's base, and each session keeps its own, so another session's prompt in the same worktree moves no base. A prompt that lands while the session's turn is still open, after an interrupt, takes no new base, so the interrupted prompt's change stays in the card. Each subagent gets its own fingerprint too, of the folder of its first event: its base. After each of its Bash steps the app looks again; that look becomes the next base only when `HEAD` moved, so an edit seen before the commit stays in the commit's card. A helper is also fingerprinted right before each Bash step: a change across that step makes it a worker, while what others changed in a shared worktree before it does not. After a restart the app fingerprints the turns still open and the subagents still running, so their next look can be compared. Ignored files do not count, and a commit alone changes nothing. A file over 1 MiB is compared by its size and modification time, and the card writer gets one line that names it instead of its diff.

The card writer runs `claude -p` on your subscription, with `ANTHROPIC_API_KEY` and `ANTHROPIC_AUTH_TOKEN` removed from its environment, in `card-writer/` beside the event log. A change is untrusted text, so the writer runs with no tools (`--tools ""`) and no MCP servers (`--strict-mcp-config`), whatever your settings allow. It gets at most the first 64 KiB of the change. It also runs without `CLAUDE_CODE_PLUGIN_DIRS`, so its own sessions send no hook events. It writes the card in Hebrew, right to left, with code left to right. A card that does not parse is asked for once more; when the second fails too, there is no card and one line on the app's stderr; `open --stderr <file>` keeps it.

An app opened from Finder or `open` does not get your shell's `PATH`, so at start the app asks your login shell where `claude` is: `$SHELL -lc 'command -v claude'`, with `/bin/zsh` when `SHELL` is unset. Only the path comes back; the writer keeps the app's own environment. A login shell that is not interactive reads `~/.zprofile` but not `~/.zshrc`, so the line that puts `claude` on your `PATH` must be in the first. With no answer, the app runs `claude` from its own `PATH`.

To run the app with a fake writer instead of `claude -p`:

    open --env STUDIO_CARD_WRITER=fake target/release/bundle/macos/Studio.app

To run the real writer on two fixture changes, one structural and one not, run this from `apps/studio`, outside auto mode. For each change it prints the raw reply, the card and the brief parsed from it. Then it prints the brief rewritten, the second card's "explain more", and a parent's card and brief from two workers' card summaries:

    cargo run -q -p studio-understanding --example real_card

A session is running from a prompt until its turn ends; needs you while it waits on your permission, answer or plan approval; your turn when its turn ended without asking you, or it started and has had no prompt yet; idle after 2 hours at your turn with no events, until its next event; ended from its `SessionEnd` until it starts again under its id. A wake-up is not your prompt, so it leaves "needs you" in place. Only "needs you" is in colour. macOS shows a notification when a session newly needs you, a card arrives or a worker fails, only while the window is not focused. A subagent's wait, failure or card shows on its session's line and names the subagent; a failed helper raises none. Notices within 3 seconds of the first show as one notification. macOS asks once for permission to show them. The Dock badge counts the sessions that need you.

- `plugin/`: the Claude Code plugin. On twelve hook events, `SubagentStart` and `SubagentStop` among them, it runs `studio-hook`.
- `hook/`: `studio-hook`, which appends each hook payload as one line to the event log, and the log's path.
- `core/`: the studio core, plain Rust with no I/O, and `scrub`, which turns event log lines into test fixtures. The app replays the whole log through it at start, then feeds it each new line live. Only live lines request cards and notifications.
- `understanding/`: the understanding layer's I/O: the card writer interface, the `claude -p` writer and a fake, the worktree fingerprint, the card store with the briefs, Claude Code's version kept beside the event log, and `weekly`, the command that prints the week's thinking and surprise rates, ratings by level and reason and flags on the key, then the drift: unrecognised wake-ups and version changes.
- `src-tauri/`: the app's Rust shell. `ui/`: its TypeScript pane.

## Build

Needs Node and the Rust toolchain.

    cd apps/studio
    npm ci
    npm run tauri build

This builds `target/release/bundle/macos/Studio.app` with an ad-hoc signature (`codesign -dv` shows `Signature=adhoc`), and `target/release/studio-hook`, which the plugin runs. Open the app with `open target/release/bundle/macos/Studio.app`.

`cargo test` runs:

- `core/tests/`: hook events replayed through the core: what the pane shows and the two recorded logs (`replay`), the session list, its states and the selection on the recorded day (`sessions`), needs you (`needs_you`), the subagent tree, workers and helpers, their status, end and fading, and the waits they raise on their session (`subagents`), agents' questions, who asks them and their answers (`questions`), notifications: their triggers, text and combining (`notices`), check lines (`checks`), cards at `Stop`, the prompt each belongs to, the `Stop` a row tells about, where the shell fingerprints, and the recorded worker's cards at its commit and its end (`cards`), and wake-ups, the needs you a wake-up leaves in place and the ask for Claude Code's version (`wake_ups`);
- `understanding/tests/`: the `claude -p` commands for a card, for "explain more" and for a brief's rewrite with its cut, built but never run, the `claude` lookup with a stand-in shell, the writer's replies, their quizzes, architecture sections and briefs, and its retry (`writer`); the fingerprint against real `git` in temporary repositories (`fingerprint`); the core, the fingerprint, `git diff --stat`, a fake writer and the card store joined as the shell joins them: the earlier summaries and the brief each card reads, the 5th update's rewrite, a stopped row's line, a worker's cards and brief in its own worktree, the 20-minute fallback and none between turns, a killed worker's end card at its wake-up, a parent's card from its workers' summaries, a wake-up's brief with no card, a helper's edits on its parent's card and a helper made a worker by its command, and not by others' changes before it (`prompt_cards`); the card store, your guesses, flags and ratings, "explain more" with a fake writer, the briefs, its move aside and its split at 30 minutes (`store`); the weekly rates and ratings (`week`); the version read from a transcript, its notice and `weekly`'s drift lines (`drift`);
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

The app tells you when the version changed. At a session's first `Stop`, it reads Claude Code's version from the session's transcript, the `.jsonl` file at the event's `transcript_path`, and opens no other file. It keeps each new version, with when it was first seen, in `claude-versions.txt` beside the event log. A version that differs from the last one kept shows a one-line notice at the top of the pane, one line on stderr, and a line in `weekly`. The first version kept has nothing to compare with, so it shows nothing.

## Wake-ups

When a subagent or a background command returns, Claude Code sends its parent a prompt that names the task's id in markup: `<agent-message from="…">` or `<task-notification><task-id>…</task-id>`. A prompt that names a task id the session knows, from a spawn's response, a background command's response or `Stop`'s `background_tasks`, that way is a wake-up, never your prompt. The session's row reads "worker … returned" until its next step or the end of its turn. A card on a wake-up's turn belongs to your prompt before it, so it reads that prompt's earlier cards. A turn a wake-up opened that changed nothing itself gets no card, only a brief updated from its workers' cards. A known id in plain text ("check what a1191 did") is your prompt. A known id in any other markup is your prompt too, logs one line on stderr and counts in `weekly`: Claude Code may have changed its markup.

## The card store

`cards-v2.json` beside the event log: each card with the change it explains, its base (`request.base`: the `HEAD` sha and the files `git status` listed), the prompt of yours it belongs to (`request.prompt`, when the hook received it; a worker's card, when the worker started), the worker whose card it is (`request.agent`, `null` for the session's own), the files other subagents edited in its worktree (`request.changed_by`, by subagent id), the workers' cards a parent's card was written from (`request.workers`), its one-line `summary`, its `architecture` section (`null` when the change is not structural) and its record, in Unix milliseconds: `added_at`, when the card arrived; `shown_at`, when the pane first drew its quiz while the window was visible; and `guess`, your tap: the `option` you tapped (from 0), whether it was `right` by the writer's key, and `at`. Each is `null` until then. `key_wrong` is `true` once you flag the key, from the quiz or from your rating. `rating` is your latest rating: its `level` (`good`, `fair` or `poor`) and its `reasons` (`unclear`, `wrong`, `too_shallow`, `too_easy`). `more` is the longer explanation from your first "explain more". A missing store starts empty. Cards stored before quizzes (#375) load with no quiz, and cards stored before briefs (#377) with no summary and no prompt. `briefs.json` beside it keeps each session's and each worker's brief, by session or subagent id, and its count of updates. The app writes each file whole to a temporary file and renames it into place, so a crash mid-write keeps the previous one. On the first start after cards came to cover a prompt's whole change (#374), the app moves the old store, `cards.json`, to `cards-v1.json`, never over one already there, and starts the new store empty; `weekly` does the same.

A check's exit code says whether it passed when nothing in the command can mask it: the check is the command's last statement (after the last `;` or line break), with no `||`, no leading `!` and no `&` that sends it to the background, and a pipe only after a `set` that turns on `pipefail`. Check lines come from the event log, not the store.

## The weekly rates

The app shows no rate, score or tally. Once a week, print the rates of the cards that arrived in the last 7 days:

    cd apps/studio
    cargo run --bin weekly

- thinking rate: of the quizzes you answered, the share you answered 3 s or more after they showed;
- surprise rate: of your guesses, the share that were wrong by the writer's key;
- ratings: how many of the week's cards you rated, by level;
- reasons: how many ratings gave each reason, and how many cards you flagged "the key is wrong", from the quiz or the rating.

Then the drift in Claude Code over the same 7 days, from the event log and `claude-versions.txt`:

- unrecognised wake-ups: the prompts that named a known task id in markup the studio does not know (see "Wake-ups");
- one line per version change, with its date.

A rate with nothing to count prints `n/a`. To record your one-line verdict on the questions, add it after `--`:

    cargo run --bin weekly -- the questions made me stop twice, the rest I clicked through

This appends the rates and your verdict to `weekly.txt` beside the event log. If you mostly click through the questions, or call them a chore, they go and plain cards stay. The thresholds are starting guesses.
