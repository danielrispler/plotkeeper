# Cross-harness capture: one integration for many agents, and where the map is shown in a terminal workspace

> **Answer:** No, not one adapter per harness, but not one adapter either. Build four layers: a git-tree recorder for every harness (files and turn timing), one hook handler that speaks Claude Code's hook shape (about 8 of 39 harnesses speak it as is, about 15 more need a small name mapping), a transcript reader for the rest, and an optional OTLP receiver and LLM proxy for gaps. Show the map and cards in a pane of the terminal workspace, as a small TUI the workspace opens, and keep the browser page as the fallback. The Studio already holds the first two layers' shape (`hook/`, `core/`, `understanding/`).
> **Evidence:** Strong for Claude Code, Codex, Gemini CLI, Copilot, Droid, Qwen Code, OpenTelemetry, the Claude gateway page, git-ai, ccusage and agentsview (official docs or source trees read). Moderate for Cursor, Kiro, Goose, Cline, OpenCode, Amp, Crush, OpenHands and herdr (docs read, but payloads and paths often not). Weak for Zed, Junie, Jules, Trae, Warp, Codex cloud, Devin cloud and most on-disk transcript paths. Coverage percentages are my arithmetic on the table in section 3, not a survey.

Research for Daniel's three questions, 22 November 2026: do we need an adapter for each of 30+ agent harnesses; how do other tools capture from many agents; and where does the map live for someone working inside herdr. It builds on `docs/studio/research/delivery-surfaces.md` (the previous round: one recorder, one adapter per agent, a shared local page) and `docs/studio/research/herdr.md`, and corrects it in section 8. Terms follow `docs/studio/CONTEXT.md`. What the understanding layer needs per turn: the prompt, the agent's replies and intentions, tool calls, turn boundaries, and the files changed (git trees at turn start and end).

All sources were fetched on 22 November 2026 by me or by research helpers that read the pages; a claim I could not confirm from a primary source is marked **unverified**. "Primary" means vendor docs, spec text or source repositories.

## 1. Short answer to the three concerns

**1. Do we need an adapter for each harness?** Not for each, but every survey tool I read does write one per harness, and the long tail is real. The cheap path:

- **Layer 0, every harness: git trees.** A recorder that snapshots the working tree at turn start and end needs no agent support. It gives changed files and timing. It does not give the prompt (section 4).
- **Layer 1, about 23 of 39 harnesses: one hook handler.** 8 harnesses use Claude Code's hook shape as is or nearly (Claude Code, Claude Code on the web, Codex, Copilot CLI, Droid, Qwen Code, OpenHands, Crush for one event). 15 more use the same ideas with other event names or config files (Gemini CLI, Antigravity CLI, Cursor, Kiro, Goose, Cline, Kimi and others). One binary plus a per-harness mapping of about 10 to 30 lines covers them. This repo's `studio-hook` already is that binary for two of them.
- **Layer 2, the rest: read the transcript.** The mature tools (agentsview, 70+ agents; tokscale, about 90; ccusage, 20) all do this, one parser per agent, 250 to 450 lines for a simple one and up to several hundred KB for a messy one (section 5).
- **Optional extras:** an OTLP receiver (8 harnesses emit OpenTelemetry; content is opt-in) and a local LLM proxy (works where a base URL can be set; sees prompts and replies but not turn boundaries or files).
- Nothing in the standards (ACP, MCP, AGENTS.md, Agent Skills, Agent Trace) gives a passive tap. The nearest to a hook standard is the Agent Plugins 1.1.0 draft with five Claude-style events, and no harness is confirmed to implement it (section 4.1).

