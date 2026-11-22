# Harness facts for the 8-harness choice (fetched 2026-11-22)

Legend: C = Claude-shaped hooks, M = command hooks with a mapping needed, P = in-process plugin. "unverified" = not found in a primary source.

## 0. Cross-cutting popularity data

- GitHub stars and npm weekly downloads (week 2026-11-15..21), read via `gh api` and `api.npmjs.org` on 2026-11-22:
  - anthropics/claude-code 156,663 stars; npm `@anthropic-ai/claude-code` 22.2M/wk
  - openai/codex 136,133; `@openai/codex` 27.6M/wk
  - sst/opencode (= anomalyco/opencode) 228,537; `opencode-ai` 3.60M/wk
  - google-gemini/gemini-cli 108,993; `@google/gemini-cli` 557k/wk
  - cline/cline 71,821; `cline` 82k/wk
  - block/goose 56,866 (no npm main package checked)
  - QwenLM/qwen-code 30,863; `@qwen-code/qwen-code` 147k/wk
  - charmbracelet/crush 29,163; `@charmland/crush` 14k/wk
  - Kilo-Org/kilocode 28,085
  - github/copilot-cli 11,452; `@github/copilot` 1.72M/wk
  - Amp: no public repo (sourcegraph/amp 404); `@sourcegraph/amp` 20k/wk (the current npm name may differ: unverified)
  - Factory Droid: closed source; `@factory/cli` 29k/wk (unverified that this is the Droid package); npm `droid` 13.6k/wk is probably an unrelated package
  - Kiro: kirodotdev/Kiro 4,487 stars (issue-tracker repo, not the source); Cursor: getcursor/cursor 33,350 stars (issues repo, last push 2026-05)
