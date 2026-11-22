# Web UI coupling: serving `ui/` from a local server

Read-only investigation, main checkout, 2026-11-22. Everything cited was read; items marked (inferred) are my reading, not checked by running anything.
Paths relative to repo root. `M` = `src-tauri/src/main.rs`, `T` = `terminal.rs`, `S` = `settings.rs`, `A` = `answer.rs`, `R` = `restart.rs`.

## 1. Commands and events

The window's contract is `interface Commands` (`ui/types.ts:449-531`): 82 commands. Seam is `ui/dom.ts:8-44`. Events the UI listens to (grep of `listen<..>("..")` in `ui/*.ts`): 12 names plus the Tauri-native `tauri://drag-drop`.
"Logic" = where the real work is. Almost every handler is a thin wrapper over a shared `Arc<Mutex<..>>` (`Core` = `Studio` from `core`, `Cards` = `CardStore`, `Notes`, `Writer`, `TurnWriting` from `understanding`; `M:30-58`, `M:1100-1135`).

### Learning layer (cards, quizzes, summaries, rail/story, Home, system map)
| command | handler | does | logic |
|---|---|---|---|
| cards | M:268 | `SessionCards` for viewed session (+risk notes) | understanding (CardStore) + inline join |
| shown | M:464 | quiz drawn in view | understanding |
| guess | M:471 | record guess | understanding |
| quiz_tap | M:489 | tap on option, returns summary | understanding |
| key_wrong | M:521 | flag key wrong | understanding |
| rate | M:527 | rating | understanding |
| explain_more | M:534 | longer explanation via writer | understanding |
| learning | M:498 | Home learning slot | understanding |
| ask | M:543 | side question to writer | understanding |
| ask_facts / ask_anchor / ask_why_not | M:102 / 571 / 636 | facts and answers for ask UI | understanding + core |
| turn_summary / turn_reply / agent_summary | M:147 / 154 / 162 | headline+body, reply text, worker summary | understanding (TurnWriting) |
| turn_file_hunk / hunk_opened | M:114 / 136 | diff hunk, mark opened | understanding (`turn_hunk`) |
| check_lists / check_output | M:73 / 81 | project check lists, output | core/understanding |
| summary_settings | M:59 | writer model/effort setting | inline |
| mark_unseen / split_shown / rail_shown | M:1764 / 1572 / 1562 | seen-tracking flags (statics `RAIL_SHOWN`, `SPLIT_SHOWN`) | inline statics |
| refresh_landed_tasks | M:1652 | recheck landed tasks (gh) | workbench (`losses`, `gh_path`) via inline |
| watch / alive / answer / ask_again / answer_blocked_command | A:188 / 197 / 205 / 177 / 166 | answer-from-card: heartbeat + reply files for the hook | `hook::answer` + inline `Watching` |

No "system map" command exists: the map is a repo file and a crate (`system-map/`), not in the window (grep of `Commands` shows none). (verified by the list above)

### Session view
sessions (M:187, list rows), select (M:210), select_terminal (M:222), open (M:231, open a subagent), selected (M:257), subagent_log (M:243), rename (M:657), delete_session (M:669), clearing (M:691, undo-delete queue; committed on app exit `M:1296`), closed (M:781, close terminal). All: core `Studio` + inline.

### Workbench
- terminals: terminal_connect (T:44), terminal_send (T:123), terminal_lost (T:110), terminal_upgrade (T:117), revival (R:24), restart_into (R:81), claude_argv (T:294), agent_argv (T:302), agents (T:309). Logic: `terminal-daemon` client inline in `src-tauri`.
- projects/worktrees: projects (T:153), add_project (T:183), remove_project (T:189), reorder_projects (T:195), suggest_projects (T:201), repository_of (T:169), choose_folder (T:175, native dialog), workspaces (T:208), workspace_git_state (T:215), fetch_repository (T:232), pull_main (T:238), create_task (T:244), create_named_task (T:250), remove_task (T:259), delete_merged_branch (T:269), branches (T:279), switch_branch (T:285). Logic: `workbench` crate (git, worktrees) plus inline project file.
- plugin setup: plugin_setup (T:345), fix_plugin_setup (T:404), agy_plugin_setup (T:461), fix_agy_plugin_setup (T:485). Inline.
- skills (skills.rs:155, reads `enabledPlugins`), settings_files (S:23), write_setting (S:209), open_settings (S:59), open_keymap (S:65), set_zoom (S:247), usage (M:310, workbench `claude_usage`/`cli_usages`), sounds (M:1775), reveal_log (M:1023), open_link (T:493), open_file (T:509).

