# Studio

The control room's app (map #266, specs #328 and #369), for macOS. On the left, the session list: every Claude Code session, one line each with its folder, state and what it is doing, grouped by state with "needs you" first. A session's line, in order: while its own agent needs you, its last step and how long it has waited ("Bash cargo test · 3m"); while a subagent needs you, its name, "asks" and how long ("Implement #355 asks · 3m"); once its turn is over, the summary of the card on that turn, the first line of the agent's last message until that card arrives, or the brief's latest line when the turn got no card; after a wake-up with no step since, "worker … returned"; else its last step, or "No step yet". Idle and ended sessions collapse at the bottom. On the right, the session you clicked: its last step, live, its brief, its cards, the last 30 minutes' apart from older ones, and its checks. Only your click changes the session shown. At a session's `Stop`, when its worktree changed since its last card, one card explains that change, commits made during the prompt included. A turn that changed nothing gets no card. When the change is structural, the card has an architecture section (ארכיטקטורה). The writer reads the summaries of the earlier cards on your prompt, its wake-ups' cards included, so a structural change spread over several cards still counts.

A worker is explained like a session. It gets a card at each commit in its worktree (its `HEAD` moved, seen after a Bash step) and at its end (`SubagentStop`, or for a killed worker, which has none, the wake-up that reports it), each on its change since its previous card, and its own brief, updated with each. Click its line to see its brief and cards. On any step of a worker, or of a prompt of yours before its turn ends, when 20 minutes or more have passed since the last card on that worktree, or since its last such look that found no change, the app looks again: a card is written when it changed, and a look that finds no change starts a new 20-minute wait. So after 20 quiet minutes, a change gets its card up to 20 minutes later. At a session's `Stop`, when it changed nothing itself but its workers wrote cards since its last card, its card is written from their summaries, not their diffs: what it split up, what came back and what is still open. When a wake-up opened that turn, there is no card; the brief takes in the workers' cards instead, and the row shows its latest line. A file a helper, or another subagent with no worktree of its own, edits or writes in its parent's worktree shows on the parent's card under that subagent's name.

