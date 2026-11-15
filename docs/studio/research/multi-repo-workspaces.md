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

1. **One list across repos, grouped by repo, with attention rolled up onto the group header** when it is collapsed. Zed (waiting count and icon on collapsed headers), herdr (most-urgent status wins), Conductor (counts on collapsed groups), T3 Code (a closed folder shows its most urgent state), AgentsRoom (project tile badge: red waiting, orange working, green idle). See "Benchmark tools".
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
3. **Rely on git's refusal and offer stash**, rather than a custom dirty check. VS Code, Zed. Workmux does the same for worktrees: dirty trees need `--force`.

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
4. **Put the opt-in "restore on start" choice in the config file**, and the thing restored in the state store. Zed's `restore_on_startup`. cmux splits the same way: `cmux.json` for settings, snapshots under `Application Support` (see "Benchmark tools").

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
4. **Archive view for the long tail**, so the live list stays short at 70+ items. Zed, Conductor, Vibe Kanban, T3 Code (archived threads leave the sidebar; a settings panel lists, unarchives and deletes them).

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

## Round 2: adding, removing, labelling and ordering projects

Taken as decided: several repos visible, live sessions on top with a project badge, a never-folded Projects section, a separate History view, a UI-owned `projects.json`, and a branch picker for the main checkout only. Same rules as above. Weak spots: Conductor, Superset and Cursor docs are thin here. Superset's detail comes from a third-party mirror. Most "unverified" cells below mean the official docs are silent, not that the feature is absent.

### 6. Adding a project

