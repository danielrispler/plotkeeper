# Cards

At a session's `Stop`, one card explains what the prompt changed, commits made during the prompt included, when the change is big enough to get one: a turn that changed less gets a lone Quiz or nothing (see [What a turn gets](../question-rules.md#what-a-turn-gets)). When the change is structural, the card has an architecture section. The writer reads the summaries of the earlier cards on your prompt, its wake-ups' cards included, so a structural change spread over several cards still counts.

A worker is explained like a session. It gets a card at each commit in its worktree (its `HEAD` moved, seen after a Bash step) and at its end (`SubagentStop`, or for a killed worker, which has none, the wake-up that reports it), each on its change since its previous card, and its own brief. Click its line to see its brief and cards.

On any step of a worker, or of a prompt of yours before its turn ends, when 20 minutes or more have passed since the last card on that worktree, or since its last such look that found no change, the app looks again: a card is written when it changed, and a look that finds no change starts a new 20-minute wait. So after 20 quiet minutes, a change gets its card up to 20 minutes later.

At a session's `Stop`, when it changed nothing itself but its workers wrote cards since its last card, and the turn's size gets a card, its card is written from their summaries, not their diffs: what it split up, what came back and what is still open. A turn's facts, its files and checks, include its workers'. When a wake-up opened that turn, there is no card of its own. A file a helper, or another subagent with no worktree of its own, edits or writes in its parent's worktree shows on the parent's card under that subagent's name.

A card's code is pulled from git, never retyped by the model: the writer names a path and a line range, and the app reads those lines from the turn's end tree, at most 12 lines of one range; a range whose lines are not in the file is dropped, and a check's card has no code. A card's file chips that are among its turn's files open the same hunk as the file's row in the evidence line, in place.

Under the explanation, "explain more" asks the writer once more, on the card's change and its explanation, and shows the longer explanation under the card; later taps open and close the kept text with no new call. When the writer gives no text, the app logs one line on stderr and the next tap asks again. Rate the explanation: Good, Fair or Poor, then tick its reasons: unclear, wrong, too shallow, too easy, and, on a card with a quiz that is not a check's, the key is wrong, which is the same flag as the quiz's.

The card store keeps the cards, your guesses, flags, ratings and longer explanations across restarts; a turn that ended while the app was down gets its card when the app starts, if it ended in the last 24 hours, changed files and has none.

To tell what a prompt changed, the app fingerprints the session's git worktree at its prompt and at its `Stop`: `HEAD`, and the files `git status` lists, with their content. The card covers the difference: the files listed either time, and the files the commits in between changed. Each card's change is the next one's base, and each session keeps its own, so another session's prompt in the same worktree moves no base. A prompt that lands while the session's turn is still open, after an interrupt, takes no new base, so the interrupted prompt's change stays in the card.

Each subagent gets its own fingerprint too, of the folder of its first event: its base. After each of its Bash steps the app looks again; that look becomes the next base only when `HEAD` moved, so an edit seen before the commit stays in the commit's card. A helper is also fingerprinted right before each Bash step: a change across that step makes it a worker, while what others changed in a shared worktree before it does not. After a restart the app fingerprints the turns still open and the subagents still running, so their next look can be compared.

Ignored files do not count, and a commit alone changes nothing. A file over 1 MiB is compared by its size and modification time, and the card writer gets one line that names it instead of its diff.

The card writer runs `claude -p` on your subscription, with `ANTHROPIC_API_KEY` and `ANTHROPIC_AUTH_TOKEN` removed from its environment, in `card-writer/` beside the event log. A change is untrusted text, so the writer runs with no tools (`--tools ""`) and no MCP servers (`--strict-mcp-config`), whatever your settings allow. It gets at most the first 64 KiB of the change. It also runs without `CLAUDE_CODE_PLUGIN_DIRS`, so its own sessions send no hook events. It writes the card in English, with code in backticks, and shows code left to right. A card stored in Hebrew before the language changed is not rewritten: it keeps its text and shows it right to left.

A card that does not parse is asked for once more; when the second fails too, there is no card and one line on the app's stderr; `open --stderr <file>` keeps it.

An app opened from Finder or `open` does not get your shell's `PATH`, so at start the app asks your login shell where `claude` is: `$SHELL -lc 'command -v claude'`, with `/bin/zsh` when `SHELL` is unset. Only the path comes back; the writer keeps the app's own environment. A login shell that is not interactive reads `~/.zprofile` but not `~/.zshrc`, so the line that puts `claude` on your `PATH` must be in the first. With no answer, the app runs `claude` from its own `PATH`.

To run the app with a fake writer instead of `claude -p`:

    open --env STUDIO_CARD_WRITER=fake target/release/bundle/macos/Studio.app