- Stack Overflow 2025 Developer Survey, "Dev IDEs" (26,143 respondents, all respondents): Cursor 17.9%, Claude Code 9.7%, Zed 7.3%, Windsurf 4.9%, Cline and/or Roo 2.2%; Kiro, Antigravity not listed. [survey.stackoverflow.co/2025/technology](https://survey.stackoverflow.co/2025/technology)
- Stack Overflow 2025, "AI agent out-of-the-box tools" (asked of agent users, 8,323 responses): GitHub Copilot 67.9%, Claude Code 40.8%, Roo Code 3.4%, Devin 2.7%, OpenHands 1%; Cursor, Windsurf, Cline, Codex, Gemini CLI not in the list. [survey.stackoverflow.co/2025/ai](https://survey.stackoverflow.co/2025/ai). Overall: 31% of developers use AI agents (search summary of the same survey).
- JetBrains State of Developer Ecosystem 2025 (24,534 respondents): 85% regularly use AI tools; 62% use at least one AI coding assistant, agent or editor ([blog](https://blog.jetbrains.com/research/2025/10/state-of-developer-ecosystem-2025/), via search snippet). Per-tool figures: the April 2026 JetBrains post "which AI coding tools do developers actually use at work" returned HTTP 403, so per-tool numbers are **unverified**. JetBrains blog ranks Copilot, JetBrains AI Assistant, Cursor, Windsurf, Tabnine (marketing post, [link](https://blog.jetbrains.com/ai/2026/02/the-most-popular-ai-tools-what-developers-use-and-why)).
- Vendor-reported user counts: none searched beyond the above (see per-harness notes). **unverified** for Cursor/Copilot MAU.

## 1. Gemini CLI vs Antigravity CLI

- Announced 2026-05-19 (I/O): from 2026-06-18 Gemini CLI and Gemini Code Assist IDE extensions stop serving Google AI Pro/Ultra subscribers and free individuals. Still works with paid Gemini / Gemini Enterprise Agent Platform API keys; Code Assist Standard/Enterprise licences are unaffected. [Google developers blog](https://developers.googleblog.com/an-important-update-transitioning-gemini-cli-to-antigravity-cli/)
- Repo stays open source, Apache 2.0, "no changes" (quote attributed to Dmitry Lyalin); Google plans model updates and enterprise security fixes. [The Register](https://www.theregister.com/ai-ml/2026/05/20/bye-bye-gemini-cli-google-nudges-devs-toward-antigravity/5243605)
- So: for subscription (consumer) users, Gemini CLI has been dead since 2026-06-18; it lives on for API-key and enterprise users. This resolves the "end date disputed" item in section 10 of cross-harness-capture.md. npm downloads of `@google/gemini-cli` are 557k/wk vs Codex 27.6M.
- Antigravity CLI hooks (primary, [docs](https://antigravity.google/docs/hooks.md)): `.agents/hooks.json` (workspace) or `~/.gemini/config/hooks.json` (global; `~/.gemini/antigravity-cli/settings.json` is NOT loaded); plugin `hooks.json`; `/hooks` lists them. Events: `SessionStart`, `PreToolUse`, `PostToolUse` (matcher regex), `PreInvocation`, `PostInvocation`, `Stop`. Handler `type` only `command`, timeout default 30 s. Stdin JSON, camelCase fields: `conversationId`, `workspacePaths`, `transcriptPath`, `artifactDirectoryPath`, `modelName`, `lastUserInput`; tool events `toolCall{name,args}`, `stepIdx`; Stop has `terminationReason`, `fullyIdle`. Output: `decision` allow/deny/ask etc. Exit codes and trust prompt: not on that page (**unverified**). Hooks carry over from Gemini CLI to Antigravity CLI per Google's post. Headless mode of `agy`: not mentioned in the sources read (**unverified**).

## 2. Cursor (IDE and `cursor-agent`/`agent`)

- Popularity: SO 2025 Dev IDEs 17.9% (above). Vendor user count: **unverified**.
- Headless ([docs](https://cursor.com/docs/cli/headless)): `agent -p "<prompt>"` (`--print`); `--output-format text|json|stream-json`. No documented read-only flag or tool-disable; without `--force`, print mode still writes files in the workspace and runs allowlisted commands but rejects actions needing approval; `permissions.allow/deny` rules exist. Docs show `CURSOR_API_KEY` for scripts; interactive login is on a separate auth page (not read), so whether `-p` runs on the subscription login is **unverified** (likely yes via `agent login`). Admins can disable headless mode.
- Hooks ([docs](https://cursor.com/docs/hooks)): `~/.cursor/hooks.json` (user), `<project>/.cursor/hooks.json`, enterprise MDM path, team dashboard; plugin hooks load afterwards. Events (camelCase): sessionStart/End, preToolUse, postToolUse(+Failure), subagentStart/Stop, beforeShellExecution, afterShellExecution, beforeMCPExecution, afterMCPExecution, beforeReadFile, afterFileEdit, beforeSubmitPrompt, preCompact, stop, afterAgentResponse, afterAgentThought, tab hooks, workspaceOpen. Common payload: `conversation_id`, `generation_id`, `model`, `hook_event_name`, `cursor_version`, `workspace_roots`, `user_email`, `transcript_path` (null if transcripts are disabled). Exit 2 blocks; other codes fail open (unless `failClosed`). Project hooks run only in trusted workspaces. Loads third-party (Claude Code) hooks per a "Third Party Hooks" page (not read). `CLAUDE_PROJECT_DIR` alias exists. Whether `stop` fires in the CLI: the page does not say (**still unverified**; only `workspaceOpen` is stated for desktop and CLI). Shape: M.
- Install channel ([plugins](https://cursor.com/docs/reference/plugins)): Cursor Plugins (`.cursor-plugin/plugin.json`; skills, MCP, rules, agents, commands, hooks via `hooks/hooks.json`); `.cursor-plugin/marketplace.json` for multi-plugin repos; submission reviewed by Cursor at cursor.com/marketplace/publish. The page gives no install command. Config-file install (`~/.cursor/hooks.json`) works without a marketplace.

## 3. GitHub Copilot (CLI and VS Code)

- Popularity: SO 2025 AI-agent tool list 67.9% (agent users); 11,452 stars on github/copilot-cli; `@github/copilot` 1.72M npm/wk. Vendor user count: unverified.
- Headless CLI ([docs](https://docs.github.com/en/copilot/how-tos/copilot-cli/automate-copilot-cli/run-cli-programmatically), [reference](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-programmatic-reference)): `copilot -p "<prompt>" -s` (`-s` = response only), `--output-format=json` (JSONL), `--share=file.md` writes the transcript. Tool control: `--available-tools=<list>` (whitelist), `--excluded-tools`, `--deny-tool`; unattended runs deny anything needing approval; so `--available-tools='read'` is an (inferred) read-only run. Auth: `COPILOT_GITHUB_TOKEN` > `GH_TOKEN` > `GITHUB_TOKEN`; `copilot login` exists; the docs read do not state that `-p` uses the interactive login (**unverified**, likely).
- Hooks, CLI ([reference](https://docs.github.com/en/copilot/reference/hooks-configuration)): repo `.github/hooks/*.json`, inline `hooks` in `.github/copilot/settings.json`/`settings.local.json`, and `.claude/settings.json` + `.claude/settings.local.json` are read; user `~/.copilot/hooks/*.json` (or `$COPILOT_HOME/hooks/`) and `~/.copilot/settings.json`; plugin `hooks.json`; policy dir `/etc/github-copilot/policy.d/`. Events: sessionStart/End, userPromptSubmitted, userPromptTransformed, preToolUse, permissionRequest (CLI), postToolUse(+Failure), errorOccurred, agentStop, subagentStart/Stop, preCompact, notification (CLI). camelCase names get camelCase payload; PascalCase names get snake_case payload (`hook_event_name`, `session_id`). `transcriptPath`/`transcript_path` appears only on agentStop/Stop, subagentStart/Stop, preCompact. Exit 2 = warning, except deny on preToolUse/permissionRequest; timeout 30 s fail-open. Trust: repo hooks "require workspace admission"; no general prompt documented. Shape: C (near).
- Hooks, VS Code ([docs](https://code.visualstudio.com/docs/copilot/customization/hooks)): preview. Locations `.github/hooks/*.json`, `~/.copilot/hooks/`, inline `hooks` map in `.github/copilot/settings(.local).json` and `~/.copilot/settings.json`; reads `.claude/settings.json` and `.claude/settings.local.json` too; events sessionStart, userPromptSubmitted, preToolUse, postToolUse, preCompact, subagentStart/Stop, agentStop, sessionEnd; a `PreToolUse` compat alias gets snake_case payloads with Claude tool names. The page does not mention `transcript_path` (full field list is in the external reference above, which says it is on `Stop`). Says "Local harness is deprecated and will be removed in February 2027" (cloud sessions experimental).
- Install channel ([docs](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/plugins-finding-installing)): `copilot plugin marketplace add OWNER/REPO` (also local path or Git URL), `copilot plugin install NAME@MARKETPLACE`, or install straight from a repo/URL/path; in-session `/plugin ...`. Whether Claude Code plugins load as-is: page does not say (its example adds `anthropics/claude-code` as a marketplace; **unverified** compatibility).

## 4. OpenCode (`sst/opencode`, now `anomalyco/opencode`)

- Popularity: 228,537 GitHub stars (`gh api`, 2026-11-22), npm `opencode-ai` 3.60M/wk. Not in SO 2025 lists read.
- Headless ([CLI docs](https://opencode.ai/docs/cli/)): `opencode run "<prompt>"` "designed for scripts, CI jobs"; flags for `run` not documented on that page (`--agent` unconfirmed). Read-only: the built-in `plan` agent is read-only for normal files; or `permissions` rules deny `edit`/`shell` (docs: [agents](https://opencode.ai/docs/agents/)); no env var/flag for non-interactive permission mode found. Subscription login ([providers](https://opencode.ai/docs/providers/)): OpenAI "Sign in with ChatGPT" (usage counts against the plan), GitHub Copilot device OAuth via `/connect`. Claude Pro/Max login: not in the docs read (**unverified**; Anthropic's policy text in cross-harness-capture.md section 4.1 applies to third-party use of subscription credentials).
- Storage: `opencode debug paths` / `opencode debug paths db` prints the data and DB paths ([CLI docs](https://opencode.ai/docs/cli/)): this closes the "transcript location" gap officially as "ask the CLI"; the SQLite file name `opencode.db` under `~/.local/share/opencode/` remains third-party (**unverified** as official).
- Install: plugins are TS/JS files in `~/.config/opencode/plugins/` (global) or `.opencode/plugins/` (project), or npm packages listed in `plugins` in `opencode.json(c)`, or `opencode plugin add <package>` (global), git sources like `github:acme/opencode-plugin` ([plugins](https://opencode.ai/docs/plugins/)). Event list and Claude-hook loading: not on the page read (**unverified**; earlier doc said not Claude-compatible). Trust prompt: not mentioned. Shape: P.

## 5. Gemini CLI

- Hooks ([docs](https://geminicli.com/docs/hooks/)): `settings.json` layers: project `.gemini/settings.json`, user `~/.gemini/settings.json`, system `/etc/gemini-cli/settings.json`, extensions (lowest). Events: SessionStart/End, BeforeAgent, AfterAgent, BeforeModel, AfterModel, BeforeToolSelection, BeforeTool, AfterTool, PreCompress, Notification. Exit 0 = JSON parsed, 2 = block (stderr reason), other = warning. Project hooks are fingerprinted; a changed hook is untrusted and warned. Stdin payload fields (incl. `transcript_path`): not documented on that page (**unverified**). Shape: M.
- Headless `gemini -p`: not re-fetched here; see support status in section 1 (subscription logins ended 2026-06-18, API key still works).

## 6. Cline (extension + CLI 2.0)

- Popularity: 71,821 stars; npm `cline` 82k/wk; SO 2025 Dev IDEs "Cline and/or Roo" 2.2%.
- CLI ([docs](https://docs.cline.bot/cline-cli/overview)): no `-p` print flag (`-p` = plan mode). Headless turns on automatically with `--json`, piped stdin or redirected output, e.g. `git diff | cline "review these changes"` or `cline --json "..."`. Plan mode is `-p, --plan` (no documented read-only mode); `--auto-approve` defaults to true; no tool allow/deny list in the CLI (docs suggest a `PreToolUse` hook to cancel `run_commands`). Auth: `cline auth` (Cline provider, ClinePass, or own key); subscription specifics not on the page (**unverified**).
- Hooks: the `customization/hooks` page only points to SDK plugins ([sdk/plugins](https://docs.cline.bot/sdk/plugins.md), not read). Earlier research found file executables in `~/Documents/Cline/Hooks/` (path disputed) with PreToolUse/PostToolUse. Payload, exit codes, install channel: **unverified**. Shape: M (file hooks) / P (SDK plugins).

## 7. Windsurf / Devin Desktop and Devin CLI

- Popularity: SO 2025 Dev IDEs Windsurf 4.9%; Devin 2.7% of agent users. Vendor user counts: **unverified**.
- Devin Desktop ([hooks](https://docs.devin.ai/desktop/cascade/hooks)): `.devin/hooks.v1.json` at project or user level (user path not given there); example is `PreToolUse` with matcher `exec`, `type: "command"`; hooks do not run in Restricted Mode. The page defers everything else to the Devin CLI hooks doc.
- Devin CLI ([hooks overview](https://docs.devin.ai/cli/extensibility/hooks/overview)) closes the gap for the payload: stdin fields `hook_event_name`, `tool_name`, `tool_input`, `session_id`, `prompt_id` (rotates per prompt; absent before the first prompt). No `transcript_path` is listed. Events: PreToolUse, PostToolUse, PermissionRequest, UserPromptSubmit, Stop, PostCompaction, SessionStart, SessionEnd (not in cloud). Exit 0 ok, 2 blocks, other = logged, non-blocking; JSON stdout with `decision` approve/block, `reason`, `hookSpecificOutput.additionalContext`, `updatedInput`. Locations: project `.devin/hooks.v1.json`, `.devin/config(.local).json` `"hooks"` key, **`.claude/settings(.local).json` `"hooks"` key (Claude format)**; user `~/.config/devin/config.json`, and `~/.claude.json`, `~/.claude/settings.json`, `~/.claude/settings.local.json`. Claude hooks load automatically while `read_config_from.claude` is on (default). Shape: C (near) for the CLI. Whether Windsurf/Devin Desktop shares this payload: **unverified** (the Desktop page does not say). Headless and plugin install for Devin: not covered in the pages read (**unverified**).

## 8. Kiro (IDE, CLI)

- Popularity: SO 2025 Dev IDEs: not listed; kirodotdev/Kiro 4,487 stars (not the source repo). Vendor count: **unverified**.
- Hooks ([docs](https://kiro.dev/docs/hooks/)): project `.kiro/hooks/<id>.json`; CLI V3 also `~/.kiro/hooks/` (global and workspace both run). File: `version: "v1"`, `hooks[]` with `name`, `trigger` (PascalCase), optional `matcher`, `action` type `command` or `agent`, `timeout` default 60 s, `enabled`, `confirm`. Triggers named: `PostFileSave`, `PreToolUse`, `PostToolUse`, `Stop` (full list on a separate page, not read). Payload JSON on stdin (fields not given); exit codes not on the page. Works in IDE, CLI and Web. Shape: M (Claude casing for the V3 triggers, now confirmed in primary: PascalCase triggers PreToolUse/PostToolUse/Stop).
- Headless ([docs](https://kiro.dev/docs/cli/headless/)): `kiro-cli chat --no-interactive "<prompt>"` (or stdin), add `--v3`; REQUIRES `KIRO_API_KEY`, available only to Kiro Pro, Pro+, Pro Max and Power subscribers, so it is not the plain login. Tool flags `--trust-all-tools` / `--trust-tools=<ids>`; no documented read-only flag (omitting trust flags means unapproved tools are not run, inferred).

## 9. Amp

- Popularity: closed source; npm `@sourcegraph/amp` 20k/wk (name may have moved: unverified). No vendor user figure found.
- Plugins ([plugin API](https://ampcode.com/manual/plugin-api)): TS files in `.amp/plugins/` (project) or `~/.config/amp/plugins/` (system), run with Bun; `plugins: reload` in the command palette; events session.start, agent.start, agent.end, tool.call, tool.result (list may be incomplete, the page was truncated at 100k of 130k characters). No trust prompt documented. Shape: P.
- Headless: `ampcode.com/manual/execute-mode` returned 404 and the main manual does not cover `amp -x`; auth for `-x` (login vs `AMP_API_KEY`) **unverified**. The manual mentions "Sign in with ChatGPT" for OpenAI models inside Amp.

## 10. Qwen Code

- Popularity: 30,863 stars; npm 147k/wk.
- Headless ([docs](https://qwenlm.github.io/qwen-code-docs/en/users/features/headless/)): `qwen -p "<prompt>"` or stdin; `-o text|json|stream-json`; `--approval-mode plan|default|auto-edit|auto|yolo`; `--exclude-tools shell,write,edit` disables tools; `--max-tool-calls` etc. Auth not on that page (free OAuth tier status **unverified**).
- Hooks ([docs](https://qwenlm.github.io/qwen-code-docs/en/users/features/hooks/)): user, workspace `.qwen/settings.json`, system settings; ~30 events incl. PreToolUse, PostToolUse, UserPromptSubmit, SessionStart/End, Stop, SubagentStart/Stop, PreCompact, PermissionRequest; common payload `session_id`, `transcript_path`, `cwd`, `hook_event_name`, `timestamp`, `permission_mode`; exit 0 JSON, 2 blocks, other non-blocking. Trust: project hooks run only after you approve that exact config or the folder is trusted; headless runs print a stderr line when hooks are withheld (important for a third-party installer: project-level install needs a trust step; user-level `settings.json` has no stated prompt). Claude matcher names (`Bash`, `Write|Edit`) work; `CLAUDE_PROJECT_DIR` and `GEMINI_PROJECT_DIR` set. Extension install of hooks: not in the visible text (**unverified**). Shape: C.

## 11. Factory Droid

- Popularity: closed source; vendor user counts not searched (**unverified**); `@factory/cli` 29k/wk (unverified mapping).
- Headless ([docs](https://docs.factory.com/cli/droid-exec/overview)): `droid exec "<prompt>"` (or `-f file`, stdin); READ-ONLY by default (no edits); `--auto low|medium|high` raise autonomy; `-o text|json|stream-json|stream-jsonrpc`. Auth: `FACTORY_API_KEY` (generated in Factory Settings); another login method for `exec` is not described, so it is not confirmed to run on the interactive login.
- Hooks ([guide](https://docs.factory.com/cli/configuration/hooks-guide)): levels Org, Runtime (`--settings`), Folder, Project (`.factory/` at git root), User (`~/.factory/`); files `settings.local.json`, `hooks.json`, `settings.json`; `/hooks` shows them. Events PreToolUse, PostToolUse, UserPromptSubmit, Notification, Stop, SubagentStop, PreCompact, SessionStart, SessionEnd. Payload `session_id`, `transcript_path` (`session.jsonl` under `.factory/projects/`), `cwd`, `permission_mode`, `hook_event_name`. Exit 2 blocks, other non-blocking unless `onFailure: "block"`, default timeout 60 s. Plugins: `hooks/hooks.json` in the plugin root, `DROID_PLUGIN_ROOT` or `CLAUDE_PLUGIN_ROOT` set; install steps not on that page. Trust: TUI asks folder trust and runs no hooks until trusted; `droid exec` does NOT ask, so hooks in an untrusted repo run. Shape: C.

## 12. Goose

- Popularity: 56,866 stars (block/goose, `gh api`).
- Hooks ([post](https://goose-docs.ai/blog/2026/05/14/goose-hooks)): plugin dirs `~/.agents/plugins/<name>/` (user) or `<project>/.agents/plugins/<name>/` with `hooks/hooks.json`, discovered at startup; no registration or trust step described; events SessionStart/End, Stop, UserPromptSubmit, PreToolUse, PostToolUse, PostToolUseFailure, BeforeReadFile, AfterFileEdit, BeforeShellExecution, AfterShellExecution; payload example `event`, `session_id`, `tool_name`, `tool_input`, `working_dir` (no transcript path shown); `PLUGIN_ROOT` env; exit codes not documented (failures are logged). Shape: M. Headless: not on that page (see below).

## 13. Supplementary (Crush, Amp, Droid, Cline, vendor numbers)

- Crush: hooks ([SKILL.md](https://raw.githubusercontent.com/charmbracelet/crush/main/internal/skills/builtin/crush-config/SKILL.md)): `hook add` in a `crushrc` or `hooks` key in `crush.json` (project files win over global); 13 events (PreToolUse, PostToolUse, PermissionRequest, SessionStart/End, TurnStart/End, ...); stdin `event`, `hook_event_name`, `session_id`, `cwd`, `tool_name`, `tool_input` (no `transcript_path`); exit 2 blocks, 49 halts the turn; accepts Claude Code output formats (`hookSpecificOutput`, `decision: "block"`); no trust prompt (config is trusted code). Headless: `crush run "<prompt>"` / stdin, `--quiet` (third-party and mirror pages; README says nothing, flag `--yolo` reportedly not accepted by `run` in 0.88.0: **unverified**); `permissions deny bash ...` hides tools ([README](https://raw.githubusercontent.com/charmbracelet/crush/main/README.md)); subscription login only via Hyper (Charm's own service). Session storage path: not documented (**unverified**). Install channel: none (config-file only). Shape: C.
- Amp headless ([search result of the official execute-mode page](https://ampcode.com/docs/cli/execute-mode)): `amp -x "<prompt>"` (also auto when stdout is redirected), `--stream-json`; scripts need `AMP_API_KEY` (token `sgamp_...`); the token that `amp login` stores is rejected for this because it expires within an hour; execute mode uses paid credits only ([news](https://ampcode.com/news/amp-x)); no tool restriction flag documented (`--dangerously-allow-all` exists).
- Droid install ([plugins](https://docs.factory.com/cli/configuration/plugins)): `droid plugin marketplace add <owner/repo|git url|path>`, `droid plugin install <plugin@marketplace> --scope user|project`; "can also install compatible Claude Code plugins", reads `.claude-plugin/marketplace.json` as a fallback and translates the layout into `.factory-plugin/` (so a Claude Code plugin can ship to Droid as is, with limits).
- Cline SDK plugins ([docs](https://docs.cline.bot/sdk/plugins.md)): in-process hooks `beforeRun`, `afterRun`, `beforeModel`, `afterModel`, `beforeTool`, `afterTool`, `onEvent`; `cline plugin install` from file URL, npm, git or local path; `pluginPaths` in config. The context carries a `snapshot` (conversation id, messages, usage). Directory layout: not on the page. Shape: P.
- Vendor numbers (secondary, all press or aggregator, **unverified** against a primary): Cursor $2B annualized revenue (Bloomberg 2026-03-02), >7M MAU claimed in a March 2026 report; one aggregator says Cursor was acquired by SpaceX (Aug 2026) which no primary source confirmed here. Microsoft FY26 Q2 (Jan 2026): 4.7M paid GitHub Copilot subscribers (+75% YoY); 20M total users in the earlier annual report (TechCrunch 2026-01-29 and Neowin summaries).

## 14. Gaps from section 10 of cross-harness-capture.md: status

| Gap | Result |
|---|---|
| Windsurf/Devin Desktop hook payload and events | Devin CLI doc gives it: events, stdin fields (no `transcript_path`), exit 2 blocks, and it reads `.claude/settings*.json` hooks. Windsurf-specific payload still unverified. |
| Cursor `transcript_path` | Confirmed official, common field, null if transcripts disabled ([hooks](https://cursor.com/docs/hooks)). |
| Cursor `stop` in the CLI | Still unverified; docs name `workspaceOpen` for desktop and CLI only. |
| Copilot hooks in VS Code | Confirmed (preview): `.github/hooks/*.json`, `~/.copilot/hooks/`, reads `.claude/settings.json`; `transcript_path` only on stop per the CLI reference, not stated on the VS Code page; Local harness removal Feb 2027. |
| OpenCode transcript/storage | `opencode debug paths [db]` is the official way to print it; SQLite file name is still third-party. |
| Gemini CLI end date | 2026-06-18 for consumer subscribers; stays for API-key/enterprise users; repo Apache 2.0 continues. |
| Antigravity CLI hooks | Events, payload (with `transcriptPath`), config paths now primary-sourced; exit codes/trust prompt not. |

## 15. Summary table

Subscription column: Y = documented to run on the login, N = documented to need an API key or token, ? = not found. Hook shape C/M/P as legend above.

| Harness | Popularity (source) | Headless command (subscription ok?) | Install channel | Hook shape |
|---|---|---|---|---|
| Cursor | SO 2025 Dev IDEs 17.9%; $2B ARR press (Bloomberg, 2026-03); >7M MAU claim (secondary) | `agent -p` (? docs show `CURSOR_API_KEY`; no read-only flag) | Plugin marketplace (reviewed submissions) or `~/.cursor/hooks.json` / `.cursor/hooks.json`; project hooks need trusted workspace | M, loads Claude hooks, has `transcript_path` |
| GitHub Copilot | SO 2025 agent tools 67.9% of agent users; 4.7M paid subs (MSFT Q2 FY26, secondary); CLI npm 1.72M/wk, 11.5k stars | `copilot -p -s --available-tools=read` (? token env documented, `login` exists) | `copilot plugin marketplace add` / `install`; or `.github/hooks/*.json`, `~/.copilot/hooks/` | C near; reads `.claude/settings.json`; `transcriptPath` on agentStop |
| OpenCode | 228,537 stars; npm 3.60M/wk | `opencode run` (Y for ChatGPT and Copilot logins; Claude login unverified); `plan` agent or deny perms | Files in `~/.config/opencode/plugins/`, `.opencode/plugins/`, npm via `opencode plugin add` | P |
| Gemini CLI | 108,993 stars; npm 557k/wk; consumer service ended 2026-06-18 | `gemini -p` (N for consumer since June; API key only) | `~/.gemini/settings.json` or `.gemini/settings.json` (fingerprint trust), extensions | M |
| Cline | 71,821 stars; npm 82k/wk; SO Dev IDEs "Cline and/or Roo" 2.2% | `cline "<prompt>"` with `--json`/piped stdin (? `cline auth`) | `cline plugin install` (npm/git/path) for SDK plugins; file hooks (path disputed) | P (SDK) / M (file) |
| Windsurf / Devin Desktop and CLI | SO Dev IDEs Windsurf 4.9%; Devin 2.7% of agent users | not found (?) | `.devin/hooks.v1.json`, `~/.config/devin/config.json`; reads `~/.claude/settings.json` and `.claude/settings.json` | C near (CLI), M (Desktop); no transcript path |
| Kiro | not in SO lists; 4,487 stars on tracker repo only | `kiro-cli chat --no-interactive` (N: needs `KIRO_API_KEY`, paid tiers only) | `.kiro/hooks/*.json`, `~/.kiro/hooks/` (CLI V3) | M (Claude-style PascalCase triggers) |
| Amp | npm 20k/wk (closed source) | `amp -x` (N: `AMP_API_KEY`; `amp login` token rejected) | `.amp/plugins/`, `~/.config/amp/plugins/`, `plugins: reload` | P |
| Goose | 56,866 stars | `goose run -t` / `-i` (? provider keys; no tool restriction found) | `~/.agents/plugins/<name>/hooks/hooks.json` or project `.agents/plugins/` | M (Open Plugins) |
| Qwen Code | 30,863 stars; npm 147k/wk | `qwen -p` + `--exclude-tools` / `--approval-mode plan` (? auth not read) | `.qwen/settings.json` (project needs approval), user settings | C, has `transcript_path` |
| Droid | closed source; npm `@factory/cli` 29k/wk (mapping unverified) | `droid exec` read-only by default (N: `FACTORY_API_KEY`) | `droid plugin marketplace add` + `install --scope user|project`; installs Claude Code plugins | C, has `transcript_path`; `exec` skips folder trust |
| Crush | 29,163 stars; npm 14k/wk | `crush run` (? Hyper only) | config file only (`crush.json`/`crushrc`) | C for events seen; no `transcript_path` |

Fixed three for reference: Claude Code 156,663 stars, npm 22.2M/wk, SO Dev IDEs 9.7%, 40.8% of agent users; Codex 136,133 stars, npm 27.6M/wk; Antigravity CLI hooks M with `transcriptPath` (section 1).

## 16. Unverified items

- Per-tool JetBrains 2025/2026 numbers (April 2026 post blocked with 403); vendor user counts for Cursor, Copilot, Windsurf, Kiro, Amp, Droid beyond secondary press.
- Cursor: `-p` on subscription login; `stop` event in `cursor-agent`; plugin install command; content of the "Third Party Hooks" page.
- Copilot: `-p` on interactive login; Claude Code plugins loading as is; VS Code `transcript_path`.
- OpenCode: official statement of the SQLite filename; `run` flags; Claude subscription login; event list; Claude hook loading.
- Antigravity CLI: headless command and flags, exit codes, trust prompt.
- Cline: hook payload and install paths for file hooks; subscription auth.
- Windsurf/Devin Desktop: payload (assumed same as Devin CLI), headless, plugin install.
- Kiro: full trigger list, exit codes, payload fields, marketplace/"powers" channel.
- Amp: tool restriction flags, thread storage path, full event list (page truncated), trust prompt.
- Goose headless auth and tool restriction; exit codes of hooks.
- Qwen auth methods and extension hook install; Droid npm package name; Crush `run` flags and storage path.
- npm `@sourcegraph/amp` being the current Amp package.