The brief, at the top of the session's view, is a short account of the session's work that you can resume from. The writer updates it with each card, from the brief so far and the change. Every 5th update rewrites it from scratch, from your prompts in the session (wake-ups left out), all its card summaries and `git diff --stat` since its first card's base, never the raw diff. Past 64 KiB, the oldest prompts are cut first, and every summary stays. When the rewrite fails, the updated brief stays and the app logs one line on stderr. A check is a command from the Falinks check list: `cargo test|nextest|clippy|check|build`, `ci-checks`, `go test|vet`, run in the foreground; flags or a toolchain may come before the subcommand (`cargo +nightly test`). Each check that ran shows as one line: passed or failed when its exit code tells, ran when something in the command can mask it (see "The card store"). A check of the session's own agent that usually runs over 2 minutes (the lower median of its earlier runs that did not fail) gets a card at its start, on the change since the last card, when there is one; a subagent's checks get no line and no card. Its quiz asks whether the check will pass, and the check's result settles your guess: until it ends, the card says only that the check runs; once it ends, it says whether it passed and whether you were right, and shows the writer's reasons for a pass and a fail. A tap after the result counts for nothing. When the exit code does not say whether the check passed, the guess is not counted and no option is named the right one. That card has no "the key is wrong". A card opens with its quiz: a multiple-choice question on the change's logic or design (unlike an agent's question, which the session view shows apart). Tap an option: on any card but a check's, the card says at once whether you were right, shows under each option why it is right or wrong, then the explanation. If you think the writer's key is wrong, tap "the key is wrong" (המפתח שגוי); the flag is kept on the card. Your guesses are recorded, never shown as a score. When the writer's quiz is missing or does not parse, the card shows its explanation with no quiz, and the app logs one line on stderr. Under the explanation, "explain more" (הסבר עוד) asks the writer once more, on the card's change and its explanation, and shows the longer explanation under the card; later taps open and close the kept text with no new call. When the writer gives no text, the app logs one line on stderr and the next tap asks again. Rate the explanation: good, fair or poor (טוב, בינוני, חלש), then tick its reasons: unclear, wrong, too shallow, too easy, and, on a card with a quiz, the key is wrong, which is the same flag as the quiz's. The card store keeps the cards, your guesses, flags, ratings and longer explanations across restarts; turns that ended while the app was down get no cards.

To tell what a prompt changed, the app fingerprints the session's git worktree at its prompt and at its `Stop`: `HEAD`, and the files `git status` lists, with their content. The card covers the difference: the files listed either time, and the files the commits in between changed. Each card's change is the next one's base, and each session keeps its own, so another session's prompt in the same worktree moves no base. A prompt that lands while the session's turn is still open, after an interrupt, takes no new base, so the interrupted prompt's change stays in the card. Each subagent gets its own fingerprint too, of the folder of its first event: its base. After each of its Bash steps the app looks again; that look becomes the next base only when `HEAD` moved, so an edit seen before the commit stays in the commit's card. A helper is also fingerprinted right before each Bash step: a change across that step makes it a worker, while what others changed in a shared worktree before it does not. After a restart the app fingerprints the turns still open and the subagents still running, so their next look can be compared. Ignored files do not count, and a commit alone changes nothing. A file over 1 MiB is compared by its size and modification time, and the card writer gets one line that names it instead of its diff.

The card writer runs `claude -p` on your subscription, with `ANTHROPIC_API_KEY` and `ANTHROPIC_AUTH_TOKEN` removed from its environment, in `card-writer/` beside the event log. A change is untrusted text, so the writer runs with no tools (`--tools ""`) and no MCP servers (`--strict-mcp-config`), whatever your settings allow. It gets at most the first 64 KiB of the change. It also runs without `CLAUDE_CODE_PLUGIN_DIRS`, so its own sessions send no hook events. It writes the card in Hebrew, right to left, with code left to right. A card that does not parse is asked for once more; when the second fails too, there is no card and one line on the app's stderr; `open --stderr <file>` keeps it.

An app opened from Finder or `open` does not get your shell's `PATH`, so at start the app asks your login shell where `claude` is: `$SHELL -lc 'command -v claude'`, with `/bin/zsh` when `SHELL` is unset. Only the path comes back; the writer keeps the app's own environment. A login shell that is not interactive reads `~/.zprofile` but not `~/.zshrc`, so the line that puts `claude` on your `PATH` must be in the first. With no answer, the app runs `claude` from its own `PATH`.

To run the app with a fake writer instead of `claude -p`:

    open --env STUDIO_CARD_WRITER=fake target/release/bundle/macos/Studio.app

To run the real writer on two fixture changes, one structural and one not, run this from `apps/studio`, outside auto mode. For each change it prints the raw reply, the card and the brief parsed from it. Then it prints the brief rewritten, the second card's "explain more", and a parent's card and brief from two workers' card summaries:

    cargo run -q -p studio-understanding --example real_card

A session is running from a prompt until its turn ends; needs you while it waits on your permission, answer or plan approval; your turn when its turn ended without asking you, or it started and has had no prompt yet; idle after 2 hours at your turn with no events, until its next event; ended from its `SessionEnd` until it starts again under its id. A wake-up is not your prompt, so it leaves "needs you" in place. Only "needs you" is in colour. macOS shows a notification when a session newly needs you, a card arrives or a worker fails, only while the window is not focused. A subagent's wait, failure or card shows on its session's line and names the subagent; a failed helper raises none. A subagent's name is its spawn's description, else the first 5 characters of its id; the list, its view's header, the notices and the card writer all use it. A session's folder is the last segment of its `cwd`, or the whole `cwd` when the folder ends in a slash. Notices within 3 seconds of the first show as one notification. macOS asks once for permission to show them. The Dock badge counts the sessions that need you.

- `plugin/`: the Claude Code plugin. On twelve hook events, `SubagentStart` and `SubagentStop` among them, it runs `studio-hook`. On the `PreToolUse` of `AskUserQuestion` and `ExitPlanMode`, and on every `PermissionRequest`, it also runs `studio-answer`, synchronously.
- `hook/`: `studio-hook`, which appends each hook payload as one line to the event log, and the log's path; and `studio-answer`, the answer hook (#477, #478): while the studio watches the asking session, it waits up to 120 seconds for an answer from the card and returns it: a question's `answers`, a plan's approval or rejection, a permission's allow or deny. It logs that answer as a `StudioAnswer` line, which clears "needs you" for a plan or a permission at once. Otherwise it returns at once with no decision and the terminal asks. Its files and trust boundary are in `hook/src/answer.rs`.
- `core/`: the studio core, plain Rust whose only I/O is reading the sessions' transcripts (`src/transcript.rs`, `.jsonl` files only), and `scrub`, which turns event log lines into test fixtures. The app replays the whole log through it at start, then feeds it each new line live. Only live lines request cards and notifications.
- `understanding/`: the understanding layer's I/O: the card writer interface, the `claude -p` writer and a fake, the worktree fingerprint, the card store with the briefs, Claude Code's version kept beside the event log, and `weekly`, the command that prints the week's thinking and surprise rates, ratings by level and reason and flags on the key, then the drift: unrecognised wake-ups and version changes.
- `terminal-daemon/`: `studio-terminal-daemon`, the background process that owns the terminals, so they outlive the app (#322, #464). Its own crate and binary; the app links only its client library and runs the binary, so an app rebuild neither rebuilds nor restarts it. `serve` runs it on `~/Library/Application Support/dev.falinks.studio/terminal-daemon.sock`, one at a time; the protocol is in `src/lib.rs`. Every terminal starts through `$SHELL -l`, so the profile sets `PATH` even when the studio was started from the Dock, and without `CLAUDE*` or terminal-identity variables. `attach <terminal>` connects any terminal app to one of its terminals; Ctrl-] detaches.
- `workbench/`: workbench git (#328, module 7): the picker's places from `git worktree list --porcelain`, and the `claude` command a session's terminal runs.
- `src-tauri/`: the app's Rust shell, with the terminal daemon's client (`src/terminal.rs`). `ui/`: its TypeScript pane.

## Build

Needs Node and the Rust toolchain.

    cd apps/studio
    npm ci
    npm run tauri build

This builds `target/release/bundle/macos/Studio.app` with an ad-hoc signature (`codesign -dv` shows `Signature=adhoc`), and `target/release/studio-hook`, which the plugin runs. The bundle carries `studio-terminal-daemon` beside the app's executable, in `Contents/MacOS/`. Open the app with `open target/release/bundle/macos/Studio.app`. `npm start` does both: it builds, recompiling only what changed, then opens the app.

`cargo test` runs:

- `core/tests/`: hook events replayed through the core: what the pane shows and the two recorded logs (`replay`), the session list, its states and the selection on the recorded day (`sessions`), needs you (`needs_you`), the subagent tree, workers and helpers, their status, end and fading, and the waits they raise on their session (`subagents`), agents' questions, who asks them and their answers (`questions`), notifications: their triggers, text and combining (`notices`), check lines (`checks`), cards at `Stop`, the prompt each belongs to, the `Stop` a row tells about, where the shell fingerprints, and the recorded worker's cards at its commit and its end (`cards`), the card at a long check's start, none for a short or unknown one, and the line that names it (`check_cards`), wake-ups, the needs you a wake-up leaves in place and the ask for Claude Code's version (`wake_ups`), and the daemon upgrade's idle rule and how each terminal comes back (`idle`);
- `understanding/tests/`: the `claude -p` commands for a card, for "explain more" and for a brief's rewrite with its cut, built but never run, the `claude` lookup with a stand-in shell, the writer's replies, their quizzes, architecture sections and briefs, and its retry (`writer`); the fingerprint against real `git` in temporary repositories (`fingerprint`); the core, the fingerprint, `git diff --stat`, a fake writer and the card store joined as the shell joins them: the earlier summaries and the brief each card reads, the 5th update's rewrite, each case of a row's line, a worker's cards and brief in its own worktree, the 20-minute fallback, its new wait after a quiet look, and none between turns, a killed worker's end card at its wake-up, a parent's card from its workers' summaries, a wake-up's brief with no card, a helper's edits on its parent's card and a helper made a worker by its command, and not by others' changes before it (`prompt_cards`); the card store, your guesses, flags and ratings, "explain more" with a fake writer, the briefs, its move aside and its split at 30 minutes (`store`); the weekly rates and ratings (`week`); a long check's card request reaching the writer with the change and the command, the check's result settling a guess made while it ran, a tap after the result or on a check whose exit code does not count, and a card stored after its check ended settled at once (`check_cards`); the version read from a transcript, its notice and `weekly`'s drift lines (`drift`);
- `terminal-daemon/tests/`: the daemon against real PTYs: spawn, attach with replay, input, resize, list with size, foreground program, exit code and memory, kill leaving no process of the terminal's session, processes left behind, an app with an older minimum protocol, one daemon per socket, the clean environment, the scrollback cut, `attach` run inside a daemon terminal, and `stop` ending every terminal while the record of the running ones stays;
- `workbench/tests/`: the picker's places against real `git` in temporary repositories: worktrees under `.claude/worktrees/` listed, one elsewhere not, `agent-*` ones hidden and counted, a detached `HEAD`, the listing from inside a task; and the `claude` command, new and resumed;
- `core/tests/terminals.rs`: each session's terminal from `studio_terminal_id`, also after a resume elsewhere;
- `core/tests/close_rename.rs`: closing asks first exactly while a session is in a turn or its latest `Stop` listed background tasks; a rename line keeps the session's name through a replay, as at a restart; a delete line hides the row through a replay, until the session starts again;
- `hook/tests/answer.rs`: the answer hook's contract (Seam 4), a payload on stdin and the studio played by the test in a scratch `HOME`: an answer's `allow` with the questions and each label, a multi-select's joined; an approved plan's `allow` with its input and a rejected one's `deny`; a permission's `allow` or `deny`; each card answer logged as `StudioAnswer`, a terminal one not; for questions, plans and permissions alike, an immediate return with no decision when nobody watches, another session is watched, the studio's pid is dead or its heartbeat old; no decision at the timeout or for "Answer in the terminal"; a wrong nonce or an answer file others may write refused;
- the unit tests of the event log's path (`hook`), of what `scrub` keeps (`studio-core`) and of the weekly rates' dates (`understanding`).

It leaves out the shell, which needs `dist/` from `npm run build`. `npm test` (after `npm ci`) builds the UI with `tsc` and runs `terminal-daemon/tests/replay.test.mjs`: a terminal's bytes replayed into a headless xterm of another size, with the resize sent, match a fresh render at that size; and `ui/tests/`: the UI's pure functions on the `tsc` output in `dist/`, such as the width rule's layout, the terminal's key mapping (`keys`) and its file paths (`paths`), and a check card's verdict and quiz state (`check_cards`).

## The terminal pane

The centre of the window is one terminal (#466): a login shell in the configured repository, `STUDIO_REPO` when set, else the main checkout of the repository the app was built in. It runs in the terminal daemon, so it outlives the app; at start the app attaches to it again and replays its screen. When no daemon runs, the app starts `studio-terminal-daemon serve` from beside its own executable, logging to `terminal-daemon.log` beside the socket. A shell that exited starts again at the next key.

The pane is xterm.js 6 with the WebGL renderer, which falls back to the default renderer when its context is lost, Unicode 11 widths, fit, search and web links. The CSP allows inline styles, which the default renderer needs. Terminal text is left to right.

- Ctrl+V, and Cmd+V with an image on the clipboard, send Ctrl+V: Claude Code reads the macOS clipboard itself and makes an `[Image #N]` chip. Cmd+V with text pastes it, bracketed.
- Shift+Enter sends ESC CR, a newline in Claude Code. Option is Meta: ESC, then the physical key, so Option+B is ESC b on any layout.
- URLs and file paths with a folder (`ui/keys.ts:12`) open on click, files in their default app, relative to the repository. A file dropped on the window types its path, quoted for the shell.
- Claude Code asks `ESC ] 7501 ; ?`, its Program Status query, and reports its status only to a terminal that answers. The pane answers `ESC ] 7501 ; ? BEL`.

Every Cmd shortcut goes to the terminal or to the app (`ui/keys.ts`):

| Shortcut | Goes to | Does |
|---|---|---|
| Cmd+C | terminal | copies the selection (the Edit menu's Copy) |
| Cmd+V | terminal | pastes text; with an image, sends Ctrl+V |
| Cmd+A | terminal | selects all of the scrollback |
| Cmd+F | terminal | finds in the scrollback: Enter next, Shift+Enter previous, Esc closes |
| Cmd+K | terminal | clears the scrollback |
| Cmd+Left, Cmd+Right | terminal | sends Ctrl+A, Ctrl+E: start and end of line |
| Cmd+Backspace | terminal | sends Ctrl+U: deletes to the line's start |
| Cmd+1, Cmd+2 | app | opens the session list, the story rail |
| Cmd+=, Cmd+-, Cmd+0 | app | zooms the window in, out (from half to double size) and back |
| any other Cmd shortcut | app | the app's menu: Cmd+Q, Cmd+W, Cmd+H, Cmd+M and the like |

Every shortcut here but Cmd+C and Cmd+V can be rebound in `keymap.json` (see Settings files).

## Settings files

`settings.json` and `keymap.json` sit in the studio's data folder, `~/Library/Application Support/dev.falinks.studio/`, in Zed's format: JSON with comments and trailing commas (#524). The app reloads them when either is saved. A file that does not parse is reported in the notice line, and the last good one stays in effect. Missing keys keep their defaults; a key of the wrong type is reported in the same line and keeps its default. `summaries` and `doctor` are read but not used yet; later tickets of #517 wire them:

```jsonc
{
  "zoom": 1,                     // 0.5 to 2
  "terminal": { "font_size": 15 }, // 8 to 40; default: the stylesheet's 12.5
  "summaries": { "model": "haiku", "effort": "low", "on_open": false },
  "doctor": [
    { "label": "Check the installation", "command": "claude doctor", "kind": "shell" },
    { "label": "Full checkup", "command": "/doctor", "kind": "session" }
  ]
}
```

`keymap.json` is Zed's array of contexts, `Workspace` for the window and `Terminal` for the pane. The actions are `studio::ToggleList`, `studio::ToggleRail`, `studio::Dismiss` (Esc), `studio::ZoomIn`, `studio::ZoomOut`, `studio::ResetZoom`, `terminal::SelectAll`, `terminal::Find`, `terminal::Clear` and `["terminal::SendText", "<text>"]`. Unlike Zed, binding an action to a new key unbinds its default key; `null` unbinds a key. A `Workspace` shortcut without Cmd, other than `studio::Dismiss`, leaves the key to the terminal or text box you are typing in:

```json
[{ "context": "Workspace", "bindings": { "cmd-l": "studio::ToggleList", "escape": null } }]
```

## Restarts and daemon upgrades

The terminals live in the daemon, so a studio restart, rebuild or crash loses none (#470). At start the app attaches every terminal the daemon runs and replays its screen, then fits it to the window, whatever size it had. One app runs at a time: a second one hands over and exits, and the first comes forward. When the second is another build, or a rebuild replaces the running app's executable, a bar over the terminal offers "Restart into it".

The daemon keeps a record of the terminals it runs, `terminal-daemon.terminals.json` beside its socket. When the app finds no daemon, after a crash or a reboot, it reads that record, starts a new daemon and brings each terminal back under its id: a shell in its folder, and a Claude terminal with `claude --resume <session_id>`, its latest session's id from the hooks, so the conversation survives and the shell's state does not. The same happens when the daemon dies while the app runs.

A daemon below the app's protocol stays in use while the app waits for idle: every shell at its prompt, and every Claude session between turns with no background tasks in its latest `Stop`. The bar names what still runs. Then the app asks, listing each terminal and what it loses. It never restarts the daemon on its own. On yes it stops the old daemon (`SIGTERM`, then the hangup and `SIGKILL` to every process of its terminals), and the path above brings each terminal back in the new one.

## Sessions in the studio

"New session", in the top bar, opens the picker: the configured repository's main checkout and its tasks, the worktrees under `.claude/worktrees/`, read from `git worktree list --porcelain` each time it opens. `agent-*` worktrees are hidden, and the main checkout says how many. Choosing one starts `claude` there in a new daemon terminal, through a login shell, as `env CLAUDE_CODE_PLUGIN_DIRS=<this checkout>/apps/studio/plugin claude`: the studio plugin alone, so a profile that also sets the variable never loads the hooks twice. The terminal's id, `claude-<uuid>`, reaches every hook as `STUDIO_TERMINAL_ID`, and the core links the session's row to it (`terminal` in the list). Clicking a row shows its terminal; a session the studio did not start shows the login shell. A terminal from an earlier app run is attached when the daemon still runs it, and reported otherwise. "Open", on an ended session's view, runs `claude --resume <session_id>` in a new terminal in the session's folder. "Rename" names the session: the shell appends a `StudioRename` line to the event log, so the name survives a restart, and, while its `claude` runs in a studio terminal, types `/rename <name>` there, so Claude Code shows the same name. An empty name gives the row its folder's again. "Close" ends the session's terminal and its whole process group (`kill`, then `forget`), and the login shell shows again. It asks first while the session is in a turn or its latest `Stop` listed background tasks. "Delete" drops the session's row: it ends its terminal as "Close" does, then the shell appends a `StudioDelete` line to the event log, so the row stays gone after a restart until a `SessionStart` of the session brings it back; its cards stay in the store. When the session's folder is in a task, `.claude/worktrees/<task>`, a box, unticked by default, also removes that worktree, asking first in the same way, then its branch, as the picker's "Remove" does: with `git branch -d` when the main checkout's `HEAD` holds it, or, after a squash merge, when `gh` lists a merged pull request whose head is the branch's tip, with `git update-ref -d` at that tip. Any other branch stays, and a kept branch is named over the story. The dialog says when another session still works in that worktree.

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

## Plan usage

Claude Code gives the plan's 5-hour and weekly usage only to the status line command, on its stdin. The line in `workbench/statusline-usage.sh`, added to your status line script right after it reads stdin into `$input`, saves that stdin as `usage/<session_id>.json` beside the event log, by temp file and rename, readable only by you (mode 0600), and prints nothing. The top bar shows the newest snapshot's `rate_limits`, or "no usage". Snapshots are never deleted; remove the folder at any time.

## Recorded logs

Two fixtures in `core/tests/fixtures/` are real event logs, scrubbed by `scrub` (`core/src/bin/scrub.rs`). Each event keeps its shape and loses its content: text becomes `…`, and lists and objects become empty, except for what stays. What stays: ids, event names, `cwd`, the permission mode, a session's start source and end reason, agent ids and types, the `description` of a spawn or a Bash command, `isolation`, the paths of the files a tool reads or writes, skill names, the types and statuses in tool results, background tasks without their commands, exit codes, numbers, flags and timings. An MCP tool keeps no text. Transcript and output paths become `…`. A prompt keeps only the markup Claude Code adds, with its ids and statuses, so a wake-up stays one; any other tag becomes `<…>`. A last message that asks Daniel something becomes `…?`.

- `checkpoint-day.jsonl`: the events of 2026-10-30, the checkpoint day of #347. Some of its sessions were still open at midnight.
- `subagent-run.jsonl`: a scripted session in a temporary repository that spawns a worktree worker and a read-only helper in the background, then stops the helper.

Re-record the second when Claude Code's version changes. The script runs a nested `claude` on your subscription, so run it yourself, outside auto mode:

    core/tests/record-subagent-run.sh

Quit the studio app first: it would take the session as live and ask for cards. The script builds `studio-hook`, runs the session with this checkout's plugin, and scrubs the session's events from the event log into the fixture. Before you commit the new fixture, run `cargo test` and check that each text it keeps is a label, an id or a path.

The app tells you when the version changed. At a session's first `Stop`, it reads Claude Code's version from the session's transcript, the `.jsonl` file at the event's `transcript_path`, and opens no other file. It keeps each new version, with when it was first seen, in `claude-versions.txt` beside the event log. A version that differs from the last one kept shows a one-line notice at the top of the pane, one line on stderr, and a line in `weekly`. The first version kept has nothing to compare with, so it shows nothing.

## Wake-ups

When a subagent or a background command returns, Claude Code sends its parent a prompt that names the task's id in markup: `<agent-message from="…">` or `<task-notification><task-id>…</task-id>`. A prompt that names a task id the session knows, from a spawn's response, a background command's response or `Stop`'s `background_tasks`, that way is a wake-up, never your prompt. The session's row and its view's "now" line read "worker … returned", with the subagent's name or else the task's short id, until the session's own next step or your next prompt; its last step stays. A card on a wake-up's turn belongs to your prompt before it, so it reads that prompt's earlier cards. A turn a wake-up opened that changed nothing itself gets no card, only a brief updated from its workers' cards. A known id in plain text ("check what a1191 did") is your prompt. A known id in any other markup is your prompt too, logs one line on stderr and counts in `weekly`: Claude Code may have changed its markup.

## The card store

`cards-v2.json` beside the event log: each card with the change it explains (`request.change`: the diff, or on a parent's card written from its workers' cards, their summaries), its base (`request.base`: the `HEAD` sha and the files `git status` listed), the prompt of yours it belongs to (`request.prompt`, when the hook received it; a worker's card, when the worker started), the worker whose card it is (`request.agent`, `null` for the session's own), the files other subagents edited in its worktree (`request.changed_by`, by subagent id), the workers' cards a parent's card was written from (`request.workers`), the command of the check whose start requested it (`request.check`, `null` on any other card), its one-line `summary`, its `architecture` section (`null` when the change is not structural) and its record, in Unix milliseconds: `added_at`, when the card arrived; `shown_at`, when the pane first drew its quiz while the window was visible; and `guess`, your tap: the `option` you tapped (from 0), whether it was `right`, and `at`. Each is `null` until then. `right` is by the writer's key; on a check's card it is by the check's result, `null` until the result settles it and for good when the exit code does not say. On a check's card, `check_result` is that result: `passed`, `failed` or `not_counted`, `null` while the check runs. The app settles it after each read of the event log, once the card is stored and before a tap, so a tap after the result never counts. `key_wrong` is `true` once you flag the key, from the quiz or from your rating. `rating` is your latest rating: its `level` (`good`, `fair` or `poor`) and its `reasons` (`unclear`, `wrong`, `too_shallow`, `too_easy`). `more` is the longer explanation from your first "explain more". A missing store starts empty. Cards stored before quizzes (#375) load with no quiz, and cards stored before briefs (#377) with no summary and no prompt. A card stored without `request.change` loads with none. `briefs.json` beside it keeps each session's and each worker's brief, by session or subagent id, and its count of updates. The app writes each file whole to a temporary file and renames it into place, so a crash mid-write keeps the previous one. On the first start after cards came to cover a prompt's whole change (#374), the app moves the old store, `cards.json`, to `cards-v1.json`, never over one already there, and starts the new store empty; `weekly` does the same.

A check's exit code says whether it passed when nothing in the command can mask it: the check is the command's last statement (after the last `;` or line break), with no `||`, no leading `!` and no `&` that sends it to the background, and a pipe only after a `set` that turns on `pipefail`. Check lines come from the event log, not the store.

## The weekly rates

The app shows no rate, score or tally. Once a week, print the rates of the cards that arrived in the last 7 days:

    cd apps/studio
    cargo run --bin weekly

- thinking rate: of the quizzes you answered, the share you answered 3 s or more after they showed;
- surprise rate: of your graded guesses, the share that were wrong by the writer's key or, on a check's card, by the check's result;
- ratings: how many of the week's cards you rated, by level;
- reasons: how many ratings gave each reason, and how many cards you flagged "the key is wrong", from the quiz or the rating.

Then the drift in Claude Code over the same 7 days, from the event log and `claude-versions.txt`:

- unrecognised wake-ups: the prompts that named a known task id in markup the studio does not know (see "Wake-ups");
- one line per version change, with its date.

A rate with nothing to count prints `n/a`. To record your one-line verdict on the questions, add it after `--`:

    cargo run --bin weekly -- the questions made me stop twice, the rest I clicked through

This appends the rates and your verdict to `weekly.txt` beside the event log. If you mostly click through the questions, or call them a chore, they go and plain cards stay. The thresholds are starting guesses.