### Events (Rust emit -> UI listen)
sessions (M:413,1693 -> main.ts:363), selected (M:418 -> main.ts:377), cards (M:443 -> main.ts:379), finished (M:1541 -> main.ts:370), notice (M:1419,1432 -> main.ts:381), writer-notice (M:1259 -> main.ts:386), retests (M:513 -> panes.ts:89), ask (M:1484 -> answer.ts:94), blocked (M:1493 -> answer.ts:95), settings-files (S:239 -> main.ts:297), terminal (T:78,82 -> daemon.ts:34), new-build (R:45,73 -> restart.ts:44), plus `tauri://drag-drop` (terminal.ts:248, Tauri-native). Also `log://log` from the log plugin (snap stub, `scripts/snap/page.mjs`).
Pattern: the read loop `follow` (M:~1300-1560) polls every 200 ms and emits only on change (`Sent`, M:~345-445). This is already push-on-change, so it maps to SSE directly.

## 2. Native-only dependencies

| thing | where | in a browser |
|---|---|---|
| xterm terminals over the daemon | `ui/terminal.ts`, `daemon.ts`, `panes.ts`; `T:44-150` Unix socket client | Needs a WebSocket bridge to the daemon socket; but v1 learning+session drops terminals. Drops |
| Drag-drop of files | `terminal.ts:248` | Drops (terminal only) |
| Focus/presence file | `presence.rs` (90 lines), `M:1223`, `M:1240`; `focused()` in `follow` M:~1500 | Page Visibility/`document.hasFocus()` posted to server; or drop. Drives Remote Control phone-push suppression and "seen" logic (`see_selected`) |
| macOS notifications + Dock badge | `M:1546` (`app.notification()`), `set_badge_count` (M:~410) | Web Notifications API (permission prompt), `document.title` count; or drop |
| Sounds | `play` M:1780 (`afplay`) | `Audio` in page, or server still `afplay` (works without Tauri) |
| File dialog | `choose_folder` T:175 (`tauri-plugin-dialog`) | No folder picker in a browser: text input, or server-side `osascript`/`open` dialog. Workbench only |
| `open`/editor/Finder | `open_link` T:493, `open_file` T:509 (`/usr/bin/open`, Zed) | `<a target=_blank>` for links; `open_file` stays a server call (server runs the editor CLI). Daniel is moving to an IDE, so this is plausibly a fixed editor command |
| Restart into new build | `restart.rs`, `ui/restart.ts` (145) | Drops (app-binary self-update) |
| Single instance | `tauri-plugin-single-instance`, `M:1117` | Replaced by a port/pid lock |
| Window zoom, drag region, overlay title bar | `set_zoom` S:247, `index.html:21 data-tauri-drag-region`, `tauri.conf.json` | CSS zoom / browser zoom; drop |
| WKWebView hover delay hack | `hover_delay` M:~1794; `ui/hover.ts` | Drops |
| NSUserDefaults/objc2 | `src-tauri/Cargo.toml` deps `objc2`, `dispatch2` | Drops |
| `tauri-plugin-log` | `studio_log()`; `dom.ts:logLine` calls `window.__TAURI__.log` | Replace with `fetch('/log')` or console |
| Keys (Cmd-table, keymap) | `ui/keys.ts`, `settings.ts` | Plain DOM `keydown` (no global keys found); browser steals some Cmd combos (inferred) |
| Unseen/"on screen" logic | `RAIL_SHOWN`, `SPLIT_SHOWN` statics | Reports through the same calls; keep |

No file in `ui/` references Tauri except `dom.ts`, `terminal.ts:248` and `index.html:21` (grep of `__TAURI__|tauri`).

## 3. Process model

