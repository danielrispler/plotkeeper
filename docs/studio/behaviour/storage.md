# Storage

## The event log

`~/Library/Application Support/dev.falinks.studio/events.jsonl`, one hook payload per line, with two added fields:

- `studio_terminal_id`: the session's `STUDIO_TERMINAL_ID`, when set;
- `studio_received_ms`: when the hook received the event, in Unix milliseconds.

The hooks are async, so Claude Code never waits for them, and they append whether the app runs or not. Two events that fire within a few milliseconds can land in either order.

The log holds tool inputs and results: commands and their output, errors, exit codes, the text an edit replaced and its hunks, and a written file's content. It leaves out two fields no reader takes (danielrispler/plotkeeper#132): an Edit's or Write's `originalFile`, the whole file before the change, and a Read's file `content`.

It keeps a session's events for 30 days after its last one. At each start, before the replay, the app moves the events of older sessions to `events-archive.jsonl` beside it, and drops the two fields from the lines it keeps; a prune moves lines, so it also removes `studio-state.json`, the state a replay made, which the next start saves again and later starts resume from, replaying only the log's tail. The app never reads the archive, and never deletes it; their turns stay in the card store.

The app also keeps `roots.json` beside the log: the git worktree root of each folder it has looked up, so a start runs `git` only for new folders.

## Plan usage

Claude Code gives the plan's 5-hour and weekly usage, and the context window's size, only to the status line command, on its stdin. Sessions the studio starts run `claude --settings` with `workbench/statusline.sh` as their status line, so you edit nothing. On every refresh it saves that stdin as `usage/<session_id>.json` beside the event log, by temp file and rename, readable only by you (mode 0600, in a folder of mode 0700), with the line in `workbench/statusline-usage.sh`.

Then it reads `statusLine.command` from `~/.claude/settings.json`, runs it on the same stdin, and passes its output and exit status through; with none set, it prints nothing. The top bar's usage button shows the tightest window across the agents, as that agent's logo and "5h 62%" or "wk 80%", or "Usage" when none has data; a click opens a popover with a tile for each agent: its windows with the share used and the time to reset, and when and from where they were read, or the reason it has none. Claude's snapshot is read on load, on focus and every 10 seconds; the other agents' usage is read only when the popover opens. Sessions started outside the studio save no snapshot. Snapshots are never deleted; remove the folder at any time.

## The card store

`cards-v2.json` beside the event log: each card with the change it explains (`request.change`: the diff, or on a parent's card written from its workers' cards, their summaries), its base (`request.base`: the `HEAD` sha and the files `git status` listed), the prompt of yours it belongs to (`request.prompt`, when the hook received it; a worker's card, when the worker started), the worker whose card it is (`request.agent`, `null` for the session's own), the files other subagents edited in its worktree (`request.changed_by`, by subagent id), the workers' cards a parent's card was written from (`request.workers`), the command of the check whose start requested it (`request.check`, `null` on any other card), its one-line `summary`, its `architecture` section (`null` when the change is not structural) and its record, in Unix milliseconds: `added_at`, when the card arrived; `shown_at`, when the pane first drew its quiz while the window was visible; and `guess`, your tap: the `option` you tapped (from 0), whether it was `right`, and `at`.

Each is `null` until then. `right` is by the writer's key; on a check's card it is by the check's result, `null` until the result settles it and for good when the exit code does not say. On a check's card, `check_result` is that result: `passed`, `failed` or `not_counted`, `null` while the check runs. The app settles it after each read of the event log, once the card is stored and before a tap, so a tap after the result never counts. `key_wrong` is `true` once you flag the key, from the quiz or from your rating.

`rating` is your latest rating: its `level` (`good`, `fair` or `poor`) and its `reasons` (`unclear`, `wrong`, `too_shallow`, `too_easy`). `more` is the longer explanation from your first "explain more". A missing store starts empty. Cards stored before quizzes (danielrispler/Falinks#375) load with no quiz, and cards stored before briefs (danielrispler/Falinks#377) with no summary and no prompt. A card stored without `request.change` loads with none.

`briefs.json` beside it keeps each session's and each worker's brief, by session or subagent id: its Goal and the count of your prompts it was written at, its "Design so far" with the cards and turns it cites (Decisions come from `decisions.json`, and Open is computed, not stored). `summaries.json` keeps each turn summary (its Headline, typed lines and Quiz), the facts and hunks it was written from, and every tap on its Quiz with its time, so a Quiz's pick, skip, reveal, chips, confidence, "why?" line and its self-test arm survive a restart; `retests.json` keeps the retests the same way.

A card's own taps are in the card. The app writes each file whole to a temporary file and renames it into place, so a crash mid-write keeps the previous one. On the first start after cards came to cover a prompt's whole change (danielrispler/Falinks#374), the app moves the old store, `cards.json`, to `cards-v1.json`, never over one already there, and starts the new store empty; `weekly` does the same.

## The Studio log

`~/Library/Logs/dev.falinks.studio/studio.log` holds the app's own errors, warnings and panics, one line each with a local time, a level and the module that wrote it. The UI's failures are in it too: a failed call from the window to the app is a line with the command's name and the error's message, and an uncaught error or unhandled rejection in the page is an `error` line. The same lines print to stderr. At most 500 KB a file, with one dated older file kept when it fills. Each start writes an `info` line with the version, and the **Mismatch log** lines are `info` lines with the target `mismatch`. A panic is logged at `error` with its location, then reaches the default panic output. The studio's own crates log at `Debug` and libraries at `Warn`, in every build. "Reveal log" in the palette opens Finder with the file selected.

An `error` line, from the app or from the page, also shows in the window's notice line, in every build, beside the settings errors and the Claude Code version notice. A message shows once, however often it repeats, until Daniel dismisses it: the line's "Dismiss" button hides it and clears its error lines, and a later error shows it again. Warnings stay in the file only.

An **error** is lost data or a failed action Daniel asked for: a card store, saved state or worktree roots that did not save, a name that did not save, a question that got no answer. A **warning** is the studio degraded but carrying on: a skipped prune, a missing `installed_plugins.json`, a presence file not written, a card or summary the writer did not give. Lines from the card writers' code are errors when they say a save failed and warnings otherwise.

A line holds ids, paths, error messages, exit codes and counts. It never holds card text, prompts, transcripts, answer contents, file contents, anything read from `~/.claude*`, or a token. A line from the writers' code is cut at the first quoted text, so a reply's words never reach the file.
