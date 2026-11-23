# Storage

## The event log

`~/Library/Application Support/plotkeeper/events.jsonl`, one hook payload per line, with added fields:

- `studio_terminal_id`: the session's `PLOTKEEPER_TERMINAL_ID`, when set;
- `studio_agent`: the agent its hook entry named (`--agent claude`, `--agent agy`, `--agent codex`, `--agent qwen`, `--agent cursor`), by which the core picks the line's parser and a prune finds its session (#355). A name plotkeeper has no parser for is counted as drift, not read as Claude's; a line with no name, written before entries named one or by the app, is read by its shape;
- `studio_received_ms`: when the hook received the event, in Unix milliseconds.

The app appends lines of its own, named `Studio…`, such as `StudioInterrupted`, which ends a turn Daniel interrupted, by its `prompt_id`, with that turn's end trees.

The data folder was `~/Library/Application Support/dev.falinks.studio/` before plotkeeper. At the first start of the hook or the app, if that folder exists and `plotkeeper/` does not, it is renamed to `plotkeeper/` and a symbolic link to it is left at the old path, so the plugin path in `~/.zshrc` (through the folder's `checkout` link) and a hook built with the old path keep working. The move never goes over an existing `plotkeeper/`.

The hooks are async, so Claude Code never waits for them, and they append whether the app runs or not. Two events that fire within a few milliseconds can land in either order.

The log holds tool inputs and results: commands and their output, errors, exit codes, the text an edit replaced and its hunks, and a written file's content. It leaves out two fields no reader takes (danielrispler/plotkeeper#132): an Edit's or Write's `originalFile`, the whole file before the change, and a Read's file `content`.

It keeps a session's events for 30 days after its last one. At each start, before the replay, the app moves the events of older sessions to `events-archive.jsonl` beside it, and drops the two fields from the lines it keeps; a prune moves lines, so it also removes `studio-state.json`, the state a replay made, which the next start saves again and later starts resume from, replaying only the log's tail. The app never reads the archive, and never deletes it; their turns stay in the card store.

The app also keeps `roots.json` beside the log: the git worktree root of each folder it has looked up, so a start runs `git` only for new folders.

## The hooks' binaries

`bin/` in the data folder, `~/Library/Application Support/plotkeeper/bin/`, holds copies of `plotkeeper-hook` and `plotkeeper-answer`, so the hooks run them at a path that never moves (#356). Each `plotkeeper` start compares each copy with the binary of the same name beside its own executable (its real path, through any symbolic link) and replaces one that differs, writing a temporary file in `bin/` and renaming it into place, so the path holds the old binary or the new one and never part of one. A start with no `bin/` creates it; equal copies are left untouched. An upgrade so reaches the hooks at the next start, with no install rerun and no npm install script. They are copies, not symbolic links, so removing or upgrading a Node version cannot break or stale them. When either binary is missing beside the executable, the start says so in one line on stderr and touches no copy, so the hooks never run two builds' binaries together. `npm run start` installs `plotkeeper-hook` and `plotkeeper-answer` beside `plotkeeper`, as an npm install lays them out.

## Plan usage

Claude Code gives the plan's 5-hour and weekly usage, and the context window's size, only to the status line command, on its stdin. Sessions the studio starts run `claude --settings` with `workbench/statusline.sh` as their status line, so you edit nothing. On every refresh it saves that stdin as `usage/<session_id>.json` beside the event log, by temp file and rename, readable only by you (mode 0600, in a folder of mode 0700), with the line in `workbench/statusline-usage.sh`.

Then it reads `statusLine.command` from `~/.claude/settings.json`, runs it on the same stdin, and passes its output and exit status through; with none set, it prints nothing. The page has no usage button or popover (danielrispler/plotkeeper#326): the snapshot is still saved, and the Tauri app's top bar read it. Sessions started outside the studio save no snapshot. Snapshots are never deleted; remove the folder at any time.

## The card store

`cards-v2.json` beside the event log holds the cards written before cards were retired (danielrispler/plotkeeper#348). No build writes one now, and the file still loads, with each card's change (`request.change`), its base, its prompt, its worker, its `summary` and `architecture`, and its record: your guess, flags, rating, taps, the check's result and the longer explanation you asked for. A store from before quizzes (danielrispler/Falinks#375) loads with no quiz, one from before briefs (danielrispler/Falinks#377) with no summary and no prompt, and a card stored without `request.change` with none. Nothing shows these cards; the studio reads them for the session's row line and for "Design so far", which may cite one. A missing store starts empty.

`briefs.json` beside it keeps each session's and each worker's brief, by session or subagent id: its Goal and the count of your prompts it was written at, its "Design so far" with the turns (and, in older lines, the cards) it cites (Decisions come from `decisions.json`, and Open is computed, not stored). `summaries.json` keeps each turn summary (its Headline and typed lines; a summary stored before danielrispler/plotkeeper#347 also holds its Quiz and the taps on it, which still load), the facts (with an unused `external` of 0, kept so that the previous build can read the file) and hunks it was written from, and the self-test arm of a summary written while that test ran (retired in danielrispler/plotkeeper#348). `retests.json` holds the retests earlier builds wrote; no build writes one now (danielrispler/plotkeeper#347), and the file still loads.

The app writes each file whole to a temporary file and renames it into place, so a crash mid-write keeps the previous one. On the first start after cards came to cover a prompt's whole change (danielrispler/Falinks#374), the app moves the old store, `cards.json`, to `cards-v1.json`, never over one already there, and starts the new store empty.

`risk-notes.json` beside it, a store of its own that the card store does not read (danielrispler/plotkeeper#298), keeps the risk notes of blocked commands: by session id, then by a hash of the command and its folder (the command, a NUL byte and the folder, hashed with FNV-1a, 16 hex digits), the note's `level` (`none`, `low`, `medium`, `high`, `very_high` or `not_sure`), its `line` and `at`, when it arrived, in Unix milliseconds. It is keyed so, and not by a card's call id, because a card's call id changes when it is Blocked again and a second card of the command and folder has another: they all show the one note. The notes are joined to the cards when the shell reads the cards, so the core's blocked-command type holds none. A failed judge call is not stored: the card reads that no note came until the next start of the app, which asks again. The file is a cache, so one that does not parse opens as no notes, and the next note writes it whole. Its failure lines in the Studio log are cut at the first quote mark, as the writers' other lines are, so no reply's words reach the log, and a line never holds the command.

## The Studio log

`~/Library/Logs/dev.falinks.studio/studio.log` holds the app's own errors, warnings and panics, one line each with a local time, a level and the module that wrote it. The UI's failures are in it too: a failed call from the window to the app is a line with the command's name and the error's message, and an uncaught error or unhandled rejection in the page is an `error` line. The same lines print to stderr. At most 500 KB a file, with one dated older file kept when it fills. Each start writes an `info` line with the version, and the **Mismatch log** lines are `info` lines with the target `mismatch`. A panic is logged at `error` with its location, then reaches the default panic output. The studio's own crates log at `Debug` and libraries at `Warn`, in every build. "Reveal log" in the palette opens Finder with the file selected.

An `error` line, from the app or from the page, also shows in the window's notice line, in every build, beside the settings errors and the Claude Code version notice. A message shows once, however often it repeats, until Daniel dismisses it: the line's "Dismiss" button hides it and clears its error lines, and a later error shows it again. Warnings stay in the file only.

An **error** is lost data or a failed action Daniel asked for: a card store, saved state or worktree roots that did not save, a name that did not save, a question that got no answer. A **warning** is the studio degraded but carrying on: a skipped prune, a missing `installed_plugins.json`, a presence file not written, a card or summary the writer did not give. Lines from the card writers' code are errors when they say a save failed and warnings otherwise.

A line holds ids, paths, error messages, exit codes and counts. It never holds card text, prompts, transcripts, answer contents, file contents, anything read from `~/.claude*`, or a token. A line from the writers' code is cut at the first quoted text, so a reply's words never reach the file.