- The Tauri app process owns everything: `main()` (M:1085-1300) builds `Studio` (core state, replays the log), `CardStore`, `RiskNotes`, the writer (`ClaudeWriter` = `claude -p`, or `FakeWriter` when `STUDIO_CARD_WRITER=fake`), `TurnWriting` (turn summaries; queue), then runs `follow` on a thread (M:1264), plus threads `prs` (M:1248, gh PR lookups), `write_due_retests` (M:1250, hourly), the writer-notice forwarder (M:1252), `restart::watch_build`, `settings::watch`, and per-judge threads for risk notes (M:~440).
- Event log: the hook (`hook/src/main.rs`) appends JSON lines to `~/Library/Application Support/dev.falinks.studio/events.jsonl` (`hook/src/lib.rs:25-31`; the path is literally Tauri's identifier). The app reads it by polling (M:~1300).
- **Hook works without the app**: `hook/src/main.rs:14-36` appends a line, always exit 0, no app contact. `studio-answer` (`hook/src/answer.rs:319-365`) writes an ask file only if `watched(data, session)` (an app heartbeat is fresh), otherwise returns None and the agent falls to its terminal prompt. Events recorded while the app is closed are replayed on next start (replay doc comment M:1303-1312); replayed lines get no cards/notifications except for turns of the last 24 h with no card. So the "no app open" lifecycle is already tolerant: events queue in the log.
- The card store and risk notes are saved on disk beside the log (`CardStore::open_beside`, M:1086-1092); the card writer is spawned by the app process, so with no app no cards are written.
- Terminal daemon (`terminal-daemon/`) is already a separate background process that outlives the app (system-map law `terminals-outlive-app`, `system-map.json:91`). It is started by the app on first `terminal_connect` ("starting it if none runs", `ui/daemon.ts:30`).
- Would have to become a standalone daemon/server: the whole of `main()` setup + `follow` + `prs` + retests + the commands (about 25 of the 82 that matter for v1) + an HTTP/SSE layer. Who starts it with no app open: nothing does today (no launchd agent); the natural v1 choice (inferred, not decided) is "started by the user (`npm run start`-like command) and by the hook if absent", which is a new lifecycle piece. The data dir path, if the Tauri identifier is gone, should move out of `tauri.conf.json` (it is hard-coded as `dev.falinks.studio` at `hook/src/lib.rs:25`, harmless but also `restart.rs`/probe docs reference it).

## 4. What drops if Tauri is frozen and v1 = learning layer + session view

Line counts are `wc -l`.
- `src-tauri/`: `src/` 4456 (main 2169 incl ~355 test lines, terminal 794, settings 582, answer 446, skills 284, restart 91, presence 90), `Cargo.toml` 30, `tauri.conf.json` 27, `build.rs` 3, `capabilities/`, 5 files under `gen`+`icons`. But about 60% of `main.rs` is not Tauri-specific (follow, Sent, ShellEffects, prs, handlers over Core/Cards): that logic moves, it does not drop.
- Crates: `workbench/` 3768 lines (the 1394 lines in its `src/` that I listed are usage/git/PR; used by terminals, tasks, usage, and by `refresh_landed_tasks` and the PR lookups that feed the session list: partly keep), `terminal-daemon/` 2056 + 751 tests (drops with terminals; but the daemon's OSC 7501 status reaches the core via `listed`, M:872, so terminal-linked session status stops updating: check), `terminal-spike/`, `v0-spike/` already spike dirs. `wayfinder/` 2043 is unrelated.
- UI drops (about 3300 lines of 9387): `terminal.ts` 272, `daemon.ts` 58, `panes.ts` 469, `keys.ts` 185, `picker.ts` 409, `workspaces.ts` 578, `projects.ts` 75, `skills.ts` 61, `settings.ts` 273, `settings-page.ts` 315, `usage.ts` 158, `restart.ts` 145, `paths.ts` 93, `hover.ts`/`hovers.ts` 120; `list.ts` 599 and `main.ts` 404 need splitting, not dropping. npm deps: `@xterm/*` (7 packages) and `@tauri-apps/cli`; the `build` script copies xterm into `dist/vendor` (`package.json:6`).
- CI (`.github/workflows/studio.yml`, 62 lines): the `macos` job comment "Tauri app built and used on macOS only" (line ~25); `npm run check` (`package.json:15`) runs `cargo clippy/nextest --workspace`, which compiles `src-tauri` (needs `dist/` first; hence the "UI first" order). Without Tauri the workspace-wide compile gets much faster, `default-members` excludes it already (`Cargo.toml:5`). Can move to ubuntu runners only if everything else is platform-neutral (`hook` uses `std::os::unix`, `afplay`, macOS paths: inferred still macOS). `snap` job unchanged (`snap` uses stub, `scripts/snap/page.mjs`), though stub must be re-pointed at the new transport.
- Scripts: `scripts/snap/*` (2137 lines incl. fixtures) keeps, but `page.mjs` stub rewrites; `record-terminal.sh`/`terminal-recording.txt` (584) drop; `scripts/wire/samples.rs` unchanged.
- Docs: `docs/agents/app-probe.md` (57) drops (probe = debug Tauri build); `docs/studio/behaviour/terminal.md` (44), terminal parts of `docs/studio/development.md` (79), `docs/studio/settings.md` (55, partly). `system-map.json`: parts `shell`, `terminal-daemon`, `window` and links/laws (`terminals-outlive-app`, ..., `system-map.json:23,36-38,59,91`) change; `system-map check` fails until they do.
- AGENTS.md rules that depend on the app: "Implementation language: Tauri 2 stack is settled" (line 27) needs revisiting; "Checks" paragraph, "The app embeds `dist/`... tell him to quit Studio and run `npm run start`" (line 33; `npm run start` = `tauri build && open ...Studio.app`, `package.json:12`); app-probe pointer (line 35); "Blocked commands go to Daniel" (Workflow section: relies on studio card and Allow/Decline; the card is the learning-layer `blocked.ts` 237 + `answer.ts` 134 + `hook::answer`: this one survives in the web page if the answer path is carried, otherwise it falls back to the terminal prompt, because `watched` is false); "run `npm run check`... builds the UI first, which the Tauri crate needs" (line 31); `CARGO_BUILD_JOBS` guidance stays.
- README.md is read by `ui/tests/readme.test.mjs`.

## 5. Effort shape

Minimal transport swap:
1. `dom.ts`: replace `invoke` with `fetch('/api/<command>', POST json args)` and `listen` with one `EventSource('/events')` that fans events to handlers by name (`dom.ts:21-24` is the only change on the UI side; `logLine` uses `fetch('/log')`; `terminal.ts:248` and `index.html:21` removed). The `Commands` typed map (`ui/types.ts:449`) can stay the contract. Snap stub (`page.mjs`) keeps the same API shape if `dom.ts` accepts an injected transport.
2. Server (a new binary, e.g. axum or tiny_http; neither is in the workspace today): `GET /` static `dist/`, `POST /api/*` dispatching to the same functions, `GET /events` SSE fed from the same `emit` points (a `broadcast` channel replacing `AppHandle::emit`).
3. Reachability of logic: **tangled but extractable.** Domain crates (`core`, `understanding`, `workbench`, `hook`) have no Tauri dependency (src-tauri is the only crate that depends on `tauri*`, `src-tauri/Cargo.toml`). But the glue is in `src-tauri`: `follow` read loop, `Sent` diffing, `ShellEffects`, `prs`, retests, notices, and the handlers take `tauri::State<..>`/`AppHandle`. `M` 2169 lines would need to be split into a Tauri-free `app` library crate with an `Emit` trait, then both a thin Tauri shell (frozen) and a server could use it, or the Tauri shell is simply forked/frozen and the library copied (inferred).

Hardest 3:
1. **Extracting the shell glue from `AppHandle`/`State`**: `follow` and `Sent::list/cards` read the window (`get_webview_window`, `is_focused`, `set_badge_count`, `app.emit`, `app.notification()`), M:~410, 1500, 1546; `ShellEffects` holds an `AppHandle` (M:1688). Needs an effects/emit abstraction before any server exists.
2. **Process lifecycle with no app**: today nothing but the app writes cards, runs the card writer, `prs`, retests, or heartbeats the answer files. Deciding who starts/keeps the server (and what happens to the answer-from-card path when no browser tab is open: `watched` goes stale, falls back to terminal, which is fine) is a new design piece, plus single-instance and data-dir (`dev.falinks.studio`) ownership; and the daemon's terminal status feed into the core (`listed`) if terminals drop.
3. **Presence/focus/seen semantics**: "seen", notifications-only-when-unfocused, `see_selected`, Dock badge and the Remote Control presence file all depend on window focus (`M:1223-1242`, `presence.rs`, `follow` `focused()`); a browser tab gives visibility events only per tab, and multiple tabs/no tab needs defining. The notification and sound path need re-homing (server `afplay` fine; notifications via Web API or osascript).
