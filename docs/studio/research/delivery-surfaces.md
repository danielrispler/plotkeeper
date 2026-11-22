# Delivery surfaces: giving developers the understanding layer on top of the IDE they already use

> **Answer:** Build one agent-neutral recorder, put a thin adapter in front of it for each agent, and show the result on a local web page that every editor and agent reaches. An adapter is a hook handler where the agent has hooks (Claude Code, Codex, Gemini CLI, Antigravity, Cursor, Copilot CLI, Kiro, Amp, OpenCode, Cline, Windsurf), a transcript or git-tree watcher where it does not (Aider, Zed's own agent, Goose), and nothing where neither exists. An agent's own UI hook (Claude Code's mods, Amp's plugin UI, OpenCode's plugin panels) is an optional extra, never the base. Per-editor plugins are the last thing to build.
> **Evidence:** Strong for what each agent exposes where I read its official docs (Claude Code, Codex hooks, Gemini hooks, Cursor, Copilot, Kiro, Amp, OpenCode, Cline SDK, Aider options, ACP registry). Moderate for reach (the Stack Overflow survey counts editors, not agents). Unverified: Antigravity's vendor docs; hook payloads for Windsurf, Goose and Zed; transcript paths for Cursor, Copilot, OpenCode, Amp and Goose; whether `ui/` runs from a plain local server.

Research for Daniel's question, 22 November 2026: "I don't believe developers who are already used to their IDE will switch to mine ... I only want to give them an extra capability on top of what they already use ... I think we need to find a better option than the desktop app." The Tauri app stays as it is (AGENTS.md, "Implementation language"); this note is about where the capability lives for people who keep their own IDE or terminal. Terms follow `docs/studio/CONTEXT.md`.

Sizes: **S** a day or less, **M** a few days, **L** one to two weeks, **XL** more. All sources were fetched on 22 November 2026; a claim I could not confirm is marked **unverified**.

## 1. Short answer

**What is already agent- and editor-independent in this repo** (read from the tree):

- `hook/` (crate `studio-hook`) reads a hook's JSON on stdin. `plugin/hooks/hooks.json` wires it to Claude Code's events and `plugin-agy/hooks.json` wires the same binary to Antigravity's six (`SessionStart`, `PreInvocation`, `PostInvocation`, `PreToolUse`, `PostToolUse`, `Stop`).
- `core/` already has a per-agent adapter model: `Capabilities` (`core/src/event.rs:209`) lists what an agent reports (asks, background tasks, replies, permission mode, durations, context, `turn_trees`, `transcript_format`); `core/src/claude.rs:14` and `core/src/agy.rs:24` each define one `CAPABILITIES` constant; views follow the flags, "never its name" (`ui/capabilities.ts:1`). Claude Code reports everything; agy reports almost none (`core/src/agy.rs:24-35`, every flag false but its transcript format).
- A third tier exists: **status-only** rows for any terminal program that reports OSC 7501 status, such as Codex (`core/tests/status_only.rs`, `StatusRow` fields at `core/src/lib.rs:330`). So today there are three levels: full hooks plus transcript (Claude Code), hooks plus a different transcript (agy), status only (Codex and the rest).
- `understanding/` (about 6,500 lines, no Tauri dependency, `understanding/Cargo.toml`) is the card writer, brief, store and turn trees; `src-tauri/` is only the shell.

Gaps in the model, to close for neutrality: `Capabilities::deserialize` matches only `"claude"` and `"agy"` and errors on any other name (`core/src/event.rs:240-245`); `TranscriptFormat` has two variants (`core/src/event.rs:249-254`); `turn_trees` is recorded by a hook at turn start and end and is false for agy, which is the capability the cards depend on (`core/src/lib.rs:2889`, `3702`). A git-tree poller would give any agent the same data without hooks (my inference; not built).

**Ranking by cost to support an agent** (detail in section 3):

| Tier | Cost | Agents |
|---|---|---|
| A: thin hook adapter, S each | JSON on stdin, a prompt event, a stop event, tool events | Claude Code (done), Antigravity (done), Codex CLI, Gemini CLI, Cursor, Copilot CLI and cloud agent, Kiro, Windsurf (Devin Local) |
| B: plugin in the agent's own language, S to M each | The hook is a JS or TS function, not a command; it can POST to the recorder | OpenCode, Amp, Cline (SDK) |
| C: files and git, M each | No hooks found. Poll git trees and read a history file; Aider can also call a command when a reply is ready | Aider, Zed's own agent (unverified), Goose (unverified) |
| D: not worth it | Roo Code's extension is reported shut down | Roo Code |

**Ranking by surface** (unchanged in spirit, now agent-neutral):

| Rank | Surface | Reach | Upkeep | Why |
|---|---|---|---|---|
| 1 | Recorder plus adapter layer (agent hooks, or file and git watching) | Every agent that has one of the two | S to M each | The only way to see what each agent did, with no editor code |
| 2 | Local web page (localhost) the adapters link to | Every editor, every agent, SSH with port forward | M, once | One screen for all. Reuses `ui/` (**unverified** that it runs outside Tauri) |
| 3 | Agent-native UI hooks (Claude Code mod, Amp plugin UI, OpenCode TUI/desktop plugin, status lines) | One agent each | S to M each, optional | Nice, never required |
| 4 | GitHub App check run for the PR debrief | Every agent, every editor, teammates | M to L | Hosted; for review, not for the quiz |
| 5 | One VS Code extension (webview) on both registries | VS Code family | L | Only if a browser tab proves too far away |
| 6 | ACP client or proxy | Editors that host agents over ACP | M (**unverified** design) | A possible extra tap, not a base (section 3.3) |
| 7 | JetBrains, Zed, Neovim, Xcode, Visual Studio plugins | One editor each | L each | Skip |

## 2. Reach: who each choice covers

Editor share, Stack Overflow Developer Survey 2025, all respondents (26,143 answered, multi-select, so shares overlap and do not add): Visual Studio Code 75.9%, Visual Studio 29%, Notepad++ 27.4%, IntelliJ IDEA 27.1%, Vim 24.3%, Cursor 17.9%, PyCharm 15%, Android Studio 15%, Neovim 14%, Sublime Text 10.5%, Xcode 10%, Claude Code 9.7%, WebStorm 7.6%, Zed 7.3%, Rider 7.1%, Eclipse 7.1%, VSCodium 6.2%, Windsurf 4.9%, Cline and/or Roo 2.2%, Aider 1.9%, Trae 0.8% ([survey, "Dev IDEs"](https://survey.stackoverflow.co/2025/technology#1-dev-id-es)). I read the JetBrains State of Developer Ecosystem 2025 page, but the editor shares were not in the text I could fetch ([page](https://devecosystem-2025.jetbrains.com/)); treat any JetBrains number as **unverified**.

What that means for each surface (my arithmetic on those shares, not a survey figure):

- **VS Code family**: VS Code, Cursor, VSCodium, Windsurf and Trae are all forks or builds of the same editor. Because answers overlap, the family is at least 75.9% (VS Code alone) and below 100%.
- **JetBrains family**: IntelliJ IDEA 27.1%, PyCharm 15%, Android Studio 15%, WebStorm 7.6%, Rider 7.1%, PhpStorm 5.8%, RustRover 3.2% overlap with each other and with VS Code. One JetBrains plugin covers all of them, but not Visual Studio, Xcode or the terminal editors.
- **Terminal editors and Xcode**: Vim 24.3%, Neovim 14%, Xcode 10%, Zed 7.3%. A terminal or browser surface is the only one-implementation way in.
- **Agent-based surfaces** do not depend on the editor at all. Claude Code is 9.7% in that survey and Cursor 17.9%; the Codex CLI and Gemini CLI are not in the list (**unverified** whether they were options). The reach of a hook plugin is the agents it targets, whatever editor the person has open.

| Surface | Editors covered by one implementation | Agents covered | Needs per-editor work |
|---|---|---|---|
| Claude Code plugin (hooks, mod, skills, MCP) | All: CLI in any terminal, VS Code and forks, JetBrains (via terminal), Desktop | Claude Code only (hooks reusable for others, below) | No |
| Hooks for Codex and Gemini | All terminals; VS Code forks for Codex's extension | Codex, Gemini CLI, Antigravity (already wired here) | No, but each agent has its own hook shape |
| Local web UI | All | Any agent that can run a hook | No |
| GitHub App or Action | All | All | No |
| VS Code extension | VS Code, Cursor, Windsurf, Kiro, Trae, VSCodium (if published to both registries) | Any | Yes, one extension |
| JetBrains plugin | All JetBrains IDEs | Any | Yes, a second codebase in Kotlin or Java |
| MCP server | Any host that speaks MCP | Any agent that calls it | No for text; UI depends on host |
| LSP server | Editors with LSP clients (VS Code, JetBrains partially, Neovim, Emacs, Zed) | None by itself; has no agent hook | Light per-editor config |

## 3. Agent coverage

### 3.1 The matrix

Columns: **Hooks** = the events I could confirm (P = prompt submit, T = tool before and after, E = turn end, S = session start and end). **Add-on** = the format a third party ships and how it is installed. **MCP** = the agent can call MCP servers. **Disk** = where it keeps transcripts, the fallback when hooks are missing. **UI** = what an add-on can show. "?" means I did not find it in a primary source.

| Agent | Hooks | Add-on and install | MCP | Disk | UI |
|---|---|---|---|---|---|
| Claude Code | P, T, E, S, plus permission, subagent, compaction, config events; handlers are command, http, mcp_tool, prompt or agent ([hooks](https://code.claude.com/docs/en/hooks)) | Plugin from a marketplace: skills, hooks, MCP, LSP, mods, `bin/` ([manifest](https://code.claude.com/docs/en/plugins-reference)); `claude plugin install` | Yes | `~/.claude/projects/<project>/<session>.jsonl`; subagents under `<session>/subagents/` ([directory docs](https://code.claude.com/docs/en/claude-directory)) | Mods (panes, bands, buttons), status line, `systemMessage` ([mods](https://code.claude.com/docs/en/plugins/mods/overview.md)) |
| Codex CLI | P, T (`PreToolUse`, `PostToolUse`), E (`Stop`), S, `PermissionRequest`, `SubagentStart/Stop`, `Interrupt` ([hooks](https://learn.chatgpt.com/docs/hooks)) | `~/.codex/hooks.json`, `.codex/hooks.json`, `config.toml`, or hooks bundled in a plugin; each non-managed hook needs a one-time trust in `/hooks` (same page) | Docs have an MCP page; **not read** | `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`, per third-party tools only (**unverified** against official docs) | `systemMessage` warning; context injection |
| Codex IDE extension | ? whether hooks run (**unverified**) | VS Code family, Xcode and JetBrains integrations ([IDE](https://learn.chatgpt.com/docs/codex/ide)) | ? | Same as CLI (**unverified**) | ? |
| Gemini CLI | S, `BeforeAgent`, `AfterAgent` (P and E), `BeforeModel`, `AfterModel`, `BeforeTool`, `AfterTool`, `Notification`; command type only ([hooks](https://github.com/google-gemini/gemini-cli/blob/main/docs/hooks/index.md)) | Extension from a GitHub URL: prompts, MCP servers, commands, hooks, sub-agents, skills ([extensions](https://geminicli.com/docs/extensions/)); `gemini extensions install <url>` | Yes (extensions bundle MCP) | `~/.gemini/tmp/<project_hash>/chats/`, kept 30 days by default; file format not stated ([sessions](https://github.com/google-gemini/gemini-cli/blob/main/docs/cli/session-management.md)) | ? (only hook JSON output) |
| Antigravity (`agy`) | S, `PreInvocation`, `PostInvocation`, `PreToolUse`, `PostToolUse`, `Stop` (this repo: `plugin-agy/hooks.json`, `core/src/agy.rs`) | `plugin.json` plus `hooks.json` in `plugin-agy/`. Vendor docs **not read**; install path **unverified** | ? | `transcript_full.jsonl` under `brain/<id>/.system_generated/` (`core/src/agy.rs:37`, `core/tests/fixtures/agy/brain/`); path comes in the hook payload (`core/src/agy.rs:143`) | ? |
| OpenCode | Plugin hooks: `session.hook("prompt", ...)`, `tool.hook("execute.before"/"execute.after")`, `permission.hook("evaluate")`, `shell.hook("create.before")`, plus an `ctx.event.subscribe` stream whose type names the doc does not list ([plugin API](https://opencode.ai/docs/build/plugins/server)) | npm package or local directory in `opencode.json` `plugins`; `.opencode/plugins/`, `~/.config/opencode/plugins/`; `opencode plugin add <spec>` ([plugins](https://opencode.ai/docs/plugins/)) | ? (server plugins can add "integrations") | ? | Desktop: `./gui-experimental` panels, commands, settings; TUI plugins in `cli.json`; no toast documented |
| Cursor (agent) | S, P (`beforeSubmitPrompt`), T (`preToolUse`, `postToolUse`, shell, MCP, file edit), E (`stop`, `afterAgentResponse`), subagent, `preCompact` ([hooks](https://cursor.com/docs/agent/hooks)) | `.cursor/hooks.json` (project), `~/.cursor/hooks.json` (user), enterprise and team; plugins can carry hooks | Yes (MCP hooks exist) | ? | `user_message` and `permission: "ask"` only; cloud agents run most hooks but not session, MCP or Tab hooks; CLI coverage not stated |
| GitHub Copilot CLI and cloud agent | S, P (`userPromptSubmitted`), T, E (`agentStop`, with `transcriptPath`), subagent, error, compaction, notification ([reference](https://docs.github.com/en/copilot/reference/hooks-configuration)); cloud agent runs a subset | `.github/hooks/*.json`; `~/.copilot/hooks/*.json` for the CLI ([about hooks](https://docs.github.com/en/copilot/concepts/agents/coding-agent/about-hooks)); VS Code is a public preview with agent-dependent events | Yes | The `agentStop` payload gives `transcriptPath` (same reference); default folder ? | ? (CLI notifications only) |
| Cline | SDK plugin hooks: `beforeRun`, `afterRun`, `beforeModel`, `afterModel`, `beforeTool`, `afterTool`, `onEvent`; 3 s timeout ([plugins](https://docs.cline.bot/sdk/plugins.md)) | `cline plugin install` from file, npm, git or path; the page does not say whether the VS Code extension runs them (**unverified**) | ? | ? | ? |
| Roo Code | ? | The Roo landing page says "The Roo Code Extension was shut down on May 15th" and points to ZooCode and Cline ([page](https://roocodeinc.github.io/Roo-Code/)) | ? | ? | ? |
| Kiro | T (`PreToolUse`, `PostToolUse`), E (`Stop`), `PostFileSave`; full trigger list on a page I did not open ([hooks](https://kiro.dev/docs/hooks/)) | `.kiro/hooks/*.json` per project; the CLI also loads `~/.kiro/hooks/`; actions are a shell command (JSON on stdin) or an agent prompt | Yes (matchers `@mcp`, `@<server>`) | ? | ? |
| Windsurf (Cascade) | `PreToolUse` confirmed; the docs moved to Devin Local hooks and the other events were not on the page I read ([hooks](https://docs.devin.ai/desktop/cascade/hooks)) | `.devin/hooks.v1.json`, project or user; not run in Restricted Mode | ? | ? | ? |
| Amp | `session.start`, `tool.call` (can reject), `tool.result`, `agent.start`, `agent.end` ([plugin API](https://ampcode.com/manual/plugin-api)) | Bun-run JS or TS in `.amp/plugins/` or `~/.config/amp/plugins/`; `plugins: reload` | Yes (MCP page exists; **not read**) | ? | Richest of all: `notify`, `input`, `select`, `confirm` dialogs, commands, tools, skills, open URL |
| Goose | None found ([repo](https://github.com/block/goose)) | MCP extensions; `goose acp` for editors | Yes | ? | ? |
| Aider | None. `--notifications-command` runs a command when a reply is ready ([options](https://aider.chat/docs/config/options.html)) | No plugin system or MCP on the options page | No (not on the page) | `.aider.chat.history.md`, `.aider.input.history`, optional `--llm-history-file`; commits changes by default with an "aider" author or co-author trailer (same page) | Terminal only |
| Zed's own agent | None found ([agent settings](https://zed.dev/docs/ai/agent-settings)) | External agents are added over ACP; extension-provided agents are deprecated ([external agents](https://zed.dev/docs/ai/external-agents)) | Yes | ? | ? |

### 3.2 What the matrix says

- **Everyone converged on the same shape.** Claude Code, Codex, Gemini, Cursor, Copilot, Kiro and Windsurf all run a command, give it JSON on stdin, and read JSON back. Amp, OpenCode and Cline instead load a function in-process. The recorder's input is therefore one of two things: a command's stdin, or an HTTP POST from a plugin function. The repo's `studio-hook` already does the first.
- **The same four moments exist almost everywhere**: prompt submitted, tool before and after, turn end, session start and end. Those four are enough for the headline, the turn summary and the turn trees (`understanding/src/turn_trees.rs`). The cards also need the tree of the worktree at turn start and end, which a hook records; where an agent has no `stop` hook (Aider, Goose, Zed's agent), `git` itself is the end-of-turn signal, because Aider commits after each change by default (options page).
- **Differences that cost real time**: field names (Copilot sends camelCase unless the event is configured in PascalCase, which switches it to snake_case, same reference), trust gates (Codex needs `/hooks` approval), hook timeouts (Cline 3 s, Copilot 30 s default, Kiro 60 s), and cloud variants that run only a subset (Cursor, Copilot).
- **The agent often hands over its own transcript.** Copilot's `agentStop` includes `transcriptPath`; Antigravity's hook payload includes `transcriptPath` (`core/src/agy.rs:143`); Claude Code's transcript is at a documented path. Where the path is not given, the file watcher needs a per-agent parser, which is the costly case.

### 3.3 ACP: one tap point?

The Agent Client Protocol "standardizes communication between code editors/IDEs and coding agents": JSON-RPC over stdio for local agents, HTTP or WebSocket for remote ones, with remote support still in progress ([overview](https://agentclientprotocol.com/overview/introduction)).

- **Agents that speak it** (registry page): Claude Agent, Codex CLI (through an adapter), Gemini CLI, Cline, Cursor, GitHub Copilot (public preview), Goose, OpenCode, Kiro CLI, Junie, Qwen Code, Factory Droid, OpenHands and many more ([agents](https://agentclientprotocol.com/overview/agents)). Antigravity, Amp, Aider and Windsurf are not on that list.
- **Editors that host them**: Zed, JetBrains, Neovim (CodeCompanion, avante.nvim and others), Emacs, Sublime Text, VS Code through several third-party extensions, Visual Studio through Poolside, Qt Creator, Pulsar ([clients](https://agentclientprotocol.com/overview/clients)). Zed adds agents from an ACP registry or an `agent_servers` entry with a command and args ([Zed](https://zed.dev/docs/ai/external-agents)).
- **Is it a single tap point?** No. ACP is the wire between an editor and an agent it launched; it sees nothing of an agent run in a plain terminal, and the editor (the client) is the one that owns the session. A recorder could sit in the middle as a proxy that the editor launches in place of the agent (the `agent_servers` command is just a command), but that is my inference: I did not find a published design for it or whether session updates carry the same four moments. Treat it as a later adapter for editor-hosted agents, not the foundation. **Unverified.**

### 3.4 User base by agent

The only primary figures I have are the Stack Overflow 2025 survey's: Cursor 17.9%, Claude Code 9.7%, Windsurf 4.9%, Cline and/or Roo 2.2%, Aider 1.9%, Trae 0.8% ([survey](https://survey.stackoverflow.co/2025/technology#1-dev-id-es)). Copilot, Codex, Gemini CLI, OpenCode, Amp, Kiro, Goose and Antigravity were not in that list, so I have no primary count for them. WakaTime's list of tools it tracks includes Claude Code, Codex CLI, Copilot CLI, Cursor, Kiro, OpenCode, Amp CLI, Antigravity, Trae and Qoder ([plugins](https://wakatime.com/plugins)); that is evidence the agents exist and are tracked, not their size. Ordering by share for support priority: Cursor and Claude Code (both hook-capable) first; Copilot, Codex and Gemini next (hook-capable, size **unverified**); then the rest.

## 4. Comparison across surfaces

| Surface | Can render cards and a quiz | Learns of agent activity | Install friction | Main limit |
|---|---|---|---|---|
| Recorder plus agent adapters (hooks, plugin functions, file and git watching) | Via the web page | Direct where hooks exist; by files and git otherwise | One adapter per agent | Needs a screen elsewhere |
| Hooks only | No UI; `systemMessage`, `additionalContext`, terminal notifications | Direct, by event | One plugin install | No place to draw a card |
| Claude Code mod | Pane beside the transcript or a band above the prompt, with tabs, buttons and text fields | Direct, inside the process | Plugin install from a marketplace | Claude Code only; a pane in a terminal is text-sized |
| Status line | One or more text lines | Reads session JSON | A setting | Text only |
| MCP server | Text, plus MCP Apps iframes and elicitation forms where the host supports them | The model must call a tool; it sees nothing on its own | Add a server | Agent-initiated; no passive observation |
| Local web UI | Anything a browser draws | By hook, over a socket or the event log | A command and a tab | A second window |
| VS Code extension | Webview panels, sidebar views, status bar | Reads the event log or a socket | Marketplace install | Webviews are heavy; fork registries differ |
| JetBrains plugin | Tool window; JCEF browser (**unverified** in the doc I read) | Same | JetBrains Marketplace | Separate codebase |
| GitHub check run or review comment | Markdown, images, annotations, up to three buttons | PR content and what CI is given | Install an App | Not at the moment of work; needs the turn data pushed |
| TUI in a tmux pane | Text, keyboard quizzes | Tails the event log | Run a command | Terminal users only |
| Menubar or tray app | Notifications, small popover | Local socket | Install an app | Back to a desktop app, smaller |

## 5. Per-surface findings

### 5.1 Claude Code's own extension points

- **Hooks.** Events cover session, turn, tool call, subagent, compaction, config and notification (`SessionStart`, `UserPromptSubmit`, `Stop`, `PreToolUse`, `PostToolUse`, `PermissionRequest`, `SubagentStop`, `Notification` and more). Handlers can be `command`, `http`, `mcp_tool`, `prompt` or `agent`. Output can inject up to 10,000 characters of `additionalContext`, show a `systemMessage` warning, block, or send terminal escape sequences (titles, notifications). Hooks "can't write to `/dev/tty`" and have no UI of their own ([hooks reference](https://code.claude.com/docs/en/hooks)). The `http` hook type means a hook can POST straight to a local server.
- **Plugins.** A plugin can ship skills, commands, agents, hooks, MCP servers, LSP servers, output styles, monitors, themes and `bin/` executables; `userConfig` prompts for settings; `${CLAUDE_PLUGIN_DATA}` keeps state across updates ([plugin manifest reference](https://code.claude.com/docs/en/plugins-reference)). Distribution is by marketplace. The studio loads its plugin in place today instead (`docs/studio/plugin.md`).
- **Mods (the UI extension point).** A mod is a plugin whose JavaScript handlers run inside Claude Code and can "draw an interface you can use": a pane beside the transcript or a band above the prompt, with tabs, buttons and text fields; hold a tool call; add a `/command`. Mods work in the CLI, the Desktop app's Code tab and the VS Code extension (2.1.299 or later); in `claude -p` and the Agent SDK hooks run but nothing draws. Mods are not sandboxed and see every prompt and tool call ([mods overview](https://code.claude.com/docs/en/plugins/mods/overview.md)). Anthropic's own samples include a pane that holds a risky command and shows buttons, and a `/replay` of the last turn's edits (same page). This is the closest official surface to a card. **Risk:** it is new (v2.1.287 or later for the terminal), and the doc says nothing about stability guarantees; treat the API as likely to move.
- **Status line.** A script gets session JSON on stdin and prints text in a bar under the prompt ([status line](https://code.claude.com/docs/en/statusline)). Good for the headline and an open-card count, not for a quiz.
- **IDE integrations.** The VS Code extension has a graphical panel, shares history with the CLI, and works in Cursor and, through Open VSX, other forks; its page names Cursor and says other forks install from the Extensions view or Open VSX ([VS Code docs](https://code.claude.com/docs/en/vs-code)). The JetBrains plugin only runs the `claude` CLI in the IDE terminal and connects through a local `ide` MCP server for diffs, selection and diagnostics ([JetBrains docs](https://code.claude.com/docs/en/jetbrains)). So a mod's pane reaches the VS Code panel but, in JetBrains, only what the terminal draws.
- **Agent SDK / `claude -p`.** The writer already uses `claude -p` (`understanding/src/summary.rs`). That keeps model calls on the user's own login, with no key for the studio to hold.

### 5.2 Codex CLI and Gemini CLI

- **Codex.** Hooks exist with `SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PermissionRequest`, `PostToolUse`, `Stop` and more; JSON on stdin; `systemMessage` and `additionalContext` outputs; config in `~/.codex/hooks.json`, `.codex/hooks.json` or `config.toml`, and plugins can bundle hooks; non-managed hooks must be trusted once in `/hooks` ([Codex hooks](https://learn.chatgpt.com/docs/hooks)). The Codex IDE extension covers VS Code, Cursor, Windsurf and Insiders; Xcode and JetBrains have their own integrations ([Codex IDE](https://learn.chatgpt.com/docs/codex/ide)). Whether the IDE extension runs the same hooks is not stated on that page (**unverified**).
- **Gemini CLI.** Hooks: `SessionStart`, `SessionEnd`, `BeforeAgent`, `AfterAgent`, `BeforeModel`, `AfterModel`, `BeforeTool`, `AfterTool`, `Notification`; command type only; JSON on stdout; extensions can define hooks ([Gemini hooks](https://github.com/google-gemini/gemini-cli/blob/main/docs/hooks/index.md)). Extensions bundle prompts, MCP servers, commands, themes, hooks, sub-agents and skills, installed with `gemini extensions install <github url>`, with a gallery ([Gemini extensions](https://geminicli.com/docs/extensions/)). That page also says unpaid-tier users moved to the Antigravity CLI on 18 June 2026, which matches the existing `plugin-agy/`.
- **Pattern.** Three agents, three hook dialects, one shape: a command that reads JSON on stdin. Section 3 extends this to Cursor, Copilot, Kiro, Windsurf, Amp, OpenCode, Cline and Aider.

### 5.3 IDE extensions

- **VS Code API.** Webviews act like an iframe the extension controls, in panels, custom editors or sidebar and panel views; state is lost when hidden unless saved with `getState` or a serializer; `retainContextWhenHidden` costs a lot of memory; the docs say to use webviews only when native APIs are inadequate ([webview guide](https://code.visualstudio.com/api/extension-guides/webview)). Extensions can also register MCP servers with `vscode.lm.registerMcpServerDefinitionProvider` ([VS Code MCP guide](https://code.visualstudio.com/api/extension-guides/ai/mcp)). CodeLens, the Comments API, chat participants and notebooks exist in the API; I did not read those pages, so their fit for a quiz is **unverified**.
- **Forks.** The Microsoft Marketplace and Open VSX are separate; publishing to both takes two CLIs (`vsce`, `ovsx`) and two tokens, reusing one `.vsix` ([publishing guide](https://code.visualstudio.com/api/working-with-extensions/publishing-extension); the two-token detail comes from a secondary dev.to guide, **unverified** against primary docs). Open VSX is the registry for VSCodium and similar ([Wikipedia](https://en.wikipedia.org/wiki/Open_VSX), secondary). Cursor's registry behaviour is **unverified** (one tutorial claims it does not use Microsoft's marketplace). Windsurf: **unverified**.
- **JetBrains.** Tool windows are registered in `plugin.xml` with a `ToolWindowFactory` ([tool windows](https://plugins.jetbrains.com/docs/intellij/tool-windows.html)). Embedding a browser with JCEF is the likely way to reuse a web UI, but the page I read did not cover it (**unverified**).
- **Maintenance.** A second plugin means a second language, a second release channel and a second review. Continue maintains both a VS Code extension and a JetBrains plugin from one repo with a shared `core/`, and its maintainers now recommend its CLI over the JetBrains plugin ([Continue README](https://github.com/continuedev/continue)); the repo is read-only and no longer maintained, a warning about what two plugins cost.

### 5.4 MCP as a cross-agent surface

- **MCP Apps.** A tool declares `_meta.ui.resourceUri` pointing at a `ui://` HTML resource; the host renders it in a sandboxed iframe in the conversation, and the app can call tools back over `postMessage`. Listed hosts: Claude, Claude Desktop, VS Code GitHub Copilot, Microsoft 365 Copilot, Goose, Postman, MCPJam, Archestra ([MCP Apps overview](https://modelcontextprotocol.io/extensions/apps/overview)). The extension support matrix lists Claude Code and Cursor as rows, but the tick marks did not survive my fetch, so their support is **unverified** ([client matrix](https://modelcontextprotocol.io/extensions/client-matrix.md)). VS Code shows apps only inline, treats the feature as experimental, and "Send message" fills the chat box without sending ([VS Code MCP guide](https://code.visualstudio.com/api/extension-guides/ai/mcp)).
- **Elicitation.** A server can ask the user for a flat form of strings, numbers, booleans and enums (no nested data), or send them to a URL. That is enough for a multiple-choice question but not for a card with code ([MCP elicitation, draft spec](https://modelcontextprotocol.io/specification/draft/client/elicitation)). VS Code Copilot supports both modes during an active turn (VS Code MCP guide).
- **Fit.** MCP is pull-based: the model decides to call the server. It cannot watch a turn. It suits "show me the open cards" as a tool, and a quiz returned as an elicitation, but not the recorder. Hooks stay the sensor; MCP is an optional second door.

### 5.5 Git-native and PR surfaces

- **Check runs.** `output.summary` and `text` take Markdown up to 65,535 characters each; annotations (50 per request) tie notes to lines; up to three action buttons send a `check_run.requested_action` webhook back; **only GitHub Apps can create check runs** ([check runs API](https://docs.github.com/en/rest/checks/runs)). That makes a PR debrief with a few questions possible, but a quiz with state needs a hosted App, which is server upkeep.
- **Review tools in practice.** CodeRabbit reviews PRs on GitHub, GitLab, Azure DevOps and Bitbucket, ships an extension for VS Code, Cursor, Windsurf and JetBrains, and a CLI that "works with Claude Code, Cursor, Codex, Gemini, and more" ([CodeRabbit docs](https://docs.coderabbit.ai/)). Its PR surface is the product; the editor and CLI are the same review at an earlier moment.
- **Git notes.** I could not find a GitHub REST API or UI for git notes (the docs page I tried returned 404), so assume notes are not shown on github.com (**unverified**). Not a delivery surface for a reader.

### 5.6 Terminal-native

A TUI beside the agent (tmux pane) or plain CLI reads the same event log. It reaches terminal-only users and costs little: no webview, no registry. herdr, studied in `docs/studio/research/herdr.md`, shows the other direction (a terminal multiplexer that draws agent state), and it chose a separate native app plus a TUI that do not share agents. For the understanding layer, a quiz in a terminal is possible but poor for code: the card has code ranges (`understanding/src/card_code.rs`).

### 5.7 Local web UI, menubar and LSP

- **Local web UI.** Vibe Kanban runs with `npx vibe-kanban`, opens a web UI in the browser, works with Claude Code, Codex, Gemini CLI, Copilot, Amp, Cursor, OpenCode and others, with a Rust backend and a separate web app; its README now says the project is shutting down ([repo](https://github.com/BloopAI/vibe-kanban)). The Claude Code multi-agent observability project has hooks POST to a Bun server on port 4000, stored in SQLite and pushed over WebSocket to a Vue dashboard on 5173 ([repo](https://github.com/disler/claude-code-hooks-multi-agent-observability)). Both are the pattern: hook, local server, browser.
- **Menubar or tray.** WakaTime suggests its Desktop App in the system tray for editors it has no plugin for ([WakaTime plugins](https://wakatime.com/plugins)). This is a fallback, and it brings back the desktop-app problem in a smaller form.
- **LSP.** Claude Code consumes LSP servers for code intelligence ([plugin reference](https://code.claude.com/docs/en/plugins-reference)); the protocol has no card or quiz concept, only diagnostics, code lenses and code actions. As a channel for pushing understanding into an editor it would reach VS Code, Neovim, Emacs and Zed with one server, but it only draws squiggles, lenses and hovers. Whether a "this line changed in turn 4, ask me why" lens is worth it is an open question. I did not verify editor LSP support from primary docs here.

## 6. What others do

The pattern in one line: a core that does the work once, then the thinnest possible client wherever the user already is, and the PR or the CLI as the surface that needs no editor at all.

| Product | What it adds | Surfaces it ships on | Editors and agents supported | How it reaches many editors |
|---|---|---|---|---|
| GitHub Copilot | Completion and chat | IDE plugins; CLI | VS Code, Visual Studio, JetBrains, Xcode, Vim/Neovim, Eclipse, Azure Data Studio; Zed not confirmed ([install docs](https://docs.github.com/en/copilot/how-tos/set-up/install-copilot-extension)) | A plugin per editor; a company-sized team. Its GitHub MCP server is listed for VS Code, Cursor, Claude, Windsurf, JetBrains, Visual Studio, Xcode and Zed ([repo](https://github.com/github/github-mcp-server)): MCP as the cheap reach layer. |
| Codex | Coding agent | CLI, IDE extension, desktop, web, cloud | VS Code, Cursor, Windsurf; Xcode and JetBrains through their own AI chat panels ([docs](https://learn.chatgpt.com/docs/codex/ide)) | One extension for the VS Code family; other IDEs host it through their own agent slot. |
| Claude Code | Coding agent | CLI, VS Code extension, JetBrains plugin, Desktop | VS Code and forks, JetBrains, any terminal ([VS Code](https://code.claude.com/docs/en/vs-code), [JetBrains](https://code.claude.com/docs/en/jetbrains)) | The CLI is the product; the JetBrains plugin only launches it and runs an `ide` MCP server for diffs and selection. The IDE is a thin client. |
| Continue | Open assistant | VS Code, JetBrains, CLI | VS Code (both registries), JetBrains ([README](https://github.com/continuedev/continue)) | `core/` plus `extensions/{vscode,intellij,cli}`. Now read-only and unmaintained, with the CLI recommended: upkeep of per-editor clients ended it (my reading of the README). |
| WakaTime | Time tracking | Plugins, desktop app | 87 listed editors and tools, including Claude Code, Codex CLI, Cursor, Neovim, Vim, Xcode, Zed, Emacs ([plugins list](https://wakatime.com/plugins)) | Per-editor plugins, many maintained by others; the page suggests the tray app for the rest. Whether they share a `wakatime-cli` is **unverified** by that page. Shows how far a tiny, passive capability can spread. |
| CodeRabbit | PR review | PR comments; IDE extension; CLI | GitHub, GitLab, Azure DevOps, Bitbucket; VS Code, Cursor, Windsurf, JetBrains; CLI with Claude Code, Cursor, Codex, Gemini ([docs](https://docs.coderabbit.ai/)) | The PR is the primary surface and needs no editor. IDE and CLI come second. |
| GitLens | Git history and blame | VS Code extension | VS Code only ([repo](https://github.com/eamodio/vscode-gitlens)) | It chose depth in one editor, and ships a separate GitKraken MCP for agents. |
| Pieces | Long-term memory of work | A local app (PiecesOS) and IDE plugins | The page I read did not list editors (**unverified**) | A local daemon on the device that plugins talk to ([Pieces docs](https://docs.pieces.app/products/quick-guides/ltm-context)): the same shape as hook, store, client. |
| Vibe Kanban | Plan and review agent work | `npx` local web UI | Claude Code, Codex, Gemini CLI, Copilot, Amp, Cursor, OpenCode and more; editor opening by SSH integration ([repo](https://github.com/BloopAI/vibe-kanban)) | Local server plus browser plus an MCP server. Sunset notice in its README: reach did not save it, which says nothing about the design but a lot about business. |
| ccusage | Token and cost reports | CLI, status line | 20 agent CLIs including Claude Code, Codex, Gemini CLI, Copilot CLI, Cline ([repo](https://github.com/ryoppippi/ccusage)) | Reads each agent's local data and runs under `npx`; a status line command for Claude Code and Codex. No editor code. |
| Hooks observability (disler) | Live event timeline | Hooks plus local server plus browser | Claude Code ([repo](https://github.com/disler/claude-code-hooks-multi-agent-observability)) | Hooks POST to a server; the browser is the screen. |
| SonarQube for IDE, Snyk, Tabnine, Cody, Windsurf plugins, CodeStream, Swimm, Stepsize, Graphite, Warp, Supermaven | Not read | | | **Unverified.** I did not fetch their docs. The one Sonar page I read covered only IntelliJ connected mode and did not describe its architecture. Add them only if the owner wants a wider table. |

**The pattern.**

1. **Agent-facing tools reach editors through the agent.** ccusage, the observability dashboard, Vibe Kanban and Claude Code's own JetBrains plugin do not write editor code for the capability; they read the agent's data or hooks.
2. **Editor-facing tools pay per editor.** Copilot, WakaTime and Continue ship a client for each editor. Only Copilot sustains it as a company; Continue stopped.
3. **Review tools put the PR first.** CodeRabbit's editor and CLI repeat what the PR does.
4. **Local daemon plus web UI is the common middle.** Vibe Kanban, the observability project and Pieces all keep state in a local process and show it in a browser or a thin client.
5. **MCP is the reach layer for pull-style features.** GitHub's server lists eight hosts; GitKraken's MCP gives agents Git data.

## 7. Recommendation: widest reach, lowest upkeep, agent-neutral

**Shape.** One recorder, one adapter per agent, one shared screen.

```
agent hooks / plugin functions  -->  adapter  -->  recorder (studio-hook, core, understanding)
transcript or git-tree watcher  -->  adapter  -->        |
                                                          v
                          local web page (cards, quiz, headline, summary) in any browser
                          optional: agent's own UI hook, status line, PR check run
```

**What to build, in order:**

1. **(M) Make the adapter model open.** Replace the two-name match in `Capabilities::deserialize` (`core/src/event.rs:240-245`) and the two-variant `TranscriptFormat` (`core/src/event.rs:249-254`) with a registry, so a new agent is one `CAPABILITIES` constant plus one parser, as `core/src/claude.rs:14` and `core/src/agy.rs:24` already are. Today Codex is a status-only row with no card data (`core/tests/status_only.rs`).
2. **(M) Add a git-tree poller as the universal `turn_trees` source.** `turn_trees` is a hook-recorded capability and false for agy (`core/src/agy.rs:33`). A poller that snapshots the worktree's tree when a prompt arrives and when the turn ends would give every tier-A, B and C agent the same turn files. This is my inference of how to cover agents with no stop hook (Aider commits as it goes, options page); the repo has no such poller yet.
3. **(S each) Tier A adapters, in this order**: Codex CLI, Cursor, Gemini CLI, Copilot CLI, Kiro, Windsurf. Each is a hooks file that calls `studio-hook` with an `--agent` name, plus a JSON mapper from that agent's field names to the four moments. Codex, Cursor, Gemini and Copilot are documented well enough to start (sources in 3.1); Windsurf needs its hook reference read first.
4. **(M) The local web page.** Serve the existing UI from a loopback server; every adapter's `systemMessage` or notification points to its URL. Whether `ui/` runs from a plain server is **unverified**; it is embedded as `dist/` in Tauri today. This is the one screen all agents and all editors reach.
5. **(S to M each) Tier B adapters**: OpenCode, Amp, Cline. A small plugin whose hook functions POST the event to the recorder. Amp's dialogs (`select`, `confirm`) could even carry a one-question quiz without the browser; OpenCode and Claude Code can draw panes. These are the optional per-agent UI extras.
6. **(M each) Tier C**: Aider through `--notifications-command` plus git and its history file; Zed's agent and Goose only after checking whether they expose any event (unverified).
7. **Later**: the GitHub App for the PR debrief, then one VS Code extension if asked. Skip per-editor plugins for JetBrains, Zed, Neovim, Xcode and Visual Studio.

**Why this meets the owner's goal.**

- **Reach**: it covers any editor (the agent runs in a terminal or the editor's panel, and the page is a browser tab) and, after step 3, the agents with the largest share in the one survey that counts them (Cursor 17.9%, Claude Code 9.7%).
- **Upkeep for one person**: no per-editor code; per agent, one mapper of about one hook payload each. The recorder, writer and UI are written once. The cost that grows is tracking each vendor's hook changes, which Anthropic's own docs show moving fast (hooks gained events, `http` and `mcp_tool` handler types, and mods in 2026).
- **Does not depend on any vendor's UI**: the mods, Amp dialogs and OpenCode panels are extras that can break without taking the product down.

**Risks.**

- Hook formats drift per release; a drift counter exists for Claude Code and agy (`AgyDriftKind`, `core/src/agy.rs:48`, and `weekly` in `understanding/src/bin/weekly.rs`); reuse it per adapter.
- Cloud and IDE variants run only subsets (Cursor, Copilot), so a person on the cloud agent will see fewer cards.
- A browser tab beside the editor may feel like the same switch the owner wants to avoid. Test with two people before adding more agents.
- Trust prompts (Codex `/hooks`) add an install step the first time.

## 8. Open questions for the owner

1. Which agents do the first ten users actually run? That sets the order in step 3; I have a primary share count only for Cursor, Claude Code, Windsurf, Cline/Roo, Aider and Trae.
2. Is a card with a quiz the product, or the headline and turn summary? The summary fits a status message and a PR comment in any agent; the quiz needs the page.
3. Is a browser tab acceptable as the one shared screen, or must the card appear inside the editor? That decides between step 4 and a VS Code extension.
4. Should git-tree polling become the primary turn-file source for every agent, with hooks only as a faster signal? It would simplify adapters but gives up the hook's exact turn boundary.
5. Does the writer keep calling `claude -p` (`understanding/src/summary.rs`) for every agent's turns, so a Codex-only user needs a Claude login? Or does each agent summarise its own work? This is a hard dependency for users without Claude Code.
6. Would a teammate who never installed anything value a PR check? That decides whether the GitHub App is worth hosting.
7. Keep the desktop app as the full control room for the owner, with the recorder, plugins and page as the lighter edition for everyone else?

## 9. Unverified or not fully read

- Antigravity: vendor docs, plugin install path, UI hooks, MCP.
- Codex: official transcript location (only third-party tools), MCP page, whether the IDE extension runs hooks.
- Gemini CLI: session file format, UI beyond hook output.
- Cursor: transcript location, hooks in the CLI versus the IDE, user base beyond the survey.
- Copilot: default transcript folder, hooks in VS Code (public preview).
- Cline: whether the VS Code extension runs SDK plugin hooks; the older shell-script hooks docs redirected to the SDK page.
- Roo Code: the shutdown claim is from one landing page; nothing else read.
- Kiro: full trigger list; Windsurf and Devin Local: events beyond `PreToolUse`.
- Amp: transcript location and MCP; Goose and Zed's own agent: hooks, storage (none found, not exhaustive).
- OpenCode: event type names, transcript location, MCP.
- ACP: whether a proxy can sit between editor and agent, and what session updates carry.
- JetBrains survey editor shares; MCP Apps support in Claude Code and Cursor; hook support in Aider beyond `--notifications-command`.
- Companies in section 6 not fetched: SonarQube, Snyk, Tabnine, Cody, CodeStream, Swimm, Stepsize, Graphite, Warp, Supermaven, Crystal, Conductor.
- Secondary sources used: dev.to and Wikipedia for registry publishing, third-party tools for the Codex path.
- Whether `ui/` runs from a plain local server; whether `src-tauri/` holds card logic that belongs in `understanding/`.
