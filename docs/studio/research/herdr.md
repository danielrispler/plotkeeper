# herdr.dev: what the studio should copy

Research for #530 (spec #517, decision 22), 9 November 2026. Question: which herdr.dev features should the studio copy? Each feature is mapped to studio terms (`docs/studio/CONTEXT.md`) and marked **copy**, **skip** or **later**. Changes to the studio's direction are listed at the end for Daniel to decide.

Sizes are rough: **S** is a day or less, **M** a few days, **L** one to two weeks, **XL** more.

## Sources and how they were read

herdr ships three products, all Apache-2.0 Rust under the [`herdrdev`][gh-org] GitHub organisation ([home page][home]):

- **Herdr** (the app, [`herdr-app`][app-repo]): a native desktop app, created 16 October 2026, at 0.1.14 on 8 November 2026 ([app changelog][app-cl]). It is built on GPUI, Zed's UI framework (vendored under `vendor/gpui-kit` and `vendor/gpui-pre` in the repo tree).
- **herdr-core** (`hc`, [`herdr-core`][core-repo]): the background runtime that owns the terminals. It "draws nothing" ([core README][core-readme]). At 0.1.19 on 7 November 2026 ([core changelog][core-cl]).
- **Herdr TUI** (`herdr`, [`herdr`][tui-repo]): the original terminal multiplexer, at v0.9.5 on 14 October 2026. It "remains supported", but "the new app and Herdr TUI don't share agents" ([home]). The pages under `herdr.dev/docs/` document the TUI, not the app.

