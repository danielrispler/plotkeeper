# How agent tools present parallel sessions

Research for the studio board, 30 October 2026: presentation only, from official docs, changelogs and blogs. Many features are in preview. **Unverified** marks a claim without a primary source.

## Cross-tool patterns

- **A grouped list, not a board.** Every tool uses a sidebar list, grouped by project or repository, some also by status. No tool ships a kanban board. Split panes and grids serve only a few sessions at once.
- **A row is one line:** name, state icon, a short activity line, age, and a PR or diff badge.
- **Helpers stay inline; workers get rows.** A short subagent is a collapsed card in its parent with a live line; expanding it shows prompt and result. A long-running worker gets its own nested row.
- **"Needs you" is its own state.** It is yellow, sorts first, has counts in the app chrome, and names what it waits on. It is kept apart from "finished, your turn".
- **Notifications go out only when you are not looking.** No source describes a tool taking focus.
- **Results are a diff count plus PR and CI status,** with an unread marker. A successful helper disappears fast; a failed one stays.

## Per tool

### Claude Code (CLI, agent view, Desktop, claude.ai/code)

1. `claude agents` groups sessions as Pinned, Ready for review, Needs input, Working and Completed. Each row shows its name, activity, age and a `#N` PR link ([agent view][cc-av]). Desktop has a sidebar list and a split view ([desktop][cc-desk]).
2. Agent view lists no subagents. Inside a session, a subagent panel shows a `(+N)` count of descendants. A subagent's row goes at once on success but stays 30 s on failure ([subagents][cc-sub]). When more than three teammates are idle, the extra idle rows fold into one `N idle agents` row. Working and failed teammates keep their own rows ([teams][cc-teams]).
3. State icons: Working animated, Needs input yellow, Completed green, Failed red. Since v2.1.205, rows show the session's own one-line report "instead of a raw tool invocation" ([cc-av]).
4. Needs input sorts first. A peek panel shows the exact question and `waiting 3m`, and takes a reply. The tab title reads `2 awaiting input` ([cc-av]). A subagent's permission prompt comes up in the main session and names that subagent ([cc-sub]).
5. A Changes card shows `+42 -18` and the PR's CI status ([cloud][cc-web], [cc-desk]).

### Cursor (Agents Window, Projects)

1. One sidebar holds all local and cloud agents ([blog][cur-blog]). Rows were cut to one line "so more agents fit at a glance". Agent Tabs show chats "side-by-side or in a grid" ([3.0][cur-30]).
2. Subagents open as tabs on their parent ([release notes][cur-rn], 3.24). Project workers "appear under the Project row" ([projects][cur-proj]).
3. A Project's coordinator "reports a short status for its current step" ([cur-rn], 3.24).
4. When a subagent waits on you, the parent's status line says "Waiting for approval" rather than "Waiting for subagent" ([cur-rn], 3.25). If Cursor is unfocused, "one system notification covers all agents that stopped" ([cur-rn], 3.26). Only the coordinator's turn plays the finished chime ([cur-rn], 3.24). **Unverified:** what the sidebar dot colours mean (the only source is a user forum post).
5. Cloud agents attach "demos and screenshots" ([cur-blog]). Unread agents come first in the Dock menu ([cur-rn], 3.24).

### Conductor

1. One sidebar row per workspace, each in its own worktree ([workflow][cond-wf]). Rows show GitHub status: merged, failing CI, conflicts ([0.44.0][cond-044]). An optional group-by-status mode puts counts on collapsed groups ([0.39.0][cond-p7]).
2. Not documented.
3. Running indicators ([cond-p7]). A chat waiting on background work shows "a dedicated timer" ([0.77.0][cond-077]).
4. The sidebar has "distinct icons for plan approval and user input states" ([cond-p7]). Clicking a workspace that is already selected jumps to its oldest unread chat ([cond-044]).
5. A Checks tab shows git status, CI, comments and todos ([checks][cond-checks]).

### Devin