**2. How do others do it?** Four methods dominate, in this order: parse the agent's own session files (ccusage, tokscale, agentsview, cass, claude-code-viewer); install per-agent hooks (git-ai, Entire, Emdash, Langfuse's Claude integration); wrap or launch the agent (Claude Squad, Happy, SpecStory `run`); proxy or OTel (LiteLLM, Claude Code's own telemetry). Attribution and review tools show up in git (notes, trailers, checkpoints); trackers and viewers show up in a CLI, TUI or local web page (section 6).

**3. Where does the map show in herdr?** In a herdr pane. Herdr's TUI has plugins whose panes are terminal commands (placements `overlay`, `popup`, `split`, `tab`, `zoomed`), opened by `herdr plugin pane open` or the socket's `plugin.pane.open`; a popup or split can run a TUI that takes keys, so the quiz works there. The map is a Kitty-graphics image where the host terminal supports it, else a mermaid-ascii or half-block drawing. If the owner runs the herdr app rather than the TUI, "mods" can draw markdown, images and SVG natively. Recommendation: a TUI companion opened by a herdr plugin pane, a notification to say a card is ready, and the browser page only as a link for the full map (section 7). Which herdr the owner runs is an open question.

## 2. What counted and how

I built the inventory from vendor docs, the ACP agent registry and the supported-agent lists of ccusage, tokscale, agentsview and git-ai. **39 harnesses are rows in section 3.1**, 38 alive (Roo Code's extension was shut down on 15 May 2026). About 12 more are named only, with no row: Blackbox AI, Docker cagent, fast-agent, Poolside, VT Code, Stakpak, Command Code, Code Assistant, AutoDev (all on the ACP agents list), and Grok Build, ZCode, DeepSeek Harness (named by ccusage). tokscale lists about 90 tools including editors and chat apps; I did not read the tail of its README. "30+" is therefore right for coding agents with their own loop, and the count grows to about 90 if every wrapper counts.

Legend for the table: **C** hook shape is Claude Code's or near it; **M** command hooks with the same ideas but other names, file or payload (needs a mapping); **P** in-process plugin (JS or TS), not a command; **none** no hook system found. "?" = not found in a primary source (**unverified**).

## 3. The harness inventory

### 3.1 Table

| Harness (kind) | Hooks | OTel | Transcript on disk | MCP | ACP agent | Custom base URL |
|---|---|---|---|---|---|---|
| Claude Code (CLI, IDE, Desktop) | **C** (reference). 30+ events, `transcript_path` in the payload ([hooks](https://code.claude.com/docs/en/hooks)) | Yes: metrics, events, beta traces; content opt-in ([monitoring](https://code.claude.com/docs/en/monitoring-usage)) | `transcript_path` is official; `~/.claude/projects/<cwd>/<id>.jsonl` is from a third-party page | Yes | Yes ([registry](https://agentclientprotocol.com/overview/agents)) | Yes; base URL alone keeps the subscription login ([gateway](https://code.claude.com/docs/en/llm-gateway)) |
| Claude Code on the web | **C**: loads the repo's `.claude/settings.json` ([cloud](https://code.claude.com/docs/en/cloud-environments.md)); which events fire is **unverified** | ? | ? | Yes | n/a | Platform sets it per session; custom value **unverified** |
| Codex CLI and IDE | **C** near: same events, `CLAUDE_PLUGIN_ROOT` aliased "for compatibility", but `PreModelRequest` and `Interrupt` differ and only command and `mcp_tool` handlers run ([hooks](https://learn.chatgpt.com/docs/hooks)) | Yes: `[otel]`; prompt text opt-in via `log_user_prompt` ([config](https://learn.chatgpt.com/docs/config-file/config-advanced)) | `history.jsonl` is official; `~/.codex/sessions/.../rollout-*.jsonl` is third-party; hooks get `transcript_path` | Yes | Yes (registry) | `openai_base_url`; with ChatGPT login only via a gateway that proxies the ChatGPT backend (third-party, **unverified**) |
| Codex cloud | ? | ? | ? | ? | n/a | ? |
| Gemini CLI (folded into Antigravity CLI; support end date disputed, **unverified**) | **M**: `BeforeAgent`, `BeforeTool` etc., command only, exit 2 blocks ([hooks](https://geminicli.com/docs/hooks/)) | Yes; `logPrompts` defaults to **true** ([telemetry](https://geminicli.com/docs/cli/telemetry/)) | `~/.gemini/tmp/<hash>/chats/`; format disputed | Yes | Yes (`--acp`) | `GOOGLE_GEMINI_BASE_URL` in gateway guides only; with Google login **unverified** |
| Antigravity CLI | **M**: `.agents/hooks.json` or `~/.gemini/config/hooks.json` ([docs](https://antigravity.google/docs/hooks.md)); this repo wires six events in `plugin-agy/hooks.json` | ? | `transcript_full.jsonl` under `brain/<id>/` (this repo, `core/src/agy.rs`) | ? | ? | ? |
| GitHub Copilot CLI and VS Code agent | **C** near: reads `.claude/settings.json`; accepts PascalCase names with VS Code snake_case payloads and camelCase names; exit 2 only blocks on `preToolUse` and `permissionRequest` ([reference](https://docs.github.com/en/copilot/reference/hooks-configuration)) | Yes, `gen_ai.*` spans; content off by default ([OTel](https://docs.github.com/en/copilot/reference/enterprise-administrators/opentelemetry-reference)) | `agentStop` payload has `transcriptPath` | Yes | Yes (registry) | BYOK `COPILOT_PROVIDER_BASE_URL` ([docs](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/use-byok-models)); with Copilot login **unverified** |
| Copilot cloud agent | **M**: `.github/hooks/*.json`, a subset of events, http hooks to allow-listed hosts only ([docs](https://docs.github.com/en/copilot/how-tos/use-copilot-agents/cloud-agent/use-hooks)) | ? | ? | Yes | n/a | n/a (GitHub runs it) |
| Cursor (IDE and CLI) | **M**: `.cursor/hooks.json`, flat, camelCase (`beforeSubmitPrompt`, `stop`, ...), `transcript_path` in the payload, exit 2 blocks; loads third-party Claude hooks ([hooks](https://cursor.com/docs/hooks)). `stop` in the CLI reported missing (forum, **unverified**) | None found for the editor; the Agent SDK has a guide (not read) | Via `transcript_path` | Yes | Yes (registry) | ? |
| Cursor cloud agents | **M** subset: no session, MCP or Tab hooks (delivery-surfaces, [hooks](https://cursor.com/docs/agent/hooks)) | ? | ? | Yes | n/a | n/a |
| Factory Droid | **C**: Claude's names, JSON stdin, exit 2, `CLAUDE_ENV_FILE` ([hooks guide](https://docs.factory.com/cli/configuration/hooks-guide)) | ? | ? | Yes | Yes (registry) | ? |
| Qwen Code | **C**: Claude matchers and tool names unchanged, `transcript_path`, `CLAUDE_PROJECT_DIR` ([hooks](https://qwenlm.github.io/qwen-code-docs/en/users/features/hooks/)) | Yes: file or collector ([docs](https://qwenlm.github.io/qwen-code-docs/en/development/telemetry)) | ? | Yes | Yes (registry) | ? |
| Kiro (IDE and CLI; Amazon Q Developer CLI's successor) | **M**: `.kiro/hooks/*.json`; v3 reportedly uses Claude casing (third-party, **unverified**) ([hooks](https://kiro.dev/docs/hooks/), [Q end of support](https://aws.amazon.com/blogs/devops/amazon-q-developer-end-of-support-announcement/)) | Admin usage metrics only, not agent level ([changelog](https://kiro.dev/changelog/general/open-telemetry-exports/)) | ? | Yes | Yes (registry) | ? |
| Windsurf / Devin Desktop | **M**: `.devin/hooks.v1.json`, matcher groups ([docs](https://docs.devin.ai/desktop/cascade/hooks)); payload and exit codes **unverified** | ? | ? | ? | ? | ? |
| Devin CLI | **M**: `.devin/hooks.v1.json` ([docs](https://docs.devin.ai/cli/extensibility/hooks/overview)); reads Claude Code and Cursor rules, skills and MCP config | ? | ? | Yes | Yes (`devin acp`) | ? |
| Devin cloud | ? | ? | ? | ? | n/a | n/a |
| Goose (Block, AAIF project) | **M**: Open Plugins hooks, `hooks.json`, 11 events with Claude-like names; no Claude-compat claim ([post](https://goose-docs.ai/blog/2026/05/14/goose-hooks)) | Yes (third-party page; content **unverified**) | ? | Yes | Yes (registry) | ? |
| Mistral Vibe | **M**: `hooks.toml`; docs say Claude Code and Kimi plugin hooks are *not* supported ([docs](https://docs.mistral.ai/vibe/code/cli/hooks)) | ? | ? | ? | Yes (registry) | ? |
| Kimi Code CLI | **M**: TOML `[[hooks]]` with Claude-style names ([docs](https://www.kimi.com/code/docs/en/kimi-code-cli/customization/hooks.html)) | ? | ? | ? | Yes (registry) | ? |
| Cline (extension and CLI 2.0) | **M**: file-based executables in `~/Documents/Cline/Hooks/` (path disputed) with `PreToolUse`, `PostToolUse` ([docs](https://docs.cline.bot/customization/hooks.md)); SDK plugins are in-process ([docs](https://docs.cline.bot/sdk/plugins.md)) | Yes: OTLP metrics and logs, opt-in ([docs](https://docs.cline.bot/enterprise-solutions/monitoring/opentelemetry)) | ? | ? | Yes (`--acp`, [blog](https://cline.bot/blog/announcing-cline-cli-2-0)) | ? |
| OpenCode (now `anomalyco/opencode`) | **P**: events `session.*`, `message.*`, `tool.execute.before/after`, `file.edited`, `permission.*` ([v1 docs](https://opencode.ai/v1/docs/plugins.md)); not Claude-compatible | `experimental.openTelemetry` flag, third-party docs, reported unreliable; community plugin sends prompts | SQLite `~/.local/share/opencode/opencode.db` (issue and wiki, not official); older builds used JSON | Yes | Yes (`opencode acp`, [docs](https://opencode.ai/docs/cli/acp/)) | Yes, per provider `baseURL` ([docs](https://opencode.ai/docs/providers/)); with a subscription login **unverified** |
| Kilo Code (CLI is an OpenCode fork) | **P** inherited (secondary source, **unverified**) | ? | ? | Yes | Yes (`kilo acp`) | ? |
| Amp | **P**: `session.start`, `agent.start/end`, `tool.call`, `tool.result` ([plugin API](https://ampcode.com/manual/plugin-api)) | None native; a vendor plugin exists | `~/.local/share/amp/threads/T-*.json` (third-party code only) | Yes | Third-party adapters only | Not documented |
| Crush (Charm) | **C** for one event: `PreToolUse`, "Claude Code hooks should work unchanged" ([repo skill](https://raw.githubusercontent.com/charmbracelet/crush/main/internal/skills/builtin/crush-config/SKILL.md)) | ? | SQLite, path **unverified** | Yes | ? | Yes |
| Zed built-in agent | none found | ? | ? | Yes | Zed is an ACP *client*; its own agent as server **unverified** ([docs](https://zed.dev/docs/ai/external-agents)) | ? |
| JetBrains Junie | ? | ? | ? | Yes | Yes (registry) | BYOK ([blog](https://blog.jetbrains.com/junie/2026/03/junie-cli-the-llm-agnostic-coding-agent-is-now-in-beta/)) |
| Augment (Auggie) | **M**: `PreToolUse`, `PostToolUse`, `Stop`, `SessionEnd` in `.augment/settings.json` ([docs](https://docs.augmentcode.com/cli/hooks)) | None native; Langfuse hook integration | ? | ? | Yes (registry) | ? |
| Trae | none found | ? | ? | Yes (IDE) | ? | Yes (IDE; third-party) |
| Jules (Google) | none found | ? | n/a (cloud) | ? | ? | n/a |
| Warp agents | none found | ? | ? | Yes (Oz CLI) | ? | ? |
| Aider (maintenance mode: last release Feb 2026, per PyPI) | none; `--notifications-command` ([options](https://aider.chat/docs/config/options.html)) | none | `.aider.chat.history.md`; commits tagged "(aider)" ([git docs](https://aider.chat/docs/git.html)) | No | No | Yes (via litellm; **unverified** here) |
| Continue `cn` CLI | **M**: Claude-compatible hooks added by a PR (Mar 2026); docs may not have shipped ([issue mirror](https://tgfed.nhyvvfeijnh.workers.dev/continuedev/continue/issues/11115), **unverified**) | ? | ? | Yes | Hidden `cn acp` | ? |
| Pi (`pi-coding-agent`) | **P**: extensions in `~/.pi/agent/extensions/` ([README](https://unpkg.com/@earendil-works/pi-coding-agent@0.83.0/README.md)) | ? | `~/.pi/agent/sessions/` (README) | ? | Yes (registry) | `~/.pi/agent/models.json` |
| OpenHands | **C**: `.openhands/hooks.json`, designed to match Claude Code, `terminal` for `Bash` ([docs](https://docs.openhands.dev/openhands/usage/customization/hooks)) | ? | ? | ? | Yes (registry) | ? |
| OpenClaw | **P**: typed plugin hooks ([ref](https://docs.openclaw.ai/plugins/hooks/reference)) | Yes: `diagnostics-otel` plugin ([docs](https://docs.openclaw.ai/gateway/opentelemetry)) | ? | ? | Yes (registry) | ? |
| Hermes Agent (Nous) | **P**: `pre_llm_call`, `on_session_start`, tool hooks (partly third-party) ([docs](https://hermes-agent.nousresearch.com/docs/)) | ? | ? | Yes | Yes | Yes |
| Qoder CLI | **M**: JSON hooks; IDE docs map tool names to Claude Code's ([docs](https://docs.qoder.com/cli/hooks)) | ? | ? | Yes | Yes (`--acp`) | ? |
| Codebuff | none found | ? | ? | ? | ? | `CODEBUFF_BASE_URL` PR seen on a mirror only, **unverified** |
| Roo Code | Dead: extension shut down 15 May 2026, repo archived ([blog](https://roocode.com/blog/sunsetting-roo-code-extension-cloud-and-router)) | | | | | |

### 3.2 What the table counts to

- **Hooks, by class (39 rows):** C 8 (Claude Code, Claude Code web, Codex, Copilot CLI, Droid, Qwen Code, OpenHands, Crush); M 15 (Gemini, Antigravity, Copilot cloud, Cursor, Cursor cloud, Kiro, Windsurf/Devin Desktop, Devin CLI, Goose, Vibe, Kimi, Cline, Augment, Continue, Qoder); P 6 (OpenCode, Kilo, Amp, Pi, OpenClaw, Hermes); none or unknown 9 (Codex cloud, Devin cloud, Zed, Junie, Trae, Jules, Warp, Aider, Codebuff); dead 1.
- **OpenTelemetry out of the box:** 8 (Claude Code, Codex, Gemini CLI, Copilot, Qwen Code, Cline, OpenClaw, Goose by a third-party page). Content is opt-in everywhere except Gemini CLI.
- **ACP agent side:** 22 are on the registry or documented (Claude Code, Codex, Gemini, Copilot, Cursor, Droid, Qwen, Kiro, Goose, Vibe, Kimi, Cline, OpenCode, Kilo, Devin, Junie, Augment, Qoder, OpenHands, Pi, OpenClaw, Hermes).
- **Custom base URL confirmed in a primary source:** about 9 (Claude Code, Codex API-key mode, OpenCode, Crush, Copilot CLI BYOK, Junie BYOK, Hermes, Pi, Trae IDE by a third party). The rest are **unverified** rather than "no"; most open-source harnesses will allow one.
- **Official transcript path in a hook payload:** Claude Code, Codex, Copilot (`agentStop`), Cursor, Qwen Code. All other paths are third-party or from this repo's own fixtures.

## 4. Common denominators: what each gives against what the layer needs

Needs: **P** prompt, **R** responses and intentions, **T** tool calls, **B** turn boundaries, **F** changed files.

| Approach | Harnesses it reaches (of 39) | P | R | T | B | F | Main cost or limit |
|---|---|---|---|---|---|---|---|
| Claude-shape hook handler | 23 (8 as is, 15 with a mapping) | Yes (prompt event) | Via `transcript_path` or a stop payload (**unverified** per harness) | Yes (tool events) | Yes (prompt and stop events) | Yes, by git trees taken at those events | Dialects: flat vs nested config, `Stop` vs `agentStop`, Copilot's exit 2 is only a warning |
| Transcript reader | Wherever the format is known: agentsview reads 70+ agents | Yes | Yes | Yes | Inferred from message order, and late (after the fact) | Edit tool calls only; use git | One parser per format; formats drift (see section 5) |
| OTLP receiver | 8 | Opt-in (`OTEL_LOG_USER_PROMPTS`, `log_user_prompt`; Gemini on by default) | Opt-in (`OTEL_LOG_ASSISTANT_RESPONSES`) | Names yes; args opt-in | Claude: `prompt.id` and an `interaction` span; others per prompt event | No | Each emits its own names (`claude_code.*`, `codex.*`, `gemini_cli.*`, `gen_ai.*`); no common schema; Claude ignores OTel settings in project settings |
| LLM proxy on a base URL | 9 confirmed, probably 20 or more | Yes | Yes (reassemble SSE) | Calls yes; local results and failures no | Guessed from request patterns | No | Cursor, Copilot IDE and Windsurf route through their own backends (**unverified** that no base URL is possible); subscription logins work only for Claude Code (below) |
| Git or filesystem watcher | 39, cloud agents after a fetch | No | No | No | Only by idle debounce, or by a status signal such as OSC 7501 (the repo's status-only tier) | Yes, exact | Loses all meaning: no prompt, no reasons |
| ACP tap (client or conductor) | 22 speak ACP, but only when *we* launch them or sit in the editor's chain | Yes | Yes | Yes | Yes | Via tool calls | Not passive: a developer running `claude` in a terminal never touches it. The proxy-chains design is a draft (below) |
| MCP server | ~All | No | No | No | No | No | Pull only: the agent must call us. Useful for "show me the card", not for capture |
| AGENTS.md / Agent Skills | ~All read them | n/a | n/a | n/a | n/a | n/a | Instructions, not events: Agent Skills defines no hooks or transcripts ([spec](https://agentskills.io/specification)) |
| Attribution records (git-ai notes, Agent Trace) | git-ai: 14 agents plus 4 background agents | No: the git-ai spec forbids messages in notes | No | No | Per checkpoint | Yes, line-level, which agent and model | Gives *who wrote which lines*, nothing about why |

### 4.1 Details that decide the table

**Hooks: convergence is real, not a standard.**
- Claude Code's vocabulary (`PreToolUse`, `PostToolUse`, `UserPromptSubmit`, `Stop`, `SessionStart`, matcher groups, JSON on stdin, exit 2 blocks) is copied or aliased by Droid, Qwen Code, Copilot CLI, Codex (with its own parity tracker), OpenHands, Crush, Kimi, Kiro v3 (third-party) and Cursor (loads third-party hooks) ([Droid](https://docs.factory.com/cli/configuration/hooks-guide), [Qwen](https://qwenlm.github.io/qwen-code-docs/en/users/features/hooks/), [Copilot](https://docs.github.com/en/copilot/reference/hooks-configuration), [Codex](https://learn.chatgpt.com/docs/hooks)).
- There are two dialects: nested matcher groups (Claude Code, Codex, Droid, Qwen) and a flat `version: 1` list with camelCase names (Cursor, Copilot's other mode); Gemini uses `BeforeTool`; Vibe refuses Claude plugins.
- **The one spec effort:** Agent Plugins (formerly Open Plugins; repo `vercel-labs/open-plugin-spec`). Its 1.1.0 *working draft* defines a root `hooks.json` with five events (`SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `Stop`), one subprocess per handler, JSON on stdin ([draft section 7.3](https://raw.githubusercontent.com/vercel-labs/open-plugin-spec/main/spec/1.1.0.md)). Vercel's launch post says v1 standardises only skills and MCP servers and that "commands, hooks, and agents remain with clients" ([post](https://vercel.com/blog/introducing-agent-plugins)), which conflicts with the draft (the draft is newer, or unratified: **unverified**). Goose says it follows the hooks part; Cursor says spec-following plugins load unchanged ([Cursor](https://cursor.com/docs/reference/plugins)). No harness is confirmed to implement the 1.1.0 hooks file. AAIF (Linux Foundation) holds MCP, goose and AGENTS.md and I found no hooks spec there ([announcement](https://www.linuxfoundation.org/press/linux-foundation-announces-the-formation-of-the-agentic-ai-foundation)).
- A hook file in the Agent Plugins shape and one in Claude Code's plugin shape differ only a little (event names match; the handler is `command` plus `args` rather than a shell string), so one handler binary could serve both.

**OpenTelemetry.**
- The GenAI semantic conventions moved to `open-telemetry/semantic-conventions-genai` and are in **Development**, not stable. They define `invoke_agent`, `execute_tool` and inference spans; message content (`gen_ai.input.messages`, `gen_ai.output.messages`) is off by default, switched by `OTEL_INSTRUMENTATION_GENAI_CAPTURE_CONTENT` ([spans](https://raw.githubusercontent.com/open-telemetry/semantic-conventions-genai/main/docs/gen-ai/gen-ai-spans.md), [agent spans](https://raw.githubusercontent.com/open-telemetry/semantic-conventions-genai/main/docs/gen-ai/gen-ai-agent-spans.md)).
- In practice only Copilot follows `gen_ai.*`; Claude Code, Codex and Gemini CLI use their own prefixes ([Claude](https://code.claude.com/docs/en/monitoring-usage), [Codex](https://learn.chatgpt.com/docs/config-file/config-advanced), [Gemini](https://geminicli.com/docs/cli/telemetry/)).
- Claude Code's opt-ins: `OTEL_LOG_USER_PROMPTS`, `OTEL_LOG_ASSISTANT_RESPONSES`, `OTEL_LOG_TOOL_DETAILS`, `OTEL_LOG_TOOL_CONTENT`, `OTEL_LOG_RAW_API_BODIES`; content truncated at 60 KB. Exporter variables in project settings are ignored, so they must come from user or managed settings. Hooks do not receive `OTEL_*` variables.
- Codex only honours `otel` in user-level `config.toml`.

**LLM proxy.**
- Claude Code: setting only `ANTHROPIC_BASE_URL` keeps the subscription login as the credential; a gateway key or `apiKeyHelper` replaces it. The gateway must forward `Authorization` and the OAuth `anthropic-beta` header ([gateway page](https://code.claude.com/docs/en/llm-gateway)). LiteLLM documents this exact setup and logs cost at API list price ([tutorial](https://docs.litellm.ai/docs/tutorials/claude_code_max_subscription)). Claude Code can also pass `TRACEPARENT` to a gateway (`CLAUDE_CODE_PROPAGATE_TRACEPARENT=1`).
- Policy: Anthropic does not allow third parties to "route requests through Free, Pro, Max, or Ultra plan credentials on behalf of their users" or to "intermediate" Claude.ai credentials, and does not restrict a user running the unmodified binary on their own login ([legal](https://code.claude.com/docs/en/legal-and-compliance)). A local proxy on the user's own machine that forwards the header and stores nothing is not addressed either way: **unverified legal reading**, ask before shipping it.
- Codex with a ChatGPT login: custom providers speak the Responses API only (Cloudflare docs); pointing a ChatGPT-login session at an arbitrary local proxy is **unverified** (only gateways that proxy the ChatGPT backend, such as Coder's, are documented).
- What a proxy cannot see (my inference): turn boundaries as the harness defines them, local tool durations and failures that never reach the model, approvals, file changes, subagent separation.

**Attribution specs.**
- **Agent Trace** exists: v0.1.0, January 2026, RFC, from Cursor ([agent-trace.dev](https://agent-trace.dev)). A record is file, line ranges, contributor type, model and a conversation URL. It does not say where records are stored. Its repo URL returned 404 for the helper (**unverified**). Thoughtworks Radar mentions Cline, OpenCode and git-ai as supporters ([radar](https://www.thoughtworks.com/radar/platforms/agent-trace)); a longer vendor list is **unverified**.
- **git-ai** is the working implementation: agents call `git-ai checkpoint`; line-level attribution lives in git notes under `refs/notes/ai`; the v3.0.0 spec says session records "must not contain messages" ([spec](https://raw.githubusercontent.com/git-ai-project/git-ai/main/specs/git_ai_standard_v3.0.0.md), [repo](https://github.com/git-ai-project/git-ai)). For the 18 agents it covers, its notes could give the understanding layer line-level file attribution for free.
- Trailers: the Linux kernel asks for `Assisted-by: AGENT:MODEL`; Aider tags the author name and can add `Co-authored-by`; Claude Code adds `Co-Authored-By` ([kernel](https://docs.kernel.org/next/process/coding-assistants.html), [Aider](https://aider.chat/docs/git.html)). I found no cross-vendor trailer standard.
- Two IETF individual drafts exist for verifiable agent conversation records and audit trails (not adopted): [conversations](https://datatracker.ietf.org/doc/html/draft-birkholz-verifiable-agent-conversations), [audit trail](https://datatracker.ietf.org/doc/html/draft-sharif-agent-audit-trail). I found no universal transcript format.

**ACP.**
- The proxy-chains RFD describes a "conductor" between client and agent where proxies can intercept any ACP message; it is an initial draft, with the Rust SDK behind an `unstable_proxy_chains` flag ([RFD](https://agentclientprotocol.com/rfds/proxy-chains), [crate](https://docs.rs/agent-client-protocol-conductor)). A tap sees only ACP traffic and nothing of agent internals. This agrees with delivery-surfaces: ACP is an extra tap, not a base.

## 5. Recommendation: layers and counts

| Layer | What it is | Harnesses covered (of 39) | Integrations to write | Gives |
|---|---|---|---|---|
| 0, universal | Git-tree recorder at turn start and end, plus a turn signal: a hook where there is one, else an idle debounce or a status signal (OSC 7501 status rows already exist in this repo's `core`) | 39 | 1 | F, B (rough), and a card with files and no reasons |
| 1, hooks | One hook binary (`studio-hook` generalised): input normaliser from Claude's shape; a per-harness table of event names, config location and install step | 23 | 1 binary, about 15 small mappings, 8 with none | P, T, B, F; R when the payload has a transcript path |
| 2, transcript | A reader per on-disk format, tailing files that grow. Needed where hooks give no replies and for the harnesses with no hooks but known files | Aider, Amp, OpenCode, Pi, Crush and any other with a known path (about 6 to 10 known, paths mostly unverified) | 1 per format, about 250 to 450 lines for a simple one | P, R, T |
| 3, optional | OTLP receiver (8 harnesses), local proxy (confirmed on about 9) | 8 and 9, overlapping layer 1 | 1 each, shared across harnesses | R and T where layers 1 and 2 are missing; token and cost |
| 4, extras, per harness | Claude Code mod pane, herdr pane, Amp or OpenCode plugin UI, git-ai notes as an attribution source | 1 each | optional | Better display and line attribution |

Reading: with layers 0 and 1, **23 harnesses get a full card** from about 1 binary plus 15 small mappings; **16 more get a files-only card** from layer 0, and a full one if layer 2 or 3 is added for that harness. Cursor, Copilot, Codex, Claude Code, Gemini CLI, Kiro and Windsurf are the largest by users and are all in the 23. Real tools support a long tail of parsers: agentsview has 65 `*_provider.go` files (about 3.8 MB of Go); ccusage 21 adapter directories averaging 134 KB each; git-ai about 20 preset files (324 KB), typically 10 to 25 KB ([agentsview](https://github.com/wesm/agentsview/tree/main/internal/parser), [ccusage](https://github.com/ryoppippi/ccusage/tree/main/rust/adapters), [git-ai](https://github.com/git-ai-project/git-ai/tree/main/src/commands/checkpoint_agent/presets)). The pattern says: do not build the long tail; build the few popular ones well and make the rest a files-only card.

**Order of work (my suggestion):** layer 0 (agent-neutral value at once), then the hook mapping for Codex, Cursor and Copilot (the three biggest not yet done), then a Gemini mapping, then layer 2 for OpenCode and Aider. OTLP and proxy only when a named harness needs them.

## 6. What others do

| Tool | Capture method | Harnesses | Where its UI is |
|---|---|---|---|
| [ccusage](https://github.com/ryoppippi/ccusage) | Local data-file parsing, one Rust adapter per agent (`loader` plus `parser`) | 20 | CLI tables, status line |
| [tokscale](https://github.com/junhoyeo/tokscale) | Local session files or SQLite; a sync step for Cursor, Antigravity, Trae, Warp, Amp; Copilot OTel | about 90 listed | Ratatui TUI, web, leaderboard |
| [agentsview](https://github.com/wesm/agentsview) | Finds each agent's session directory, syncs to SQLite with FTS5, a file watcher and a 15 minute resync | 70+ | Local web page, desktop app, CLI |
| [cass](https://github.com/Dicklesworthstone/coding_agent_session_search) | Not stated (**unverified**) | "30+ providers" | TUI and CLI |
| [claude-code-viewer](https://github.com/d-kimuson/claude-code-viewer) | Reads `~/.claude/projects/*/<id>.jsonl`, watches | 1 | Local web app, PWA |
| [SpecStory](https://github.com/specstoryai/getspecstory) | `specstory run <agent>` wrapper, sync of old sessions; Cursor cloud through its API | 16 | Markdown files, CLI, editor extensions |
| [Langfuse](https://langfuse.com/integrations/other/claude-code) | A Stop hook reads the transcript and posts turns as traces; separate guides for Codex, Copilot, Cursor, OpenCode, Augment | per guide | Hosted or self-hosted web |
| [LiteLLM proxy](https://docs.litellm.ai/docs/tutorials/claude_responses_api) | HTTP proxy on a base URL | any with a base URL (Claude Code, OpenCode, Copilot/VS Code, Gemini CLI, Qwen Code, ...) | Admin web, logs |
| [OpenInference / Phoenix](https://github.com/Arize-ai/openinference) | In-process OTel instrumentation of SDKs; coding-agent coverage is only the Claude Agent SDK | frameworks | Phoenix web |
| Claude Code own OTel | Native export | 1 | Any OTLP backend |
| [git-ai](https://github.com/git-ai-project/git-ai) | Agents' own hooks call `git-ai checkpoint`; it installs the hooks; notes in git | 14 plus 4 cloud agents | `git ai blame`, editor plugins, paid team dashboard |
| [Entire](https://github.com/entireio/cli) | `entire enable` installs per-agent hooks and git hooks; a checkpoint per commit with prompts, transcript and files on git refs | 8 and "more" | CLI, hosted web view |
| [Agent Trace](https://agent-trace.dev) | Spec only | 0 shipped by itself | none |
| [herdr](https://github.com/herdrdev/herdr-core) | Programs self-report status over OSC 7501; core recognises no agent brand | any that reports | Terminal panes, app, mods |
| [Claude Squad](https://github.com/smtg-ai/claude-squad) | Runs agents in tmux; state detection not stated (**unverified**) | Claude Code, Codex, Gemini, Aider, OpenCode, Amp, any via `--program` | Go TUI |
| [Superset](https://github.com/superset-sh/superset) | Worktree and terminal per agent; depth varies per agent; status method not stated | 21 | Desktop app |
| [Emdash](https://github.com/generalaction/emdash) | Auto-detects agent CLIs, adds lifecycle hooks to some agents' user config | 9+ | Desktop app |
| [Vibe Kanban](https://github.com/BloopAI/vibe-kanban) | Not stated; README announces a sunset | 10+ | Local web kanban |
| [Happy](https://github.com/slopus/happy) | Wraps Claude Code and Codex; any ACP agent can plug in | 4, plus ACP | Phone and desktop apps |
| [CodeRabbit CLI](https://docs.coderabbit.ai/cli) | Reads git changes, not transcripts; agents call it | Claude Code, Codex, Cursor, Antigravity guides | Terminal, PR |
| Conductor, Crystal/Nimbalyst, Terragon, Sculptor, cmux (as a capture tool), Greptile, Graphite, Cubic, Helicone, OpenLIT, AgentOps | Not read | | **unverified** |

**The pattern.**

1. Files first. The tools with the most harnesses (agentsview 70+, tokscale 90) only parse files that the agents already write. The tools that need live data (git-ai, Entire) install hooks and cover 8 to 18.
2. Always one adapter per harness. No tool I read has a generic parser; the cost is a loader and a parser per agent, with format drift the main upkeep.
3. No one uses a proxy or ACP as the base for a multi-agent product. LiteLLM is a proxy product; OTel is vendor-specific.
4. UIs follow the user's place: trackers in CLI or TUI, viewers in a local web page, managers in a TUI or desktop app, attribution and review in git and the PR. None of the open tools I read draws in the agent's own terminal pane except Claude Code mods.
5. Long-lived data (Entire's checkpoints, git-ai's notes) lives in git, which is agent-neutral and survives the tool.

## 7. Where the map and cards show (the herdr question)

Assumption: the owner's herdr is the TUI (`herdrdev/herdr`) *or* the app (`herdrdev/herdr-app`, built on `herdrdev/herdr-core`). The docs differ, and which one he runs is an open question.

| Option | Renders a rich map? | Quiz interaction? | Reach | Examples and notes |
|---|---|---|---|---|
| **Herdr TUI plugin pane** (`overlay`, `popup`, `split`, `tab`, `zoomed`, from `herdr-plugin.toml`; opened by `herdr plugin pane open` or the socket's `plugin.pane.open`) | As a terminal program: Kitty graphics (herdr passes them through, per release notes seen via a snippet, **unverified**), else text | Yes: a TUI gets keys. A popup is modal and closes when the command exits; it fails with `ui_busy` if Settings is open | Herdr users, any terminal | "The entire Herdr CLI is the plugin API"; panes are terminal only, "native non-terminal plugin UI are not part of plugin v1" ([plugins](https://herdr.dev/docs/plugins/), [socket API](https://herdr.dev/docs/socket-api/)) |
| **Herdr app mods** (JavaScript, drawn natively) | `TextView.markdown`, `image()` and `svg()` of bundled files; no DOM, no webview ([mod guide](https://github.com/herdrdev/herdr-app/blob/master/defaults/AGENTS.md)) | Buttons, checkboxes, inputs | Herdr app users | Gallery: Herdr Zoo, markdown panel, a lazygit-style git panel ([gallery](https://herdr.dev/gallery/)). Mods run programs and use the network. Whether a mod can show an image from a file generated at run time is **unverified** |
| Herdr notification (`notification.show` over the socket, `notify()` in a mod) | Text | No click action in the docs | Herdr | Good for "your card is ready" |
| **TUI companion** (Textual, Ratatui, Bubble Tea) run in any pane | `ratatui-image` picks Kitty, Sixel or iTerm2 images, with a half-block fallback ([repo](https://github.com/benjajaja/ratatui-image)); `mermaid-ascii` draws flowcharts as text ([repo](https://github.com/AlexanderGrooff/mermaid-ascii)); D2 can export text but is beta ([D2](https://d2lang.com/tour/exports/)) | Yes | Any terminal, tmux, SSH | Examples are agent managers (Claude Squad, Agent Deck), not a map plus quiz: I found none that does both |
| Graphics protocols | Kitty protocol: kitty, Ghostty, WezTerm, iTerm2, Warp, Konsole. Sixel: iTerm2, WezTerm, xterm, foot; not Ghostty, kitty or Warp ([arewesixelyet](https://www.arewesixelyet.com/), [kitty](https://sw.kovidgoyal.net/kitty/graphics-protocol/)) | No | Terminal-dependent | tmux needs `allow-passthrough on` (3.3+) or Unicode placeholders; Zellij 0.45 added Kitty graphics (secondary source) |
| tmux `display-popup` | Text and images with passthrough limits | Yes | tmux users | Claude Code's own image element needs a tmux pane, not a popup ([mods gallery](https://code.claude.com/docs/en/plugins/mods/gallery.md)) |
| Zellij WASM plugins, floating panes | Rust plugins draw text; commands can open floating panes ([API](https://zellij.dev/documentation/plugin-api-commands.html)) | Yes | Zellij users | Web client renders the session in a browser tab, off by default |
| WezTerm, Kitty, iTerm2, Ghostty | Inline images (`wezterm imgcat`, `kitten icat`, OSC 1337) | Via the program | One terminal each | iTerm2's browser profile has no scripting API ([docs](https://iterm2.com/documentation-web.html)); Ghostty has an AppleScript dictionary on macOS |
| cmux (libghostty terminal) | Embedded scriptable browser pane | Yes (web) | Mac users of cmux only | The one terminal found with a browser pane and a CLI and socket for it ([repo](https://github.com/manaflow-ai/cmux)) |
| **In-agent: Claude Code mods** | `Image` (kitty 0.28+, Ghostty, iTerm2 3.5+ only), `Raster`, `Markdown`; `Svg` in Desktop and VS Code only | Buttons, Select, Input: a quiz fits | Claude Code only | Draws nothing in `claude -p` ([mods](https://code.claude.com/docs/en/plugins/mods/overview), [reference](https://code.claude.com/docs/en/plugins/mods/reference.md)) |
| In-agent: hook `systemMessage`, `statusLine`, `terminalSequence` | Text (10,000 chars); status line with OSC 8 links; OSC 9, 99, 777 notifications | No | Claude Code | A status line cannot hold a quiz ([hooks](https://code.claude.com/docs/en/hooks), [status line](https://code.claude.com/docs/en/statusline)) |
| In-agent: Amp plugin UI | `notify`, `input`, `select`, `confirm` with read-only image previews | Select works as a quiz | Amp CLI | ([plugin API](https://ampcode.com/manual/plugin-api)); custom components **unverified** |
| In-agent: OpenCode TUI plugins | **unverified** | ? | OpenCode | `cli.json`, no UI API on the page read |
| OS notification | Text | Click can run a command: `terminal-notifier -open`/`-execute` ([repo](https://github.com/julienXX/terminal-notifier), not notarized) | macOS | OSC 9/99/777 clickability is terminal-dependent, **unverified** |
| Menubar or tray app | ? (no sources gathered) | ? | macOS | **unverified** |
| Browser tab (local web page) | Full map, any size | Yes | Everyone, SSH with a port forward | The fallback; the thing the owner wants to avoid for daily use |
| Editor panes (VS Code Simple Browser, JetBrains) | Web page | Yes | Editor users | Page I tried 404'd; **unverified** |

**Recommendation, in order:**

1. **The cards live in a TUI companion** that a herdr plugin pane opens, as a popup (quick quiz after a turn) or a split (map beside the agent). It works in the TUI today, in any other terminal, and over SSH. It is the only option that gives a real quiz in a terminal for every harness, not just one.
2. **The map is drawn as an image where the terminal supports it** (Kitty graphics), else text. If the owner's setup is herdr in Ghostty, kitty or iTerm2, the image path works; in Terminal.app it falls back to mermaid-ascii or half-blocks, and a large map will be cramped. The full-size map is one key away as a link to the local web page.
3. **A herdr notification** tells him a card is ready; it is the "pull" that avoids a context switch he did not choose.
4. **Per-harness extras only after that:** a Claude Code mod (quiz in the transcript pane) and Amp's `select`. Never required, and each reaches one agent.
5. If he runs the herdr app, write a mod instead of (or beside) the TUI; its native markdown and image elements may be enough for the map. Check first whether a mod can show a generated image (**unverified**).

## 8. Corrections to `delivery-surfaces.md`

- It said Goose has "none found" for hooks. Goose now documents hooks following the Open Plugins spec ([post](https://goose-docs.ai/blog/2026/05/14/goose-hooks)); it also has OTel (third-party page).
- It listed Cline's hooks as SDK plugin hooks only. Cline also documents file-based executable hooks and CLI 2.0 flags `--acp` and `--hooks-dir` ([docs](https://docs.cline.bot/customization/hooks.md), [blog](https://cline.bot/blog/announcing-cline-cli-2-0)). The current state of the extension's hooks is **unverified**.
- It left Cursor's transcript as "?". Cursor's hook payload carries `transcript_path` (one helper read it in the hooks docs; another found it only through snippets: treat as **unverified** until the payload is read).
- It left Copilot CLI's compatibility open. Copilot CLI reads `.claude/settings.json` and accepts PascalCase event names with snake_case payloads ([reference](https://docs.github.com/en/copilot/reference/hooks-configuration)), which makes it a C, not just an M.
- Windsurf's docs now live under Devin (`docs.devin.ai/desktop`); the Continue-became-Devin claim I saw conflicts with them and is not used here.
- Aider is in maintenance mode (last release 0.86.2, Feb 2026), which lowers its priority. Gemini CLI's future is under Antigravity CLI; the end date is disputed.
- It said "Antigravity's vendor docs: not read". The Antigravity hook config location is now sourced ([docs](https://antigravity.google/docs/hooks.md)); the event names and payload were not extracted. This repo's `plugin-agy/hooks.json` uses `.json` at plugin root, not `.agents/hooks.json`: check which the current CLI reads.
- The ACP conclusion stands, with a new fact: the proxy-chains RFD is a draft, so a conductor is not yet a stable tap.

## 9. Open questions for a grilling session

1. Which herdr does the owner run, the TUI (`herdr`) or the app (`herdr-app`), and which terminal and multiplexer sit under it (Ghostty, kitty, iTerm2, Terminal.app, tmux)? The answer decides image or text for the map.
2. Is "the developer never leaves the terminal" a hard rule, or is a browser tab for the full map acceptable when a card links to it?
3. How many harnesses must work at launch? Proposal: Claude Code (done), Antigravity (done), then Codex, Cursor, Copilot, Gemini as the first four; everything else gets the files-only card.
4. Is a files-only card (no prompt, no reasons) worth showing, or should a harness without a prompt source show nothing?
5. Is the owner willing to write to each harness's hook config (Entire, git-ai and Emdash do), or must the Studio load hooks without touching user config as it does for Claude Code with `CLAUDE_PLUGIN_DIRS`? (herdr.md row 23 chose the second for Claude Code.)
6. May the recorder read other tools' transcripts (agentsview style)? That reads local session files, never credentials, and stays inside the repo's hard rules, but it touches each agent's private data.
7. Do we reuse an existing parser set (agentsview, ccusage) or write our own? Check licences first (**unverified**); reuse would turn "one parser per harness" into a dependency.
8. Is a local LLM proxy acceptable for Claude subscription users, given Anthropic's wording on intermediating credentials? It needs a legal read, not an engineering one.
9. Should git-ai notes be an input (free line attribution for 18 agents), and should the Studio write Agent Trace records out for other tools?
10. Is the quiz allowed to block the terminal pane (popup, modal) or must it never take focus?
11. Who owns the format dialect work: do we publish our own normalised hook schema (as OpenHook tried, one star, no adopters) or follow Agent Plugins 1.1.0?

## 10. Unverified or not fully read

- Hook payloads and exit codes for Windsurf/Devin Desktop, Devin CLI, Goose (exit codes), Kimi, Kiro (V3 casing is third-party), Continue `cn` (docs may not have shipped), Augment (compatibility not confirmed), Qoder, Pi, OpenClaw, Hermes, Kilo (inherited from OpenCode only by a secondary source).
- Antigravity CLI event names and payloads; whether `plugin-agy/hooks.json` matches the current config location.
- Gemini CLI end-of-life date (sources conflict); whether Gemini CLI accepts a base URL with Google login (`GOOGLE_GEMINI_BASE_URL` appears only in gateway guides).
- Transcript paths for Codex (`~/.codex/sessions`), Claude Code (`~/.claude/projects`), OpenCode (SQLite), Amp (`threads/*.json`), Pi: all third-party or from this repo's fixtures. Cursor, Copilot, Droid, Qwen, Kiro, Goose, Cline, Crush and most others: not found.
- OTel: OpenCode (flag reported unreliable), Amp, Cursor editor, Droid, Kiro agent level, Goose content capture, `OTEL_SEMCONV_STABILITY_OPT_IN` for GenAI; Copilot span names came partly from third-party docs.
- Custom base URL with a subscription login for Codex (ChatGPT), Copilot, Cursor, OpenCode, Gemini; whether Cursor, Windsurf and Copilot IDE can be proxied at all.
- Anthropic's position on a local pass-through proxy (the policy text is verified; its application is not stated).
- Whether the Agent Plugins 1.1.0 hooks section is ratified, and which harness implements it.
- Agent Trace repository URL (404) and supporter list beyond Thoughtworks' three; git-ai's agent list is from its README.
- Codex cloud, Devin cloud, Jules, Warp, Zed built-in agent, Junie, Trae, Codebuff: hooks, OTel and base URL not found or status not confirmed (Trae and Jules 2026 status especially).
- Roo Code, Plandex and Cody status come from third-party posts (Roo from the vendor's own blog).
- Herdr: Kitty graphics pass-through (release-note snippets, not the docs), behaviour inside tmux or over SSH, mod image support for runtime files, what the "Browser tabs", "Image Viewer" and "Git Graph" mods do.
- Zellij 0.45's Kitty support (secondary source) and plugin pipes; tmux popup version; iTerm2 Python API and status bar; WezTerm Lua; Warp plugins; menubar and tray apps; VS Code Simple Browser and JetBrains web views; clickability of OSC 9/99/777 notifications.
- Tools not read: Conductor, Crystal/Nimbalyst, Terragon, Sculptor, Greptile, Graphite, Cubic, Helicone, OpenLIT, AgentOps, splitrail; how cass, SpecStory (live or file), Claude Squad, Superset and Vibe Kanban capture state; tokscale's README tail (the last 34k characters).
- Adapter sizes are bytes from GitHub tree listings, not lines read; "about 250 to 450 lines for a simple adapter" comes from file sizes of a few adapters (goose, amp, gemini).
- Every coverage figure in sections 3.2, 4 and 5 is my count over the table and depends on its "?" cells being read as "not confirmed", not "no".