The GitHub files (READMEs, `docs/`, changelogs, the app's agent guide [`defaults/AGENTS.md`][app-guide]) were read raw with `gh api`. The herdr.dev pages ([home], [core page][core-page], [guide], [TUI agents][tui-agents], [TUI integrations][tui-int], [TUI session state][tui-state]) were read through a fetch tool that summarises pages, so their quotes are less exact than the repo's. herdr was not installed or run, and no screenshot or video was viewed, so nothing here judges how the UI looks.

Sources are rich; the gaps are named where they matter.

## Features, mapped

### 1. Agents outlive the window

- **herdr:** "The window arranges terminals. The core runs them. Closing the window leaves them running" ([app README][app-readme]). Holder processes own the terminals, so "a core restart or update can leave programs running" ([core security][core-sec]); `hc update` keeps running terminals ([core README][core-readme]). After a reboot or core crash, "kept panes get fresh shells in their last folders, not their old programs" ([app README][app-readme]). The app does not resume agents after that ([home], FAQ); the TUI resumes Claude Code with `claude --resume <id>` when its integration recorded the session ([tui-state]).
- **Studio:** the terminal daemon and revival. Already built ([#322][s322], #266 Notes): the daemon outlives the app, and revival brings Claude terminals back with `claude --resume`.
- **Mark: skip.** The studio already has it. One difference is worth keeping in mind: herdr updates its core without stopping programs, because a holder per terminal owns each program; the studio's daemon upgrade instead waits until every terminal is idle, asks, then revives ([#322][s322]). **Later (L)** if daemon upgrades start to hurt.

### 2. Agents keep working with the computer closed

- **herdr:** only on another machine. "Remote work keeps running when your local window closes or your laptop sleeps, as long as the remote machine and core stay up" ([app README][app-readme]); "Remote agents keep running while your laptop sleeps or the app is closed" ([home]). No source says local agents keep running while a laptop sleeps. Nothing describes a hosted cloud: you bring the machine ([home]).
- **Studio:** a place on another machine. The studio has no such concept.
- **Mark: later, XL. Direction change A.** It needs feature 3.

### 3. Every machine in one sidebar (remote over SSH)

- **herdr:** `hc machine add user@host --name box`, then `hc run --on box ...`; the remote machine needs `hc`, which `add` can copy there ([core machines][core-mach]). A `--via` command carries the protocol through any pipe, with examples for `fly ssh console` and a Vercel Sandbox helper ([core-mach]). "The remote core owns its programs, terminal history, and files"; the local core "combines terminal lists and events" ([core-mach]). SSH does login and encryption; core "does not add another encryption layer or authenticate a remote user itself" ([core-sec]). In the app, remote spaces and agents join the same sidebar under the machine's name ([home], [app-readme]).
- **Studio:** the terminal daemon on another machine; places and tasks on it; the session list and board merging both machines.
- **Mark: later, XL. Direction change A.** The studio's understanding layer reads Claude Code hooks into a local `events.jsonl`, plus transcripts and Git, all on Daniel's Mac (#266 Notes, #340). A remote session would need those carried back too, not only its terminal. herdr does not solve that part: it carries terminals, status and files, not hook events.

### 4. Any agent, no lock-in

- **herdr:** "Herdr runs real terminals, so any terminal-based agent works" ([home]). Status comes from OSC 7501, the Program Status Protocol: "A terminal's status is whatever its program reported through OSC 7501 ... hc never infers status from the screen. Programs that don't speak 7501 have no status" ([core schema][core-schema]). Claude Code speaks it: its 2.1.295 changelog says "Added Program Status Protocol (OSC 7501) support" ([Claude Code changelog][cc-cl]). Agents are added to the + menu through `programs.json` ([app-guide]). The TUI instead reads about 25 named agents' state from the screen, with optional per-agent hook integrations ([tui-agents], [tui-int]).
- **Studio:** a data source for agent sessions other than Claude Code. The studio is Claude Code only by decision ([#320][s320]).
- **Mark: later. Direction change B.** Two sizes:
  - **Status only (M):** read OSC 7501 in the terminal daemon, for any program. Other agents would get running, needs you and your turn on the board, but no intention, cards, brief or estimate, since those come from Claude Code's hooks and transcript. The #322 spike found Claude only sends a query (`ESC ] 7501 ; ?`) and reports to a terminal that answers; the reply format is still not researched. herdr-core's source (`crates/herdr-core/src/status.rs`) answers it and could be read, but this note did not read it.
  - **Full understanding layer for another agent (XL):** a second hook source, a second transcript reader and a second fixture set, per agent.

### 5. Four agent states, with "done until seen"

- **herdr:** "Working: still busy. Blocked: waiting for you. Done: finished while you were looking elsewhere. Idle: not working, or a finished agent you have looked at" ([app-readme]). A space's and a tab's status "is the most urgent of its agents: blocked > done > working > idle" ([app-guide]). Seen agents are shared between windows ([app-cl] 0.1.12).
- **Studio:** running, needs you, your turn, idle and ended. Your turn has no seen or unseen mark; a search of `apps/studio` found none.
- **Mark: copy, S.** An unseen mark on your-turn rows, cleared when Daniel opens the session. The earlier survey found the same pattern in Claude Code and Conductor ([agent session UI research](../agent-session-ui-research.md), "unread marker"). The rollup onto a task's board row is probably what the board does already; check before building.

### 6. Spaces, tabs and split panes

- **herdr:** "A space holds work on one machine, usually in one repo. Each space has tabs. Split a tab into panes" ([app-readme]). Suggested use: "a space per repo, a tab per task" ([home]).
- **Studio:** a space is near a project, a tab near a task, a pane near a pane. The studio shows one pane at a time by decision (`CONTEXT.md`, **Pane**).
- **Mark: skip.** The studio already has projects, tasks and places. Split panes are already decided as up to two terminal panes ([#517](https://github.com/danielrispler/Falinks/issues/517), "Split terminals", decision 20); herdr's free-form splits go beyond that, which #517 puts out of scope ("Splits beyond two panes, and saved layouts").

### 7. Worktrees as spaces, with branch state

- **herdr:** the `worktree` mod makes worktrees into spaces under `~/.herdr/worktrees/<repo>/<branch>`; the sidebar "nests worktree spaces under their repo's and shows each space's repo, branch, ahead/behind and uncommitted changes" ([app-guide]).
- **Studio:** a task's worktree on its board row. The studio already creates and removes tasks with worktrees under `.claude/worktrees/` ([#320][s320]). A search of `apps/studio` found no ahead/behind count.
- **Mark: copy, S.** Ahead/behind and an uncommitted-changes count on each task's board row, from Git, which is already a data source.

### 8. Agents run agents

- **herdr:** `hc run --bg --name review reviewer "..."`, `hc send review "..."`, `hc read review`; children report back with `hc send`, parents wait with `hc wait --children all`, and "Children end with their parent unless started with `--outlive-parent`" ([core-readme]). Templates give an agent a command and a role ([core templates][core-tmpl]). The app lists these children in the agents sidebar and opens one in **peek** over the tab ([app-readme]).
- **Studio:** subagents, workers and helpers. These are Claude Code's own `Agent` calls and background tasks, which the studio watches through hooks.
- **Mark: skip.** Claude Code already spawns and wakes subagents, and the Falinks engine is the studio's multi-session orchestrator. A second channel would add sessions the hooks cannot see as subagents. Peek is close to opening a worker in the story rail, which the studio has. Direction change E covers this if Daniel wants it.

### 9. Change anything: plain files, layouts and mods

- **herdr:** layout, sidebar, tabs, keys and themes are plain files in `~/.config/herdr-app`, with an `AGENTS.md` explaining them to your agent ([home]). Mods are JavaScript programs that draw panels; "A broken edit keeps the last good version and reports the error" ([app-readme]). "Mods are trusted code, not sandboxed extensions" ([app-readme]). The home page claims 215 layouts and 435 mods in its gallery ([home]).
- **Studio:** settings. `settings.json` and `keymap.json`, reloaded on save, with the last good settings kept (`CONTEXT.md`, **Settings**). This is already the same pattern.
- **Mark: skip** mods and layouts. Daniel is the only user, and his agent can change the studio's own source; a mod API would be a second, weaker way to do that.

### 10. Command palette

- **herdr:** "Ctrl+Shift+P shows every command and its key (Cmd+Shift+P on macOS)" ([app-readme]).
- **Studio:** none found in `apps/studio`; the keymap exists.
- **Mark: later, M.** Useful once the studio has more commands than Daniel remembers.

### 11. Notifications, sounds and the phone

- **herdr:** settings include sounds, such as "a softer sound when an agent needs me" ([guide]). Phone alerts come from a mod "that watches the agents and calls ntfy.sh" ([guide]). The app's changelog mentions "the phone app" once ([app-cl] 0.1.12); no other source describes it, so it is unverified here.
- **Studio:** one macOS notification per newly needs you, only while the window is unfocused (#266 Notes).
- **Mark: later, S** for a needs-you sound. **Direction change D** for phone push.

### 12. A CLI that drives the window

- **herdr:** `herdr-app do tab.new --name "fix login" --argv '["claude"]' --task "look at the login bug"`; `herdr-app status` describes what is on screen ([app-guide]). The guide's example: "Open a worktree for issue 42 and start Codex on it with the issue as its task" ([guide]).
- **Studio:** a command that creates a task and starts an agent session in it with a first prompt.
- **Mark: later, M.** It would let Daniel's agent open a task for a ticket. Nothing in round 5 needs it.

### 13. A Changes panel with commits

- **herdr:** a Changes panel lists changed files and takes a commit message ([app-cl] 0.1.8, 0.1.14).
- **Studio:** cards explain each change; the agent commits.
- **Mark: skip.** The understanding layer covers a change with a card, and Daniel's agents do the commits.

### 14. Agent hook integrations

- **herdr (TUI):** `herdr integration install claude` writes `hooks/herdr-agent-state.sh` into `~/.claude` and "updates `settings.json` with Herdr hook entries", to record the session id for resume ([tui-int]).
- **Studio:** the studio plugin, loaded through `CLAUDE_CODE_PLUGIN_DIRS` precisely to avoid writing `~/.claude/settings.json` ([#340][s340]).
- **Mark: skip.** The studio's way is already less invasive.

### 15. Native GPU app, telemetry, install scripts

- **herdr:** GPUI, no Electron (repo tree; [tui-repo] README: "one rust binary, no electron"). "Usage telemetry is on by default"; it sends agent names and usage counts daily, not terminal text ([app security][app-sec]). Install is `curl ... | sh` ([home]).
- **Studio:** Tauri 2 by decision ([#320][s320]).
- **Mark: skip** all three. Tauri is settled; telemetry has no reader for a one-user app.

### 16. "A simple UI"

Daniel named it. The sources describe the default layout only as sidebars for spaces and agents plus a tab bar ([app-readme], [app-guide]), and built-in layouts "Herdr, Terminals and Herdr 98" ([app-readme]). Without viewing it, this note cannot say what makes it feel simple. The studio's own session list and story rail are the matching surfaces; the copyable parts found are features 5 and 7.

## Headline

- **Copy (both S):** an unseen mark on your-turn rows (5); ahead/behind and uncommitted counts on board rows (7).
- **Later:** remote machines and laptop-closed work (2, 3, XL); OSC 7501 status for other agents (4, M) or their full understanding layer (XL); holder-per-terminal upgrades (1, L); command palette (10, M); needs-you sound (11, S); a CLI that opens a task (12, M).
- **Skip:** what the studio already has (1, 6, 9, 14), what conflicts with a decision (splits beyond two panes, Electron-free rewrite, settings-file hooks), and what another part already covers (8, 13, 15).

## Direction changes for Daniel

Each goes against a current studio decision or adds a new kind of thing. None is recommended here; they are listed so Daniel can decide with the evidence above.

- **A. Remote places, so agents keep working with the laptop closed.** herdr gets this only by running on another machine over SSH (features 2, 3). For the studio it would also mean carrying hook events, transcripts and Git state back to the Mac, which herdr does not do. Containers (#531) are a related option and may use this note.
- **B. Providers other than Claude Code.** Against "Claude Code only" ([#320][s320]). OSC 7501 makes status cheap for any agent; the understanding layer is not cheap (feature 4).
- **C. Use herdr-core instead of the studio's own terminal daemon.** #322 chose "our own background terminal daemon (no tmux, no Herdr)". herdr-core is Apache-2.0 Rust with an OpenRPC API (`hc schema`), keeps terminals through its own updates, and already does remote machines ([core-readme], [core-sec]). The cost: it runs anything for any process under Daniel's account that reaches its socket ("There is no separate API password or per-client permission check", [core-sec]), it is three weeks old and moving fast ([core-cl]), and the studio's revival and daemon tests would be rebuilt on it.
- **D. Phone notifications.** herdr sends them through ntfy.sh (feature 11). Session names and states would leave the Mac.
- **E. An agent-to-agent channel outside Claude Code.** `hc send` and `hc wait --children` (feature 8) overlap Claude Code's subagents and the Falinks engine.

[gh-org]: https://github.com/herdrdev
[home]: https://herdr.dev/
[core-page]: https://herdr.dev/core/
[guide]: https://herdr.dev/guide/
[tui-agents]: https://herdr.dev/docs/agents/
[tui-int]: https://herdr.dev/docs/integrations/
[tui-state]: https://herdr.dev/docs/session-state/
[app-repo]: https://github.com/herdrdev/herdr-app
[app-readme]: https://github.com/herdrdev/herdr-app/blob/master/README.md
[app-guide]: https://github.com/herdrdev/herdr-app/blob/master/defaults/AGENTS.md
[app-cl]: https://github.com/herdrdev/herdr-app/blob/master/CHANGELOG.md
[app-sec]: https://github.com/herdrdev/herdr-app/blob/master/docs/security.md
[core-repo]: https://github.com/herdrdev/herdr-core
[core-readme]: https://github.com/herdrdev/herdr-core/blob/master/README.md
[core-cl]: https://github.com/herdrdev/herdr-core/blob/master/CHANGELOG.md
[core-mach]: https://github.com/herdrdev/herdr-core/blob/master/docs/machines.md
[core-sec]: https://github.com/herdrdev/herdr-core/blob/master/docs/security.md
[core-tmpl]: https://github.com/herdrdev/herdr-core/blob/master/docs/templates.md
[core-schema]: https://github.com/herdrdev/herdr-core/blob/master/schema/README.md
[tui-repo]: https://github.com/herdrdev/herdr
[cc-cl]: https://github.com/anthropics/claude-code/blob/main/CHANGELOG.md
[s320]: https://github.com/danielrispler/Falinks/issues/320#issuecomment-6465695482
[s322]: https://github.com/danielrispler/Falinks/issues/322#issuecomment-6497575267
[s340]: https://github.com/danielrispler/Falinks/issues/340#issuecomment-6496945549