1. The sidebar groups sessions by status (Working / Ready / Blocked / Inactive), by PR status or by repository. Hovering a row opens a preview card ([release notes][devin-rn]).
2. Child sessions form "a nested tree" with tree connectors. An Agents tab opens on its own and shows each child's status, todos and PRs ([devin-rn]). The coordinator "is woken automatically" when a child finishes or needs input ([advanced][devin-adv]).
3. The current step shows "what Devin is doing" once that step's summary is ready ([devin-rn]).
4. The sidebar "names the approval a session is waiting on" ([devin-rn]).
5. Row labels such as "PR created". A status stops asking for review "once that step is done" ([devin-rn]).

### OpenHands (Agent Canvas)

1. Each parallel agent has its own worktree, "all visible from one interface" ([canvas][oh-canvas]). Conversations sit in workspace folders. One with an error "is not dimmed" ([conversations][oh-conv]).
2. A child conversation stays linked to its parent, and its result returns there. How children are displayed is not documented ([oh-conv]).
3. A composer chip shows the current action and falls back to `Thinking` ([oh-conv]).
4. The status model has `WAITING_FOR_CONFIRMATION` and `STUCK` ([enterprise][oh-ent]). How they are displayed is not documented.
5. A Changes tab ([features][oh-feat]).

### VS Code + GitHub Copilot

1. A sessions list grouped by workspace or by time, with pin and "Mark as Done" ([sessions][vs-sess]).
2. Expanding a subagent shows its "full initial prompt ... and the result" ([1.109][vs-109]). A collapsed parent row summarizes its hidden chats ([1.139][vs-139]). On GitHub, subagent activity is "collapsed by default, with a heads-up display showing what it's working on right now" ([changelog][gh-vis]).
3. A running subagent shows its task and "whichever tool is currently used" ([vs-109]).
4. A command-center indicator counts sessions that are in progress, unread or need attention ([vs-109]). A compact row expands when its session needs input ([vs-139]). By default, notifications appear only when the window is unfocused ([1.136][vs-136]). A Dock badge is in preview ([1.138][vs-138]).
5. A Changes pill combines edits across chats ([vs-sess]). GitHub shows the session log next to Files changed ([mission control][gh-mc]).

### Zed

1. A Threads Sidebar groups threads by project. Each entry shows its title, status indicator and agent ([parallel agents][zed-par]).
2. **Unverified:** how subagents appear.
3. Indicators show "which tools the model is using" ([agent panel][zed-panel]).
4. A desktop notification and a sound ([zed-panel]). The status states are not documented.
5. The panel shows the files changed, how many, and how many lines. Each hunk can be accepted or rejected ([zed-panel]).

### Warp

1. The panel lists runs as Working, Blocked 🟨, Failed 🔺 or Success ✅ ([runs][warp-runs]). Vertical tabs show live status for CLI agents such as Claude Code ([CLI agents][warp-cli]).
2. Local children show as a pill bar above the parent and get no rows. Cloud children get their own rows. "A parent can finish successfully while a child is still running or has failed" ([warp-runs]).
3. A status icon on each tab ([notifications][warp-notif]).
4. Notification types are Complete, Request and Error. A toast appears only for an agent in another tab, with at most two on screen, and it auto-dismisses. Child conversations send no toasts ([warp-notif]).
5. A shared session with plan, commands and output ([warp-runs]).

### LangGraph Studio / LangSmith

1. A dropdown lists threads ([Studio][lg-studio]).
2. The trace tree shows "nested runs, timing, retries". A trajectory flattens the main agent and its subagents into one ordered list. You read the trajectory first, then the trace ([trajectories][ls-traj]).
3. **Unverified:** whether the graph highlights the running node ([blog][lg-blog]).
4. Agent Inbox is modelled on "an email inbox and a customer support ticketing system". It has three kinds of item: notify, question and review ([ambient agents][lc-ambient]).
5. Turns, nodes and state keys can collapse ([lg-studio]).

## Implications for the studio

