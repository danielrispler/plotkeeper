# How agent tools show Claude Code subagents, and how they switch to one

Research for #54 (selecting a subagent in the session list leaves the terminal pane on the parent's PTY), 14 November 2026. It extends `docs/studio/research/herdr.md` (herdr internals, not repeated here) and `docs/studio/agent-session-ui-research.md` (list presentation, not repeated here). This note asks only: how is a subagent detected, what does the main pane show when you pick one, and how do live and finished ones differ.

Studio terms follow `docs/studio/CONTEXT.md`. Each tool's own words appear only when describing that tool.

## How this was read

- **Code.** Shallow clones under the session scratchpad, read only, never built or run. Commits:
  - `siteboon/claudecodeui` at `7943e28` (path prefix `cui:`)
  - `winfunc/opcode` at `c24004d` (`opc:`)
  - `stravu/crystal` at `1e18e0b` (`cry:`)
  - `smtg-ai/claude-squad` at `070eb93` (`sq:`)
  - `herdrdev/herdr` at `ee5bfb5` (`tui:`), `herdrdev/herdr-app` at `5216e6f` (`app:`), `herdrdev/herdr-core` at `c84d6e7` (`core:`). The herdr note used older commits, so line numbers there differ from these.
  - Citation form: `cui:server/x.ts:10-20` is that path at that commit, lines 10 to 20. Line numbers are from the clone, not from GitHub's UI.
- **Docs.** Claude Code's own pages, read through a page-to-markdown fetch that truncates long pages: [hooks](https://code.claude.com/docs/en/hooks), [sub-agents](https://code.claude.com/docs/en/sub-agents), [VS Code](https://code.claude.com/docs/en/vs-code). The hooks page is 293k characters. I read the SubagentStart and SubagentStop sections and the common fields, not the whole page.
- **Conductor.** Closed source. Only its public changelog was read. Nothing about its internals is verified.
- **Limits.** No tool was installed or run. Nothing here judges how a UI looks or feels. The tools were checked only for what the code or docs say. **Unverified** marks anything without a primary source.
- **Searches.** Each repo was grepped for `subagent`, `sub-agent`, `parent_tool_use_id`, `isSidechain`, `SubagentStart`, `SubagentStop` and `/subagents/`. A "no hits" result below means that grep, not a full read.

## Short answer