To run the real writer on two fixture changes, one structural and one not, run this from the repository's root, outside auto mode. For each change it prints the raw reply and the card parsed from it. Then it prints the Goal written from the prompts, the second card's "explain more", and a parent's card from two workers' card summaries:

    cargo run -q -p studio-understanding --example real_card

## Wake-ups

When a subagent or a background command returns, Claude Code sends its parent a prompt that names the task's id in markup: `<agent-message from="…">` or `<task-notification><task-id>…</task-id>`. A prompt that names a task id the session knows, from a spawn's response, a background command's response or `Stop`'s `background_tasks`, that way is a wake-up, never your prompt. The session's row and its view's "now" line read "worker … returned", with the subagent's name or else the task's short id, until the session's own next step or your next prompt; its last step stays.

A card on a wake-up's turn belongs to your prompt before it, so it reads that prompt's earlier cards. A turn a wake-up opened that changed nothing itself gets no card, only a brief updated from its workers' cards. A known id in plain text ("check what a1191 did") is your prompt. A known id in any other markup is your prompt too, logs one line on stderr and counts in `weekly`: Claude Code may have changed its markup.

## Blocked commands

In a Claude Code session in auto mode, a Bash command that auto mode blocks raises a card on the session's view, docked above the open questions. It shows the command in full, its folder, the rule that blocked it and, for a subagent's block, the subagent's name. Cards stack in block order. A block of any other tool keeps Claude Code's own notice, and Codex and agy keep their own prompts. The card has Allow and Decline and no edit, so what runs is what the card shows.

The plugin's `PermissionDenied` hook for Bash waits up to 600 seconds for your click while the studio's window shows life, whichever session it shows, or none. The session needs you until your click or the agent's next event. The card reads Blocked.

- Allow reads Allowed. The hook returns `retry: true`, and the agent retries the exact command in its own Bash tool. The studio grants a one-time pass for that command and folder, good for 30 minutes: the retry's `PreToolUse` claims it and asks, and the `PermissionRequest` after it is allowed once, whichever session the studio shows, while the studio runs. A retry that is re-quoted, in another folder or another session, expired, or already used, gets no pass and goes back to the classifier.
- Decline closes the card and the denial stands.

Each Allow or Decline enters the session's decision history, as given on the card.

When the studio's window shows no life, the hook returns at once with no decision, and when no click comes by 600 seconds it returns none too. Either way the card stays, with Allow and Decline, until you choose, and if the studio refuses an answer the card keeps its buttons. A decision the hook could not write to the event log leaves the card's request in place too. The session needs you only until the agent's next event or your choice, then shows its real state, and its row reads "N blocked commands waiting" (see [Sessions](sessions.md)). An Allow after the hook has returned still grants the pass (the only one for that command and folder) and enters the decision history, which the studio logs itself, since no hook is left to. The agent does not retry by itself then, so, when the studio's answer found the hook gone, the studio types a retry line into the session's terminal, as a bracketed paste and then Enter, through the same typing as `/rename`: it waits for your unsent draft, and never types into a session that needs you, showing a notice instead. For the session's own block the line reads "Daniel approved the blocked command: `<command>`. Run it again." For a subagent's block it reads "Daniel approved the command subagent `<name>` was blocked on. Run `cd <folder>` as its own Bash call, then run exactly: `<command>`. Or pass it to the subagent if it still runs." The retry after that `cd` runs in the subagent's folder, so it matches the pass. A line typed while the agent is mid-turn is queued by Claude Code, and the card moves to Running at the retry whenever it comes.

### After Allow

The card follows the run. The core matches the agent's retry by command and folder:

- **Running** at the `PreToolUse` of the same command and folder, with the time since it started, which counts up each second while the card shows. Live output stays in the session's terminal.
- **Finished** at its end, with its exit code and the last 40 lines of its output, read as a failed check's are.
- **Running in background** when the agent backgrounded the call, until the wake-up that names its task, which finishes the card. The wake-up's summary, as in `Background command "…" completed (exit code 0)`, gives its exit code, which the Finished card shows; a summary without one reads "No exit code". The card has no tail for a background run: its output is in the task's output file, which the studio does not read.
- **Blocked again** when a new `PermissionDenied` of the same command follows the Allow, or, for a Running card, when it is that retry's own call that was denied. A same-command block by another call gets a card of its own. It is the same card, with Allow and Decline once more, and the session needs you again.
- **Allowed, not run** when the turn ends with no matching call, so the approval went unused: the session's own turn ending covers its own Allowed cards and those of subagents that have ended, and a running subagent's card waits for that subagent's own end, since its worker may retry by itself. The row still counts the card as waiting. It shows **Ask again** and nothing else. Ask again writes a fresh pass for that command and folder, replacing any earlier one, since the 30-minute pass may have expired or been spent, then types the same retry line. A retry that comes later still runs the card.
