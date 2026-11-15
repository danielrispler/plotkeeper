# Multi-repo workspaces: prior art for #16

Research for danielrispler/plotkeeper#16 ("choose the repository in the UI"), 15 November 2026. It does not repeat `herdr.md` or `agent-session-ui-research.md`; it adds only what they lack.

Studio terms follow `docs/studio/CONTEXT.md`. Other tools' words (thread, workspace, space) appear only when describing that tool.

## How this was read

- Official docs, changelogs and source, fetched through a summarizing reader. Quotes are as the reader returned them, not read raw.
- "Unverified" marks claims I could not trace to an official page.
- No tool was installed or run. Nothing here judges how a UI feels.
- Weak spots: Conductor's docs are thin on the sidebar, and its changelog entries could be read by title only. Cursor's behaviour comes mostly from forum posts with a staff reply.

## 1. Multi-repo visibility and surfacing "needs attention"

| Tool | Several repos visible at once? | How attention in another repo surfaces |
| --- | --- | --- |
| Zed | Yes. "The Threads Sidebar can hold multiple projects at once." Each project has its own group ([parallel agents](https://zed.dev/docs/ai/parallel-agents.md)). | Source: a collapsed group still counts waiting threads and shows a Warning icon ("1 thread is waiting for confirmation"); a finished thread shows an accent dot until opened ([sidebar.rs](https://raw.githubusercontent.com/zed-industries/zed/main/crates/sidebar/src/sidebar.rs)). Desktop notification and sound via `agent.notify_when_agent_waiting`, `agent.play_sound_when_agent_done` ([agent panel](https://zed.dev/docs/ai/agent-panel.md)). |
| VS Code | Yes. Multi-root workspaces show each folder as its own root ([multi-root](https://code.visualstudio.com/docs/editing/workspaces/multi-root-workspaces)). The Repositories view manages "multiple Git repositories in a single workspace" ([repos](https://code.visualstudio.com/docs/sourcecontrol/repos-remotes)). | No agent concept in these pages. Not applicable. |
| Cursor | Yes in the Agents Window: "run many agents in parallel across repos and environments" ([3.0](https://cursor.com/changelog/3-0)). A staff reply says chats group by git remote URL, so clones and worktrees of one repo share a section ([forum](https://forum.cursor.com/t/worktrees-in-agent-sidebar/156637)). | Unverified. The changelog page has no attention text. |
| Conductor | Yes. "Multiple Git Repos" shipped in 0.25.6 and "Group workspaces by repo" in 0.35.2 ([changelog](https://www.conductor.build/changelog), titles only). Each project holds one repo; each repo holds many workspaces ([concepts](https://www.conductor.build/docs/concepts/workspaces-and-branches)). | "Mark Workspaces as Unread" (0.25.11) and "Workspace Status" (0.35.0), titles only. See also `agent-session-ui-research.md`, group-by-status with counts. |
| herdr | Yes. All spaces share one sidebar; worktree spaces nest under their repo ([guide](https://raw.githubusercontent.com/herdrdev/herdr-app/b721deda985270ec8256552692729a4add389b2a/defaults/AGENTS.md)). | A space's status is its most urgent agent: "blocked > done > working > idle". "Herdr plays a sound and (unless they're looking) posts a toast that goes there when clicked." |
| claudecodeui | Lists all projects found under `~/.claude` ([README](https://raw.githubusercontent.com/siteboon/claudecodeui/main/README.md)). | Unverified. README names none. |
| Vibe Kanban | One workspace can span several repos, each with "independent git state" ([repositories](https://vibekanban.com/docs/workspaces/repositories.md)). The sidebar lists all workspaces ([interface](https://vibekanban.com/docs/workspaces/interface.md)). | A "Needs Attention" state with a raised-hand icon; an accordion view groups by Needs Attention / Idle / Running. |
| Superset | Add or clone repos; each task gets its own worktree ([README](https://github.com/superset-sh/superset)). | Sidebar shows working, done or waiting; completion chimes and dock badges "when one needs your attention". |
| Claude Squad | Not addressed in its README ([repo](https://github.com/smtg-ai/claude-squad)). Per-task worktrees. | None documented. |
| Crystal/Nimbalyst | Lists "project-level workspace management"; no detail ([repo](https://github.com/stravu/crystal)). | Unverified. |

Patterns worth adopting:

1. **One list across repos, grouped by repo, with attention rolled up onto the group header** when it is collapsed. Zed (waiting count and icon on collapsed headers), herdr (most-urgent status wins), Conductor (counts on collapsed groups).
2. **Attention outside the list too:** a toast or sound that jumps to the session when clicked, skipped if the user is already looking. herdr, Zed, Superset.
3. **Separate "waiting on you" from "finished while you were away."** Zed's two icons; herdr's `done` becomes `idle` once seen.
4. **Group by repo identity, not by folder.** Cursor's remote-URL grouping merges clones and worktrees; users complained it hid worktrees. Zed and herdr keep worktrees as nested rows under the repo.

## 2. Sidebar hierarchy and scale

| Tool | Hierarchy | Finished items |
| --- | --- | --- |
| Zed | Project header, then threads and terminal rows. "Threads running in linked Git worktrees appear under the same project as their main worktree." Newest first. | Archive icon on hover or shift-backspace. "Running threads cannot be moved to history until they finish." Thread History holds all threads across projects, newest first ([parallel agents](https://zed.dev/docs/ai/parallel-agents.md), [agent panel](https://zed.dev/docs/ai/agent-panel.md)). Archiving a worktree thread saves its git state and removes the worktree from disk. |
| VS Code | Explorer roots; source control lists repos, each worktree its own repo ([branches](https://code.visualstudio.com/docs/sourcecontrol/branches-worktrees)). `git.detectWorktreesLimit` caps shown worktrees (default 50). | Not applicable. |
| Cursor | Per repo section, chats inside. | Unverified. |
| Conductor | Repo group, workspace rows. Pinned ones stay on top. | Docs say the workflow ends with "archive the workspace" ([intro](https://www.conductor.build/docs/)). Changelog titles only: "Better archiving" 0.33.5, "Instant Archiving" 0.46.0. The View Archive link at the sidebar bottom is Vibe Kanban's ([docs](https://vibekanban.com/docs/workspaces/managing-workspaces.md)); Conductor's archive view is unverified. |
| herdr | Machine, space, worktree spaces nested under repo. Drag to reorder. | Closing the last tab closes the space, but it "stays in `recent`". |
| claudecodeui | Project, sessions. Auto-discovered. | Unverified. |
| Vibe Kanban | Flat list or accordion by status. | Archive: "Move completed workspaces out of the main list." Pin keeps items on top "regardless of their last activity time". |
| Superset, Claude Squad | Flat list of tasks. Claude Squad has pause (commit, remove worktree) and resume ([README](https://github.com/smtg-ai/claude-squad)). | Pause is its archive. |

Patterns worth adopting:

1. **Archive is a separate view, not a filter on the main list.** Zed (Thread History), Conductor and Vibe Kanban (View Archive). The main list then holds only live work.
2. **Archiving frees the worktree but keeps the record.** Zed saves git state and deletes the checkout; Claude Squad pauses after a commit.
3. **Pin overrides recency.** Conductor, Vibe Kanban.
4. **Collapsed groups still load counts.** Zed's source skips thread rows for collapsed groups but keeps counting running and waiting threads, which keeps a big list cheap.

## 3. Branch switching in a main checkout

| Tool | Where the picker lives | Guards |
| --- | --- | --- |
| Zed | A branch picker "next to the worktree picker", and the worktree picker is "in the title bar, next to the project picker". Command `git: switch`, no default key ([git docs](https://zed.dev/docs/git.md)). It offers "an existing, unused branch". | The docs describe no uncommitted-change guard; they point to stash. Unverified beyond that. Worktrees start detached so branches are not shared ([parallel agents](https://zed.dev/docs/ai/parallel-agents.md)). |
| VS Code | Status bar shows the branch and "provides quick branch switching"; command **Git: Checkout to** ([branches](https://code.visualstudio.com/docs/sourcecontrol/branches-worktrees)). Lists local, remote and recent branches. | "Git may block the switch to protect your work"; docs say commit or stash. The guard is git's own refusal. No check for running processes. |
| Cursor | Unverified. | Unverified. |
| Conductor | No picker. The docs tell the user to run `git checkout` in the workspace's terminal ([concepts](https://www.conductor.build/docs/concepts/workspaces-and-branches)). A "Change Target Branch" entry exists (0.28.7, title only). | The page says nothing about uncommitted changes. New workspaces start from the latest `origin` commit, "even if your local checkout is behind". |
| herdr | Spaces show branch, ahead/behind and dirty state. A mod creates worktrees under `~/.herdr/worktrees/<repo>/<branch>`. No picker documented. | None documented. |
| claudecodeui | Git Explorer: "You can also switch branches" ([README](https://raw.githubusercontent.com/siteboon/claudecodeui/main/README.md)). | Unverified. |
| Vibe Kanban | Target branch chosen per repo at workspace creation; a working branch is created from it. | Not applicable: no switching in place. |
| Superset, Claude Squad | Branch is fixed per task worktree. | Not applicable. |

Finding: no tool in this set documents a guard for "an agent or process is running in this folder." The only guard anywhere is git's own refusal when changes conflict.

Patterns worth adopting:

1. **Picker in a persistent header or status bar, plus a palette command.** VS Code (status bar + Checkout to), Zed (title bar + `git: switch`).
2. **Avoid in-place switching by giving each branch its own folder.** Zed, Conductor, Vibe Kanban, Superset, Claude Squad. The main checkout then stays on one branch.
3. **Rely on git's refusal and offer stash**, rather than a custom dirty check. VS Code, Zed.

## 4. Config versus UI state

| Tool | User-edited config | UI state |
| --- | --- | --- |
| Zed | `settings.json`, with per-project `.zed/settings.json` ([config](https://zed.dev/docs/configuring-zed)). `restore_on_startup` is a setting. | Local SQLite databases hold workspace and project state, named `0-<channel>` ([persistence](https://zed.dev/docs/workspace-persistence), found via search; the page itself returned 404 on my fetch, so the detail is unverified). In source, the sidebar serializes only a width the user set and its active view, not defaults ([sidebar.rs](https://raw.githubusercontent.com/zed-industries/zed/main/crates/sidebar/src/sidebar.rs)). Recent projects: File > Open Recent ([windows](https://zed.dev/docs/windows-and-projects.md)). |
| VS Code | User `settings.json` under `Code/User/` ([settings](https://code.visualstudio.com/docs/configure/settings)); `.code-workspace` files allow comments and hold folders, settings and tasks ([multi-root](https://code.visualstudio.com/docs/editing/workspaces/multi-root-workspaces)). | `globalStorage/state.vscdb`, `workspaceStorage/<id>/state.vscdb`, `storage.json`. Official docs do not describe these. Sources are community posts, so unverified. |
| Conductor | Repo settings, "Repo settings" in 0.62.0 (title only). | Unverified. |
| Claude Squad | `~/.claude-squad/config.json`. | Where instance state lives is not stated. |
| claudecodeui | Reuses `~/.claude`. | "Preferences are saved locally"; no path given. |
| Vibe Kanban | Settings page; worktree dir configurable ([repositories](https://vibekanban.com/docs/workspaces/repositories.md)). | Unverified. |
| herdr | Layouts, keys and mods are files the user edits ([herdr.md](herdr.md)). | `recent` list is kept by the app; storage unverified. |

Patterns worth adopting:

1. **Two stores: a hand-edited file the app only reads, and a machine-owned store for UI state.** Zed (settings.json vs SQLite), VS Code (settings.json vs `state.vscdb`).
2. **Persist only what the user changed.** Zed saves sidebar width only when set by the user, so a new default still applies to everyone else.
3. **Per-project config beside the project, global state in the app dir.** Zed `.zed/settings.json`, VS Code `.vscode` and `.code-workspace`.
4. **Put the opt-in "restore on start" choice in the config file**, and the thing restored in the state store. Zed's `restore_on_startup`.

## 5. Grouping, filtering and search at scale

| Tool | Grouping | Search / filter | Archive |
| --- | --- | --- | --- |
| Zed | By project. | Fuzzy "Search threads…" over group labels, titles, worktree names and terminal titles. While a filter is active, collapsed groups are force-loaded and headers do not collapse (source). Thread History search matches titles. | Yes, as above. |
| Conductor | By repo; optional by status. | "Search Workspaces" (0.26.0, title only). | Yes. |
| Vibe Kanban | Flat or by status. | "Filter workspaces by name or branch". | Yes. |
| Linear | By status, assignee, project, priority; sub-groups; empty groups hideable; ordering puts "completed and canceled issues" after active work ([display options](https://linear.app/docs/display-options)). | Filters (not read in detail). | Completed items sink; a time window applies to completed projects, not issues. |
| herdr | By machine, then space. | Unverified. | `recent` list. |
| Cursor, claudecodeui, Superset | Cursor by repo; the rest flat or unverified. | Unverified. | Unverified. |

Patterns worth adopting:

1. **Fuzzy search over title, repo and branch, one box**, which overrides collapse while typing. Zed, Vibe Kanban, Conductor.
2. **Status ordering puts finished work last** instead of removing it. Linear; Vibe Kanban accordion.
3. **Group by repo as the top level, status inside.** Zed, Conductor, herdr.
4. **Archive view for the long tail**, so the live list stays short at 70+ items. Zed, Conductor, Vibe Kanban.

## Implications for #16

Options, not decisions. The studio today: one repository, a sorted live list, then folded "Ended" and folded "Workspaces".

**A. Switcher for one repository (smallest).**
- Picker in the title bar or palette; the choice is stored in a UI-owned file.
- Fits: no repo grouping needed, the sidebar stays as is.
- Cost: a "needs you" session in the other repo is invisible, which is the problem the user has with two repos. Every tool above that handles several repos shows them together (section 1).

**B. Several repositories at once, grouped by repo.**
- Live sessions grouped under a repo header, with needs-you count rolled up on collapsed headers (Zed, herdr). Workspaces fold per repo or stays one list with a repo label.
- Fits: matches the user's two-repo case and the common pattern.
- Cost: the list is longer, "Workspaces" needs a repo dimension, and "Ended" needs a repo column or filter. Needs a way to add a repo (Zed's Add Project popover with recents is one model).

**C. B plus a global attention surface.**
- Add a toast or sound with click-to-focus for needs-you in a non-focused repo (herdr, Zed).
- Cost: the notification path, plus the "already looking" check.

**Where the choice is stored.**
- Keep `settings.json` read-only for the app, as Zed does, and put open repos and recents in a studio-owned state file (Zed, VS Code). Trade-off: two places to look, but comments survive.
- The `repository` key could stay as the default or first repo. Trade-off: a UI that opens a second repo changes behaviour that today comes only from the file.

**Branch switching in a main checkout.**
- Option: no in-place switching; make a task (worktree) for another branch, as most tools do. Trade-off: no Zed-like picker, but no new guard code.
- Option: a status-bar or title-bar picker with git's own refusal plus stash offer (VS Code, Zed). Trade-off: no precedent guards against a running agent; the studio would be first, and would need its own check (for example, disabled while a session runs in that folder).

**Finished sessions.**
- Option: a separate Archive view per Zed, Conductor and Vibe Kanban. Trade-off: one more place to look.
- Option: keep the fold and add a search box and a repo grouping inside it. Trade-off: cheaper, but the fold still holds everything.

**Open gaps to verify before building.**
- Zed's persistence page and DB path (404 on fetch).
- Whether Conductor and Cursor surface attention across repos (no official text found).
- Any tool guarding branch switch against a running agent (none found, which may only mean undocumented).