1. **No tool in this set switches to a subagent's PTY, types `/tasks` into a PTY, or drives the arrow keys.** Option (b) has no precedent here.
2. **The only tool that gives a subagent its own view is Claude Code itself,** and that view is a read-only transcript. In VS Code, "Click a subagent to see its prompt and tool calls, open its read-only transcript, or stop it while it runs" ([VS Code docs](https://code.claude.com/docs/en/vs-code), agent map). In the CLI, `/tasks` then Enter opens the transcript ([sub-agents docs](https://code.claude.com/docs/en/sub-agents)). Typing into it to resume is allowed only while its row is still listed.
3. **The third-party tools that read subagents at all render them inside the parent's chat,** as a collapsible card with a timeline (claudecodeui), or a nested tool-call group (Crystal). None gives a subagent a pane of its own. Opcode and Claude Squad show nothing specific.
4. **Detection is by transcript, not by hook.** claudecodeui reads `subagents/agent-<id>.jsonl` and its `.meta.json` sidecar. Crystal uses `parent_tool_use_id` on the SDK stream. No tool in this set uses SubagentStart or SubagentStop to track subagents. herdr's Claude hook does the opposite: it discards them.
5. **herdr's "sub-agent" is not a Claude Code subagent.** It is a child terminal started with `hc run`, which has its own PTY. herdr-app can swap such a terminal into a pane. That works because it is a real PTY, so it does not carry over to #54.

## Per tool

### Claude Code itself (the reference)

All facts here are from official docs.

- **Events.** `SubagentStart` and `SubagentStop` exist. Both carry `agent_id` and `agent_type`, plus the usual `session_id` and the parent's `transcript_path` ([hooks, SubagentStart input](https://code.claude.com/docs/en/hooks)). `SubagentStop` adds `agent_transcript_path`, which "is the subagent's own transcript stored in a nested `subagents/` folder", and `last_assistant_message` (same page, SubagentStop input).
- **Tool hooks inside a subagent.** "When a subagent calls a tool, tool events such as `PreToolUse` and `PostToolUse` fire the same configured hooks as in the main conversation, and the input carries the `agent_id` and `agent_type`" (hooks, common input fields).
- **SubagentStart also fires on resume** and for agent-team teammates handling a new message (hooks, SubagentStart). So one `agent_id` can start more than once.
- **Where it lives.** "Subagent transcripts ... `~/.claude/projects/{project}/{sessionId}/subagents/` ... `agent-{agentId}.jsonl`." They persist across restarts and are deleted after `cleanupPeriodDays`, 30 by default (sub-agents docs).
- **CLI view.** A subagent panel under the prompt shows a tree with a `(+N)` descendant count. `/tasks`, then Enter on a subagent, opens its transcript. "While that subagent's row is still in the subagent panel, type into its transcript to resume it yourself." (sub-agents docs.)
- **Live versus finished** (sub-agents docs):
  - A background subagent that succeeds loses its row at once, with a 30 s `/tasks to see subagents` footer hint. Before v2.1.232 the row stayed 30 s. Before v2.1.208 it left at once.
  - A failed or stopped one keeps its row 30 s, or until you press `x`.
  - The docs also say a completed one "stays listed in `/tasks`, marked done and sorted below running work, for the same 30 seconds". That sentence and the "removes its row immediately" sentence appear to conflict on the same page. **Not reconciled.** The studio should not rely on either.
  - A message from Claude (`SendMessage`) resumes a completed subagent in the background under the same id, so it shows as running again.
- **VS Code extension.** This is the closest thing to what #54 needs, and it is first-party.
  - An agent count such as "2 agents" sits at the bottom of the prompt box. Clicking it opens an **agent map**: "a tree under the main agent, each with its status, elapsed time, and token count" (v2.1.269 or later).
  - Clicking a subagent shows "its prompt and tool calls", a button for its "read-only transcript", and a stop button.
  - Live progress rows sit under the tool call that started them (v2.1.269).
  - Typing `/tasks` opens the map when no agent count shows (v2.1.277).
  - When a subagent shows as one status line with no tool calls, the docs say to open `/tasks` and click it, whose card has "a button that opens its transcript".
- **Desktop app.** The sub-agents page says nothing about a Desktop subagent view. **Unverified.**
- **The spawn tool's name.** The hooks page names an `Agent` tool whose `tool_response` has `agentId` and `status` (hooks, PreToolUse section). The tools below that parse `Task` match older transcripts and may miss `Agent`.

### claudecodeui (siteboon) — verified from source

Web UI that drives Claude through the Agent SDK, not a PTY (`cui:server/modules/providers/list/claude/claude-runtime.provider.ts:4`, imports `query` from `@anthropic-ai/claude-agent-sdk`, line 20). `node-pty` (`cui:package.json:218`) serves its separate shell tab.

1. **Detection.** Two sources, and not hooks.
   - **The SDK stream.** `transformMessage` copies `parent_tool_use_id` to `parentToolUseId` "for subagent tool grouping" (`claude-runtime.provider.ts:615-621`). Live task state comes from the stream's `task_started`, `task_progress` and `task_notification` events (`cui:server/shared/types.ts:588-603`).
   - **The transcripts.** For each spawn row whose `toolUseResult.agentId` is set, it opens `<projectDir>/<sessionId>/subagents/agent-<agentId>.jsonl`, falling back to the older `<projectDir>/agent-<agentId>.jsonl` (`claude-sessions.provider.ts:226-247`). It also reads the sidecar `.meta.json` for type and description (`:204-216`, `:743-766`). It caps the timeline sent to the client at 200 entries (`:64`). Workflow runs live under `subagents/workflows/<runId>/` (`:1732-1753`).
   - **Not in the session list.** The file synchronizer skips every path containing `subagents` or `tool-results`, because those files "repeat the parent session's `sessionId`, so indexing them as standalone sessions overwrites the parent row's `jsonl_path` and corrupts the main session record" (`claude-session-synchronizer.provider.ts:29-42`, `:57-59`; changelog entry for #854 at `cui:CHANGELOG.md:286`). The studio should note this: a subagent transcript carries its parent's `sessionId`.
2. **Main pane when selected.** There is nothing to select. A subagent is a card inside the parent's chat, and it is not a row in the sidebar. `SubagentPanel` is collapsed by default (`useState(false)`, `cui:src/modules/chat/tools/SubagentPanel.tsx:89`) and expands into `SubagentTimeline` (`:176`). The card carries the spawn prompt, the result, and the tool timeline. A strip of running background tasks is a separate component, placement not checked (`cui:src/modules/chat/transcript/BackgroundTasksStrip.tsx`). It is a read-only view with no keystroke path into the subagent.
3. **Live versus finished.** Status is deliberately not taken from the transcript. The code explains why: a background agent "routinely ends its turn cleanly while its work is still outstanding", so a clean ending is not proof of completion (`claude-sessions.provider.ts:~800-825`). Status comes from the `<task-notification>` turn folded onto the launch row (`:735-870`). With no notification, the status is `running` only if the CLI process that launched it is still alive. If that process is gone it is `stopped`, since "a background agent runs inside the CLI process that launched it" (`:~790-800`, `launchedByLiveRun`). A synchronous agent counts as `completed`. This is the best-documented treatment of the live-versus-finished problem in the set.

### Crystal (stravu) — verified from source; deprecated

Crystal was deprecated in February 2026 and replaced by Nimbalyst (`cry:README.md:13-15`). The last commit is 26 February 2026. It is dead, but its approach is common.

1. **Detection.** It runs `claude --verbose --output-format stream-json` (`cry:main/src/services/panels/claude/claudeCodeManager.ts:72`) and parses the messages. A message with `parent_tool_use_id` marks its tool calls as children (`cry:frontend/src/components/panels/ai/transformers/ClaudeMessageTransformer.ts:87-95`). A `tool_use` named `Task` is flagged `isSubAgent`, with `subagent_type` as its label (`:127-132`). No reads of `subagents/` and no hooks (grep: no hits for `.claude/projects` or `SubagentStart`).
2. **Main pane when selected.** Not selectable. The `Task` call is a tool card, "always expanded by default" (`cry:frontend/src/components/panels/ai/components/ToolCallView.tsx:302-305`), with a "Sub-agent Actions (N)" list of nested child tool calls (`:360-375`). The terminal pane is a separate panel (`cry:main/src/services/terminalPanelManager.ts`).
3. **Live versus finished.** A tool call is `pending` until a matching `tool_result` exists, then `success` (`ClaudeMessageTransformer.ts:122-127`). There is no other state.

### Opcode, formerly Claudia (winfunc) — verified from source

1. **Detection.** None. It runs `claude -p ... --output-format stream-json` (`opc:src-tauri/src/commands/claude.rs:1240-1277`). The only subagent handling is cosmetic: a tool named `task` renders a `TaskWidget` (`opc:src/components/StreamMessage.tsx:184-186`, `opc:src/components/ToolWidgets.tsx:2004-2020`, "Spawning Sub-Agent Task" with description and prompt). Grep finds no `parent_tool_use_id`, `isSidechain` or `subagents/`. `SubagentStop` appears only as a hook the user can edit in its settings (`opc:src/types/hooks.ts:21`, `opc:src/lib/hooksManager.ts:30`). Opcode's "agents" (`cc_agents/*.opcode.json`) are its own saved prompts, not Claude Code subagents.
2. **Main pane when selected.** Not applicable. A subagent's own tool calls are not separated from the parent's, so they interleave in the one stream. **Inference** from the absence of any handling; no source says so.
3. **Live versus finished.** Not tracked.

### Claude Squad (smtg-ai) — verified from source

1. **Detection.** None. The repo has no hits for any subagent term, and no reads of `~/.claude/projects` or JSONL (grep, Go files). It runs each agent in a tmux session and polls pane text with `HasUpdated` (`sq:session/tmux/tmux.go:244`), plus auto-dismissing the trust prompt (`:159-179`).
2. **Main pane when selected.** An instance is a tmux session. The preview shows the pane, and `Attach` takes over the terminal on the tmux session (`sq:session/tmux/tmux.go:269`, `sq:session/instance.go:368-372`). A subagent is just text inside that pane, so it is option (a) by construction, with no banner.
3. **Live versus finished.** Not tracked.

### Conductor — closed source; changelog only

Nothing about its internals is verified.

1. **Detection.** The changelog says it runs the Claude Agent SDK (a 2026 release note mentions a "regression in the Claude Agent SDK when using async subagents", found through search; the page was not re-read). **Unverified** beyond that. Changelog lines that show it recognises subagents:
   - 0.38.3 (10 March 2026): "Fix subagent rendering to recognize the latest Claude Code output format" (search snippet; the entry was not opened). That wording points at parsing Claude's output, not hooks.
   - 0.46.0 (5 April 2026): "subagents could get permanently stuck waiting for tool approval when managed approvals are enabled".
   - 0.78.0 (30 July 2026): "Fixed background tasks started by subagents appearing in the main chat transcript".
2. **Main pane when selected.** Not documented. The changelog implies chat-inline rendering and gives no sign of a subagent view of its own: "Codex sub-agent work appearing as an unexpandable spawn row" (0.77.0, so spawn rows exist and, for Claude, presumably expand). **Inference.**
3. **Live versus finished.** 0.77.0 (23 July 2026): "Chats waiting on background agents or tasks now show a dedicated waiting indicator and timer", and "Queued messages now wait until the agent is fully idle instead of sending while it is paused on background tasks". 0.78.0: "You can now stop agents while they wait on background tasks". So it treats a parent with live background agents as still busy, one state on the parent, with no state per subagent. The existing `agent-session-ui-research.md` cites the same 0.77.0 timer.
- Sources: [0.77.0](https://www.conductor.build/changelog/0.77.0-early-access-multiplayer-api-background-tasks-performance), [changelog pages 3 and 7](https://www.conductor.build/changelog/page/3).

### herdr (TUI, app, core) — verified from source

Prior detail is in `herdr.md`. This section adds only what concerns subagents.

1. **Detection of Claude Code subagents: it discards them.**
   - The TUI's Claude hook exits at once for any hook input with an `agent_id`: `is_subagent = bool(hook_input.get("agent_id"))`, then `raise SystemExit(0)` (`tui:src/integration/assets/claude/herdr-agent-state.sh:56-58`). Its only remaining job is reporting the session id on `SessionStart`.
   - Tests pin this: a `SubagentStop` with an `agent_id` must send no `working`, `idle` or `release` report (`tui:tests/cli/hooks.rs:136-143`).
   - It also removes its older `SubagentStop` hook from the user's settings (`tui:src/integration/claude_settings.rs:35-38`).
   - Changelog: subagent hooks "no longer make the parent Claude pane look idle or released when a subagent finishes" (`tui:CHANGELOG.md:1031`), and no longer "revive an idle pane" (`:856`).
   - So herdr found subagent hooks harmful to the parent's state, and tracks parent status only. This answers the HN question quoted in `herdr.md` ("shouldn't they show up in the sidebar?"): they do not.
2. **herdr's own "sub-agent" is a different thing.** `hc run --bg` creates a child terminal with a `parent` field (`core:crates/herdr-core/src/terminals.rs:80-100`). Its skill tells agents to prefer `hc` "over your own sub-agent tool" (`core:skills/herdr-core/SKILL.md:3`). The app lists these under their parent (`app:CHANGELOG.md:66`, "A kept sub-agent shows once in the agents list, under its parent"). This is multi-PTY orchestration, so each child has a real process and terminal.
3. **Main pane when selected (app).** The app can show a child terminal in place of its parent's pane: `swapped: HashMap<String, String>`, "Panes showing another terminal for now (pane → address), e.g. a sub-agent looked at in place of its parent. Not saved; the pane keeps its own" (`app:src/window.rs:383-385`). A peek overlay has a bar showing "who it is and how it's doing, ↗ to go to its pane (or, without one, keep it as a tab)" (`app:defaults/mods/peek/main.js:1-4`). `terminal.keep` gives a pane-less child a pane (`app:src/commands.rs:45`). This works only because the child has its own PTY. A Claude Code Task subagent has none, so nothing here applies to it.
4. **Live versus finished.** The agents list shows live children in a tree (`app:defaults/mods/agents/main.js:170-180`). A child "is never done: it reports to its parent, and the parent's done is the news" (`:53-55`). Its alerts go to the parent, not to the person (`app:src/model.rs:564`, `:759`). Core keeps finished terminals 24 h (`herdr.md`).
5. **Documented limit.** For Antigravity and Grok, herdr says they "count as idle once they return to their prompt, even if background tasks, monitors, or subagents are still running" (`tui:CHANGELOG.md:27`). It does not claim to detect them.

### Others found

- **VS Code extension** is covered above under Claude Code itself, since it is first-party.
- **No other tool was read.** Candidates not checked: Vibe Kanban, CCManager, claude-code-viewer-style JSONL readers, and Claude Code Desktop's own UI. **Unverified.**

## Comparison

| Tool | Source | Detects subagents by | Main pane on selecting one | Live vs finished |
| --- | --- | --- | --- | --- |
| Claude Code CLI | docs | Its own panel; `/tasks` | Read-only transcript; typing resumes only while the row is listed | Row gone at once on success (v2.1.232+), kept 30 s on failure, resumable by `SendMessage` |
| Claude Code, VS Code | docs | Agent map | Read-only transcript button, plus prompt, tool calls and stop | Status, time and tokens per row; live progress rows |
| claudecodeui | code | SDK `parent_tool_use_id` and `task_*` events; reads `subagents/agent-*.jsonl` and `.meta.json` | Collapsible card with timeline inside the parent chat; no row, no pane | `<task-notification>` first; else running only while the launching process lives |
| Crystal (dead) | code | `parent_tool_use_id` and tool name `Task` on the stream | Expanded tool card with child calls | Pending until the tool result arrives |
| Opcode | code | Tool name `task` only | Spawn widget; children not grouped | Not tracked |
| Claude Squad | code | None | Parent's tmux pane | Not tracked |
| Conductor | changelog only, closed | Parses Claude output (inference) | Not documented | Parent shows waiting while background agents run |
| herdr TUI | code | Discards subagent hooks by design | Parent's PTY, no subagent view | Not tracked |
| herdr app | code | Not for Claude subagents; its "sub-agents" are `hc` child PTYs | Swaps the child PTY into the pane | Tree of live children; done goes to the parent |

## Implications for #54

### Verified facts

- Claude Code's own first-party answer, for both the CLI and the VS Code extension, is a read-only transcript, reached from a list. The VS Code docs name it "read-only transcript".
- In the CLI that transcript is opened by `/tasks` plus Enter, a keyboard path the studio would have to imitate. In VS Code it is a click on the agent map.
- Typing into the transcript, which resumes the subagent, works only while the row is still listed (sub-agents docs). After that, a message from Claude's `SendMessage` is the documented route.
- Among the third-party tools, no one types `/tasks` into a PTY, and no one points a terminal at a subagent. Where subagents are visible at all, they are rendered from data (the stream or the JSONL), in the parent's chat.
- The data the studio would need already exists on disk and in hooks:
  - `agent_id`, `agent_type` and (on stop) `agent_transcript_path` come from hooks.
  - `~/.claude/projects/<project>/<sessionId>/subagents/agent-<agentId>.jsonl` holds the transcript, with a `.meta.json` sidecar (claudecodeui reads it for type and description).
  - Transcripts last 30 days by default.
- A subagent transcript carries its parent's `sessionId`. claudecodeui had a bug from indexing them as sessions (`cui:CHANGELOG.md:286`).
- A `.jsonl` is not proof a subagent has finished. claudecodeui's author found a clean turn end is not proof of completion, and used the task notification and process liveness instead.

### Option (a): keep the parent PTY and label the terminal header

- Evidence for: Claude Squad and herdr (for Claude subagents) do exactly this by default, with no label.
- Evidence against: the header would be honest about the bug, but it does not show the thing the person selected. #54's complaint is that the pane contradicts the rail.
- **Inference:** it is a valid stopgap and costs the least. It is the only option that never misleads about what a keystroke does, since the PTY is the parent's.

### Option (b): type `/tasks` and arrow keystrokes into the PTY

- Evidence for: it is the documented CLI path, and the only one that reaches a live subagent's own screen, including the resume-by-typing path.
- Evidence against:
  - No tool in this set does it. The nearest comparison is herdr's typing rules (`core:input.rs`, in `herdr.md`), which show how careful typed input has to be: wait for the person's draft, refuse a blocked program, bracketed paste.
  - Nothing documented addresses a subagent by id, so the row is chosen by position (↑/↓ counts). Rows reorder: the list sorts running above done, rows leave after 30 s, and a failed one stays 30 s. A row index counted when the user clicked may not match one counted a moment later.
  - Typing into the PTY would fire `UserPromptSubmit` and trample whatever Daniel had half-typed, the risk `herdr.md` section 6 point 2 already records for `/rename`.
  - Finished subagents have left `/tasks` after about 30 s, so it cannot serve most rows in the list.
  - Once the view opens, the keys Daniel types go to the subagent, not the parent. The studio cannot know when he leaves it.
- **Inference:** fragile for live subagents, impossible for finished ones, and unlike anything else in this set. Not recommended as the main path.

### Option (c): render the subagent's transcript, read-only, in place of the terminal

- Evidence for:
  - It matches Claude Code's own VS Code design, and the one alternative claudecodeui and Crystal both chose, rendering from data.
  - The studio already reads transcripts, and the story rail already switches to the subagent's story. Showing the same data in the main pane makes the two agree.
  - It works for finished subagents too, as long as the file exists (30 days by default), which (b) cannot.
  - It needs no keystrokes into the parent's process.
- Evidence against:
  - It is read-only. The CLI allows typing into a listed subagent's transcript to resume it. The studio would lose that, unless it keeps (a) alongside it.
  - Whether a subagent is still running comes from hooks, not from the file. claudecodeui had to work out status by notification and process liveness, and the studio should expect the same trap: do not read a quiet file as "done".
- **Inference:** the best fit for the evidence, with (a) as the fallback when no transcript exists yet. A mixed design (c for a subagent row, the parent's PTY for the parent row, a link or button to return to the PTY) is consistent with VS Code's separation of agent map and conversation.
- **Open design question:** what to show before the transcript file exists. A SubagentStart hook arrives with `agent_id` before or about the time the file is created. The studio has not checked the ordering. See Unknowns.

### What the evidence does not settle

- Whether Daniel wants to type into a live subagent from the studio. Nothing in the third-party set offers it, and Claude Code offers it only for a short time.
- Whether the studio should treat a transcript view as a third kind of pane, next to the two terminal panes (`#517` decision 20), or as a replacement for the pane's content. The tools here have no such constraint.

## Unknowns

1. **Conductor's internals.** Closed source. Whether it reads `subagents/*.jsonl`, uses SDK events, or parses stdout is unverified. Its subagent view when you click a spawn row is unverified. The 0.38.3 line was seen only as a search snippet.
2. **The two docs statements on how long a finished subagent stays in `/tasks`.** They appear to conflict. Not tested against a live Claude Code.
3. **Whether `/tasks` can be reached by id** in any version. The docs show nothing, but absence in docs is not proof. Not tested.
4. **The order of SubagentStart and the creation of `agent-<id>.jsonl`**, and whether the file exists when the first hook arrives. Not tested.
5. **Whether `agent_transcript_path` is present on SubagentStart.** The docs list it on SubagentStop only. The SubagentStart example JSON shows `agent_id` and `agent_type` without it.
6. **The spawn tool's name in the transcript** today: the hooks docs say `Agent`, Crystal and Opcode match `Task`. Whether a transcript can contain either depends on the Claude Code version.
7. **Whether the VS Code agent map reads the same JSONL files or a live in-process feed.** The docs say what it shows, not where the data comes from.
8. **Claude Code Desktop's subagent UI.** The sub-agents page is silent.
9. **Tools not read:** Vibe Kanban, CCManager, claude-code-viewer-style tools, Zed, Warp (their docs are in `agent-session-ui-research.md`, which says Warp shows local children as a pill bar above the parent, and Zed's subagent display is unverified).
10. **herdr-app's behaviour with a real Claude Code Task subagent.** Source and changelog say it tracks `hc` child terminals only. Not run.