| Tool | Ways to add | Auto-add when a session starts in a folder? |
| --- | --- | --- |
| Vibe Kanban | Three ways when creating a workspace: recent repositories, "Browse repos on disk", "Create new repo on disk" ([creating](https://vibekanban.com/docs/workspaces/creating-workspaces.md)). Clone from URL: not on that page. | No. A workspace is made from a chosen repo. |
| Conductor | "Open project" (local repo), "Open GitHub project", "Quick start" (new repo) ([first workspace](https://www.conductor.build/docs/first-workspace)). No generic Git URL found ([search result](https://www.conductor.build/docs/concepts/workflow)). | No. Sessions are created inside workspaces it made. |
| claudecodeui | Auto-discovery: "all your existing sessions are discovered automatically" from `~/.claude` ([README](https://raw.githubusercontent.com/siteboon/claudecodeui/main/README.md)). Also a creation wizard with a folder browser and GitHub auth, and a clone service ([source tree](https://github.com/siteboon/claudecodeui/tree/main/src/modules/project-creation-wizard), [clone service](https://github.com/siteboon/claudecodeui/blob/main/server/modules/projects/services/project-clone.service.ts)). | Yes in effect: a session in any folder under Claude Code appears as a project. Whether that is a scan or a live hook: unverified. |
| Cursor Agents Window | Pick a workspace in a selector; the staff-confirmed gap is that it cannot select worktrees made outside Cursor ([forum](https://forum.cursor.com/t/cursor-agents-workspace-selector-is-unhelpful/156600/4.md)). `cursor <dir>` should open the window there but "doesn't currently" ([same post](https://forum.cursor.com/t/cursor-agents-workspace-selector-is-unhelpful/156600/4.md)). Docs say only "multi-workspace" ([docs](https://cursor.com/docs/agent/agents-window)). | Unverified. |
| Superset | "add a repository from your computer or clone one" ([README](https://github.com/superset-sh/superset)). A third-party mirror says Cmd+Shift+O opens an existing directory and "adds the project ... without creating a worktree" ([mirror](https://mintlify.wiki/superset-sh/superset/guides/workspace-management), unverified as official). | No. |
| herdr | No project list. A space is named after its first terminal's repo or folder ([README](https://raw.githubusercontent.com/herdrdev/herdr-app/b721deda985270ec8256552692729a4add389b2a/README.md)). | Yes by design: a new terminal in a folder makes or joins a space ([guide](https://raw.githubusercontent.com/herdrdev/herdr-app/b721deda985270ec8256552692729a4add389b2a/defaults/AGENTS.md)). |
| Zed | "Add Project" button, with recent projects, Add Local Folders and Add Remote Folder ([parallel agents](https://zed.dev/docs/ai/parallel-agents.md)). Open Recent, drag a folder in, `zed ~/project`, and `zed --add` "Add to the current window's threads sidebar" ([windows](https://zed.dev/docs/windows-and-projects.md)). | Opening a folder puts it in the sidebar. |

Patterns worth adopting:
1. **Folder browse plus a recents list as the main path.** Vibe Kanban, Conductor, Zed, Superset.
2. **A CLI or drag-drop route into the same list.** Zed (`zed --add`, drag). Cheap for a user who lives in terminals.
3. **Auto-add is a deliberate choice, not a default.** Only claudecodeui and herdr do it; the orchestrators that own worktrees all require an explicit add.
4. **"Add" does not create a worktree.** Superset's mirror and Zed's folder open both add the project alone.

### 7. Removing a project that has live sessions or worktrees

| Tool | Behaviour |
| --- | --- |
| claudecodeui | Soft by default: `isArchived` is set on the project row, "hide from the active list; DB only". `force` deletes the session transcript files, session rows and the project row. Archived projects can be restored. No refusal for live sessions in the code read ([delete service](https://github.com/siteboon/claudecodeui/blob/main/server/modules/projects/services/project-delete.service.ts)). |
| Superset | Deleting a workspace "runs teardown scripts, removes the worktree, and deletes workspace-specific data" (mirror, [workspace management](https://mintlify.wiki/superset-sh/superset/guides/workspace-management)). Removing a project: not found. |
| Conductor | Docs describe archiving, not deleting. Archived workspaces are restorable "from the History pane ... including its chat history" ([workflow](https://www.conductor.build/docs/concepts/workflow)). Removing a repo: not found. |
| Vibe Kanban | Repo removal not documented. Scripts run "when a workspace closes" ([settings](https://vibekanban.com/docs/settings/projects-repositories.md)). |
| Zed | Project removal not documented. Archiving a worktree thread saves git state and deletes the checkout; running threads cannot be archived ([parallel agents](https://zed.dev/docs/ai/parallel-agents.md)). |
| herdr | Closing the last tab closes the space, which "stays in `recent`"; asks for confirmation unless confirmed ([guide](https://raw.githubusercontent.com/herdrdev/herdr-app/b721deda985270ec8256552692729a4add389b2a/defaults/AGENTS.md)). |
| Cursor | Unverified. |

Finding: no tool documents refusing removal while sessions run. Two models appear: hide-but-keep (claudecodeui, Conductor, herdr) and destructive delete behind a confirm (Superset, claudecodeui force).

Patterns worth adopting:
1. **Remove means hide, with the record kept and restorable.** claudecodeui archive, Conductor History, herdr `recent`.
2. **A hard delete is a separate, named action** that says what it deletes (claudecodeui `force`, Superset's teardown).
3. **Archiving frees the worktree, keeps the transcript.** Zed, Conductor.
4. **Refusing while something runs has Zed precedent** for archive ("running threads cannot be moved to history"), but not for project removal.

### 8. Sessions in a folder that belongs to no added project

- **claudecodeui:** moot; every folder becomes a project (README above).
- **herdr:** moot; every folder gets a space. An unnamed space is named from the repo, else the folder, else `~` ([README](https://raw.githubusercontent.com/herdrdev/herdr-app/b721deda985270ec8256552692729a4add389b2a/README.md)).
- **Zed:** every session is under a project; a worktree thread joins its main worktree's project ([parallel agents](https://zed.dev/docs/ai/parallel-agents.md)). The sidebar source also has a `ThreadEntryWorkspace::Closed { folder_paths, project_group_key }` case for threads whose workspace is not open ([sidebar.rs](https://raw.githubusercontent.com/zed-industries/zed/main/crates/sidebar/src/sidebar.rs)).
- **Conductor, Vibe Kanban, Superset, Cursor:** sessions exist only inside a workspace they created, so the case does not arise. Cursor's staff reply says worktrees made outside it are not selectable ([forum](https://forum.cursor.com/t/cursor-agents-workspace-selector-is-unhelpful/156600/4.md)).
- No tool documents an "Other" or "ungrouped" bucket.

Patterns worth adopting:
1. **Show it, never hide it.** Every tool that can see such a session lists it; none drops it.
2. **Name it from its folder** (herdr: repo, else folder, else `~`).
3. **Offer a one-click "add as project"** on that row. No tool documents it, so this is a gap, not a pattern.

### 9. The per-row project badge

- **Zed:** no per-row badge. The project is a group header, and rows show title, status icon and agent ([parallel agents](https://zed.dev/docs/ai/parallel-agents.md)). Search matches group labels, so typing the project name works ([sidebar.rs](https://raw.githubusercontent.com/zed-industries/zed/main/crates/sidebar/src/sidebar.rs)).
- **Vibe Kanban:** the Repository Panel lists "Repository name" with a status indicator ([multi-repo](https://vibekanban.com/docs/workspaces/multi-repo-sessions.md)). Workspace rows: Pinned, Dev Server, PR Status badges ([interface](https://vibekanban.com/docs/workspaces/interface.md)).
- **claudecodeui:** project `displayName` comes from `package.json` `name`, else the last folder segment ([source](https://github.com/siteboon/claudecodeui/blob/main/server/modules/projects/services/projects-with-sessions-fetch.service.ts)).
- **Cursor:** the section is named after the repo; the selector shows paths that make "similarly-named directories" hard to tell apart ([forum](https://forum.cursor.com/t/cursor-agents-workspace-selector-is-unhelpful/156600/4.md)).
- **herdr:** the space name is the label, sticky once renamed ([README](https://raw.githubusercontent.com/herdrdev/herdr-app/b721deda985270ec8256552692729a4add389b2a/README.md)).
- **Conductor, Superset:** colour, initials or icon per repo: unverified.
- **Colour per project setting:** none found in any tool.

Patterns worth adopting:
1. **Name text first.** Every documented example labels by name; none documents a colour or an icon.
2. **Derive the display name, allow an override.** claudecodeui's `package.json` then folder name; herdr's sticky rename.
3. **Disambiguate duplicates.** Cursor's path complaint is the failure to avoid when two repos share a folder name.

### 10. Ordering projects

| Tool | Order |
| --- | --- |
| claudecodeui | Has a star flag (`isStarred`) toggled per project ([star service](https://github.com/siteboon/claudecodeui/blob/main/server/modules/projects/services/project-star.service.ts)). Sort order itself: unverified. |
| herdr | Local machine first, then machines by name; drag to reorder spaces within a machine ([guide](https://raw.githubusercontent.com/herdrdev/herdr-app/b721deda985270ec8256552692729a4add389b2a/defaults/AGENTS.md)). |
| Conductor | Group by repo (0.35.2, title only). Pin keeps workspaces on top ([changelog](https://www.conductor.build/changelog)). Repo order: unverified. |
| Vibe Kanban | Pinned workspaces "regardless of their last activity time" ([managing](https://vibekanban.com/docs/workspaces/managing-workspaces.md)). Project order: unverified. |
| Zed | Threads newest first within a group ([sidebar.rs](https://raw.githubusercontent.com/zed-industries/zed/main/crates/sidebar/src/sidebar.rs)). Project order: not documented. |
| Cursor, Superset | Unverified. |

Patterns worth adopting:
1. **Manual drag order that persists**, with new projects appended. herdr.
2. **Pin or star to float a few to the top.** claudecodeui, Conductor, Vibe Kanban.
3. **Recency inside a group, not between groups**, so the list does not jump. Zed.

### Implications for the decided design

- **Adding.** Folder dialog plus recents covers every tool. A palette or CLI add is a cheap second route. Auto-add on session start is rare (claudecodeui, herdr); the studio's section 8 case could instead show an "add project" action on the row.
- **Removing.** Hide-and-keep fits `projects.json` and the never-lose-history stance. Refusal while sessions run has only archive precedent (Zed); the branch picker's "disabled while a session runs" has none.
- **Unknown folders.** Needs its own decision: ungrouped row named from the folder, or auto-add. Both are unsupported by a documented precedent beyond herdr's naming rule.
- **Badge.** Name text is the only documented form. Colour or initials would be new.
- **Order.** Manual drag order stored in `projects.json` matches herdr and costs one array.

## Benchmark tools: cmux, Workmux, AgentsRoom, T3 Code

How each name was identified. Each is the terminal or desktop orchestrator for coding agents, not a namesake:
- **cmux** is `manaflow-ai/cmux`, a native macOS terminal on Ghostty "for AI coding agents" ([README](https://raw.githubusercontent.com/manaflow-ai/cmux/main/README.md)). Search also returns many forks and unrelated repos called cmux; I used the Manaflow one.
- **Workmux** is `raine/workmux`: git worktrees plus tmux windows for parallel agents ([README](https://raw.githubusercontent.com/raine/workmux/main/README.md)). Other copies are forks.
- **AgentsRoom** is the Electron desktop app at [agentsroom.dev](https://agentsroom.dev/docs/multi-project-multi-agent). Vendor pages only.
- **T3 Code** is `pingdotgg/t3code`, an Electron and web control surface for Claude Code, Codex and others ([README](https://raw.githubusercontent.com/pingdotgg/t3code/main/README.md)). Read from source files, not docs.

Unverified means the docs or source I read are silent. Nothing was run.

### 1. Hierarchy, attention rollup, cross-project surfacing

| Tool | Hierarchy | Live vs finished | Rollup on collapsed groups | Cross-project "needs input" |
| --- | --- | --- | --- | --- |
| cmux | Flat workspace rows by default. Each row shows branch, PR, folder, ports and last notification text ([README](https://raw.githubusercontent.com/manaflow-ai/cmux/main/README.md)). "Workspace groups" nest rows in collapsible named sections ([groups](https://raw.githubusercontent.com/manaflow-ai/cmux/main/docs/workspace-groups.md)). A sample sidebar extension groups by project root with a "No Folder" bucket ([example](https://github.com/manaflow-ai/cmux/blob/main/Examples/CmuxExtensionSidebarExamples/Sources/CmuxExtensionSidebarExamples/ProjectWorktreeSidebar.swift)); it is sample code, not the default ([custom sidebars](https://raw.githubusercontent.com/manaflow-ai/cmux/main/docs/custom-sidebars.md)). | No finished list. Agents are terminals; none documented. | Not covered by the groups doc. | Yes. Panes get "a blue ring", and `Cmd+Shift+U` jumps to the latest unread; `cmux jump-to-unread` in the CLI ([notifications](https://raw.githubusercontent.com/manaflow-ai/cmux/main/docs/notifications.md)). A notification panel is the durable record. |
| Workmux | Worktree = tmux window. The dashboard TUI lists "all active AI agents across all tmux sessions", with sort modes (priority, project, recency), filters and a Worktrees tab ([README](https://raw.githubusercontent.com/raine/workmux/main/README.md)). | Working, waiting, finished icons; finished "clears when the window is focused". No finished list. | Not applicable (flat). | The dashboard is the cross-project view, sortable by priority. |
| AgentsRoom | Sidebar "lists every project grouped by zone"; inside a project, agents sit under Needs you, To review, Working, Available, Inactive ([doc](https://agentsroom.dev/docs/multi-project-multi-agent)). | Inactive is a group. No separate history. | Yes: project tiles show a red badge when an agent waits, orange while one works, green when idle. | Via the tile badges and sounds on "needs input"; a top-level list: unverified. |
| T3 Code | Project folder, then threads. Sections inside: snoozed, settled, pinned, active ([Sidebar.logic.ts](https://raw.githubusercontent.com/pingdotgg/t3code/main/apps/web/src/components/Sidebar.logic.ts)). | Archived threads leave the sidebar. | Yes: a closed folder shows its most urgent state, in order attention, failed, unread, working. | Not found. |

### 2. Adding, removing, folders outside any project

- **cmux:** `Cmd+N` new local workspace, `Cmd+Shift+W` closes one ([README](https://raw.githubusercontent.com/manaflow-ai/cmux/main/README.md)). No project object. A workspace outside any folder is a normal row; the sample sidebar files it under "No Folder".
- **Workmux:** no project list. The project is the repository you run `workmux add` in; per-project config is `.workmux.yaml` ([README](https://raw.githubusercontent.com/raine/workmux/main/README.md)). Agents outside tmux do not show: the dashboard "requires agent status tracking".
- **AgentsRoom:** `Cmd+N` for a new project. Drag a project to another zone; dropping on a new zone name creates it. The close cross "asks for confirmation first" when work is still running; cancel keeps everything running ([doc](https://agentsroom.dev/docs/multi-project-multi-agent)). Folders outside a project: unverified.
- **T3 Code:** the mobile app has add-project routes for a local folder, a repository and a new project ([source](https://github.com/pingdotgg/t3code/tree/main/apps/mobile/src/features/projects)). Grouping modes are "Group by repository", "Group by repository path" and "Keep separate", with per-project override, so clones of one repo can merge ([sidebarProjectGrouping.ts](https://raw.githubusercontent.com/pingdotgg/t3code/main/apps/web/src/sidebarProjectGrouping.ts)). `groupSidebarThreadsByProject` drops "threads whose project is in no group" (read through the reader, so treat as unverified). Pin and archive exist on projects (migration `076_ProjectionProjectsPinArchive`, file name only).

### 3. Worktree and branch ergonomics

- **Workmux (the reference here).** All from the [README](https://raw.githubusercontent.com/raine/workmux/main/README.md):
  - `add` creates a worktree and a matching window and switches to it; `--headless` makes the worktree with no window.
  - `open` switches to an existing worktree's window; `list` shows worktrees with agent, window and merge status.
  - `merge` checks for uncommitted changes and "errors if found, unless `--ignore-uncommitted`". It then removes the worktree, window and local branch.
  - `remove` deletes without merging. A dirty worktree "still require[s] `--force`". It prompts on unmerged commits; `confirm_unmerged_removal: false` skips that.
  - No guard against a running agent is documented.
  - There is no in-place branch switch; a branch is a worktree.
- **cmux.** The README shows the branch in the sidebar and says nothing about worktrees. Source has worktree seeding and an extension prototype ([tree](https://github.com/manaflow-ai/cmux/tree/main/Packages/macOS/CMUXAgentLaunch/Sources/CMUXAgentLaunch/WorktreeSeed)). The user-facing flow, keys and guards: unverified.
- **AgentsRoom.** One setting: "Enable a git worktree per agent so each works in its own copy and branch" ([doc](https://agentsroom.dev/docs/multi-project-multi-agent)). Picker and guards: unverified.
- **T3 Code.** "Each thread can work in its own worktree" ([README](https://raw.githubusercontent.com/pingdotgg/t3code/main/README.md)). Source has worktree removal and "restore missing worktree" code ([worktreeRemoval.ts](https://github.com/pingdotgg/t3code/blob/main/apps/server/src/vcs/worktreeRemoval.ts)). Branch picker and dirty-tree guard: unverified.

### 4. Archive and history

- **T3 Code:** archived threads are excluded from the sidebar, and a settings page lists them with Unarchive and Delete ([panel](https://github.com/pingdotgg/t3code/blob/main/apps/web/src/components/settings/ArchivedThreadsPanel.tsx)). The closest match to the decided separate History view.
- **AgentsRoom:** no history section; "Sessions live in memory with their scrollback". A "Recent agents" row lists open agents ([doc](https://agentsroom.dev/docs/multi-project-multi-agent)).
- **cmux:** no archive. Quit saves a snapshot; relaunch restores layout, folders and best-effort scrollback, but "does not checkpoint arbitrary live process state" ([README](https://raw.githubusercontent.com/manaflow-ai/cmux/main/README.md)).
- **Workmux:** no archive or history. `merge` and `remove` end the worktree. State files are read from `~/.local/state/workmux/agents/`.

### 5. UI state versus config

- **cmux:** user config `~/.config/cmux/cmux.json` (sidebar, tabs, behaviour) plus the Ghostty config; snapshots in `~/Library/Application Support/cmux/`; agent session mappings in `~/.cmuxterm/` ([README](https://raw.githubusercontent.com/manaflow-ai/cmux/main/README.md)). The closest match to the decided split.
- **Workmux:** config `~/.config/workmux/config.yaml` and per-repo `.workmux.yaml`; runtime state under `~/.local/state/workmux/` (XDG split).
- **AgentsRoom:** shortcut settings follow the account; disk layout of projects: unverified.
- **T3 Code:** a desktop client-settings module exists in source (`apps/desktop/src/settings/DesktopClientSettings.ts`); what it holds and where: unverified.

### Does the benchmark set agree on project-first?

Mostly, among tools with a project concept.
- **Project-first:** herdr (worktree spaces nest under their repo; [guide](https://raw.githubusercontent.com/herdrdev/herdr-app/b721deda985270ec8256552692729a4add389b2a/defaults/AGENTS.md)), AgentsRoom (projects, status inside), T3 Code (folders, status as dots and sections), Zed, and Cursor's Agents Window (by repo; [forum](https://forum.cursor.com/t/worktrees-in-agent-sidebar/156637)).
- **Not project-first:** cmux (flat workspace rows; groups are manual, and a status "Agents Board" template is in its beta sidebars) and Workmux (a flat dashboard sorted by priority, project or recency). Both have no project object.
- **Cursor "background agents" as a separate list:** unverified. Only the Agents Window was read.
- **Status inside project:** the projects that group by project keep status as a section or colour inside, and roll the worst status up to the header (herdr, AgentsRoom, T3 Code, Zed). None found does the reverse, status groups containing projects, except Conductor's optional mode and Vibe Kanban's accordion (see `agent-session-ui-research.md`).
- **Cross-project jump:** cmux and Workmux solve cross-project attention with a global jump or dashboard because they lack a project header. A project-first sidebar with a rollup covers the same need, and cmux's `Cmd+Shift+U` is a cheap addition.
