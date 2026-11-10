# Round 5: Daniel's final checks

Every check in round 5 (#517) that needs Daniel, collected in one place. Daniel decided on 2026-11-09 that he does them all at once at the end: he upgrades to the new build, uses it, and goes through this list. A ticket that adds a by-hand check adds a line here.

Between passes, nothing waits for him. After each pass, a code-review agent that did not write the code reviews it. Once its fixes are in and the automated checks pass, the pass merges to `main`.

## Real model calls

Run once, outside auto mode, from any checkout:

```sh
apps/studio/real-calls.sh
```

It writes every output to `local-evidence/2026-11-09-round-5/real-calls-<time>/`, with its own terminal output in `run.log`, and it skips a step whose code is not built yet. It runs:

- [ ] **#518:** the 5-run writer timing. Record the median and slowest on #518, beside #471's median of 20.1 s and slowest of 24.4 s. If safe mode made the writer much slower, that triggers grilling (pass plan).
- [ ] **#518:** real cards; their text carries no plugin footer. The script greps for one.
- [ ] **#526:** Haiku summaries on 3 real turns. Judge them, and record their time and cost on #526. Weak summaries reopen decision 14 (pass plan).
- [ ] **#543:** can macOS kill the terminal daemon? It builds a separate app, `StudioRepro`, with its own identifier, home, socket and data folder, launches it through `open`, and ends it by Quit, `kill -9` and `kill -SEGV`. After each, it checks the daemon and both terminals' programs are alive: `543-daemon-kill.json`, with the daemon's log in `543-daemon.log`. The crash may show a "quit unexpectedly" dialog; dismiss it. Record the three results on #543; a killed daemon means #555 adds the launchd job. Memory pressure is not tested.

## In the running app

- [ ] **#518:** one card written by the app itself, with no plugin footer.
- [ ] **#519:** a session row and the rail show `context N%` with the same number as Claude's status line, and the hover reads "… tokens used · Claude's status line shows this as M% left".
- [ ] **#520:** a finished turn's intention is its final reply's first line, not a footer.
- [ ] **#521:** in a session the studio started, `--settings` wins over the `statusLine` in `~/.claude/settings.json`: `ps -o args= -p <claude pid>` shows `--settings` with `workbench/statusline.sh`, and the session's `usage/<session_id>.json` updates. Daniel's status line looks the same, padding included (the wrapper's `--settings` sets no `padding`). If `--settings` does not win, stop and report on #521; that reopens decision 5.
- [ ] **#521:** then remove the #481 line (the one in `apps/studio/workbench/statusline-usage.sh`) from `~/.claude/statusline-command.sh`, with Daniel's approval, and check the top bar's usage still updates from a studio-started session.
- [ ] **#522:** a real "your turn" notification while Daniel is in another app, in a screenshot in the PR. (No orange text: #565, decision 1, dropped it.)
- [ ] **#522:** with Remote Control on (Daniel's own `remoteControlAtStartup`), in a session the studio started: no phone push while the studio is focused, and a push while it is in the background. `~/Library/Application Support/dev.falinks.studio/present` exists only while the window is focused.
- [ ] **#522:** a turn driven from the phone still sends hook events: its row and card update in the studio. If it fails, report it on #522.
- [ ] **#523:** the rail header shows the model and effort, such as `claude-opus-5-5 · effort medium`.
- [ ] **#524:** saving `settings.json` changes zoom and font size live; ⌘=, ⌘- and ⌘0 reach the window and are not taken by a menu.
- [ ] **#525:** a table in a response renders; clicking a link opens the browser, and the studio's window stays on the app.
- [ ] **#527:** both Doctor defaults run in a new terminal in the selected place.
- [ ] **#528:** "Edit" opens a user skill in Zed.
- [ ] **#529:** two live terminals side by side and stacked, each taking input and resizing.
- [ ] **#535:** a finished subagent leaves the list; opening its history entry writes one real summary.
- [ ] **#540:** a reply ending with a question shows "your turn", not "needs you"; a session waiting on its subagents is not in the "Needs you" group.
- [ ] **#541:** a session started with Auto mode runs in auto mode, and a blocked action still shows "needs you".
- [ ] **#542:** the docked panel's shell survives an app restart and shows under a #529 split.
- [ ] **#568:** a turn that only writes a `.DS_Store` (open its folder in Finder) gets no card.
- [ ] **#569:** a question, a plan and a permission each dock within the read loop's 200 ms, not after up to 1.2 s. Time the worst of a few from the terminal's prompt to the card, and record it beside 1.2 s in the PR. The UX harness (`local-evidence/2026-11-09-ui-ux/measure/`) stubs the shell and has no dock scenario, so it cannot time this.
- [ ] **#557:** from a task worktree session, clicking `path:line` in the terminal and a file link in the rail opens that worktree's file, not the main checkout's, at that line in Zed.

## Checked automatically, early

The two checks that would cost rework if found late, run on 2026-11-09:

- **#525, Markdown in WebKit:** the renderer's output with the studio's stylesheet, rendered through macOS QuickLook, which uses WebKit like WKWebView. Tables, nested lists, code blocks, quotes and inline code render, and raw HTML shows as text. One defect: links use WebKit's default dark blue on the dark background. The render and its script are in `local-evidence/2026-11-09-round-5/webkit-525/`. The link click and the navigation guard still need the app.
- **#521, `--settings` over the status line:** Claude Code's settings docs say `--settings` "applies above your user, project, and local files and below managed settings", and a key set there "overrides the same key in your project and user settings files" ([Settings files and precedence](https://code.claude.com/docs/en/settings), read 2026-11-09). `statusLine` can be set from any file ([settings reference](https://code.claude.com/docs/en/settings-reference.md)). Only a managed `statusLine` would win, and Daniel's machine has no `/Library/Application Support/ClaudeCode/` folder, where a managed settings file would be. The live check above confirms it.