1. **Make the board a grouped list with one line per row,** needs-you group first, as Claude, Devin and Conductor do. A kanban board has no precedent here.
2. **Show the agent's own words, not its last tool.** Claude Code dropped raw tool invocations from rows ([cc-av]). The studio's hooks may only give tool steps, so use those as a fallback and expect them to read poorly.
3. **Name the wait and its age.** Put the question or permission on the row with "waiting 3m", and allow a reply from a peek ([cc-av], [devin-rn], [vs-139]).
4. **Use the worktree to tell workers from helpers.** A subagent with its own worktree gets a nested row ([devin-rn], [warp-runs]). One without stays a collapsed line with its live task, and expands to the spawn prompt and result ([vs-109], [gh-vis]).
5. **Raise a nested wait to the top-level row and name who asks.** Cursor had to fix exactly this ([cur-rn]), and Claude names the asking subagent ([cc-sub]).
6. **Keep needs you and your turn apart** in colour, sort order and notifications. Most tools separate them ([cc-av], [vs-136], [warp-notif], [devin-rn]).
7. **Notify only when Daniel isn't looking, combine alerts, never take focus,** and keep children quiet ([cur-rn], [warp-notif], [vs-136]). Show the count in the Dock and the title ([vs-138], [cc-av]).
8. **Let a helper that succeeds fade, and keep a failed child on its parent** ([cc-sub], [warp-runs]).
9. **Clear derived states once the step resolves.** Devin and Cursor shipped fixes for stale "review" and "running" states ([devin-rn], [cur-rn]); hook-derived state risks the same.
10. **Results rows need a diff count, PR and CI status, and an unread marker.** No tool opens a result with a question first. That leaves room for the studio's card; this point is an inference from that absence.

[cc-av]: https://code.claude.com/docs/en/agent-view
[cc-sub]: https://code.claude.com/docs/en/sub-agents
[cc-teams]: https://code.claude.com/docs/en/agent-teams
[cc-web]: https://code.claude.com/docs/en/claude-code-on-the-web
[cc-desk]: https://code.claude.com/docs/en/desktop
[cur-blog]: https://cursor.com/blog/cursor-3
[cur-30]: https://cursor.com/changelog/04-02-26
[cur-rn]: https://cursor.com/docs/release-notes
[cur-proj]: https://cursor.com/docs/agent/projects
[cond-wf]: https://www.conductor.build/docs/concepts/workflow
[cond-044]: https://www.conductor.build/changelog/0.44.0-new-sidebar-rebuilt-composer-codex-checkpoints
[cond-p7]: https://www.conductor.build/changelog/page/7
[cond-077]: https://www.conductor.build/changelog/0.77.0-early-access-multiplayer-api-background-tasks-performance
[cond-checks]: https://www.conductor.build/docs/reference/checks
[devin-rn]: https://docs.devin.ai/release-notes/2026
[devin-adv]: https://docs.devin.ai/work-with-devin/advanced-capabilities
[oh-canvas]: https://www.openhands.dev/product/canvas
[oh-conv]: https://docs.openhands.dev/openhands/usage/agent-canvas/conversations
[oh-ent]: https://docs.openhands.dev/enterprise/conversations-and-sandboxes
[oh-feat]: https://docs.openhands.dev/openhands/usage/key-features
[vs-sess]: https://code.visualstudio.com/docs/copilot/chat/chat-sessions
[vs-109]: https://code.visualstudio.com/updates/v1_109
[vs-136]: https://code.visualstudio.com/updates/v1_136
[vs-138]: https://code.visualstudio.com/updates/v1_138
[vs-139]: https://code.visualstudio.com/updates/v1_139
[gh-vis]: https://github.blog/changelog/2026-03-19-more-visibility-into-copilot-coding-agent-sessions/
[gh-mc]: https://github.blog/changelog/2025-10-28-a-mission-control-to-assign-steer-and-track-copilot-coding-agent-tasks/
[zed-par]: https://zed.dev/docs/ai/parallel-agents
[zed-panel]: https://zed.dev/docs/ai/agent-panel
[warp-runs]: https://docs.warp.dev/platform/managing-cloud-agents/
[warp-notif]: https://docs.warp.dev/agents/capabilities/agent-notifications/
[warp-cli]: https://docs.warp.dev/agents/cli-agents/overview/
[lg-studio]: https://docs.langchain.com/langgraph-platform/use-studio
[lg-blog]: https://www.langchain.com/blog/langgraph-studio-the-first-agent-ide
[ls-traj]: https://www.langchain.com/blog/langsmith-trajectories-tracing
[lc-ambient]: https://www.langchain.com/blog/introducing-ambient-agents
