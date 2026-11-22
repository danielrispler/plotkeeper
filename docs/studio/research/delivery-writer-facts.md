# Writer facts: one-shot card writer per harness (2026-11-22)

Legend: [L] = run locally today (versions: claude 2.1.323, codex-cli 0.171.0, agy 1.5.14, opencode 1.19.6); [D] = vendor doc page named; unverified = not found / not run. Copilot, Cursor `agent`, Cline, Qwen, Devin are NOT installed here (`command -v` empty): doc facts only.

## Today's code (what the abstraction must replace)
- `understanding/src/lib.rs:983-1000` `run_with`: `claude --tools "" --strict-mcp-config --safe-mode [--model M --effort E --system-prompt S] -p`, env_remove ANTHROPIC_API_KEY / ANTHROPIC_AUTH_TOKEN / CLAUDE_CODE_PLUGIN_DIRS (forces subscription login), cwd = neutral folder, prompt on stdin (NULs stripped), own process group, timeout kill (`stdout_within`, 1020-1052).
- Reply is free text: `crate::parse_object(reply)` in `summary.rs:127` extracts a JSON object from text. No `--output-format`, no schema, no usage read. Failure = non-zero exit, empty stdout, or timeout, each one log line (`availability.failed/worked`).

## Claude Code
1. `claude --tools "" --strict-mcp-config --safe-mode -p` with prompt on stdin [L]. `--tools ""` disables all built-ins [L help]. `--safe-mode` skips plugins/hooks/skills/CLAUDE.md [L help]. Neutral folder: today's code does it already.
2. Structured: `--output-format json` gives ONE object [L]: `result` (text), `structured_output` (parsed, when `--json-schema '<schema>'` given), `is_error`, `api_error_status`, `terminal_reason`, `usage{input_tokens,output_tokens,cache_*}`, `total_cost_usd`, `modelUsage`, `duration_ms`. Ran with Haiku + schema: `structured_output:{"answer":"pong"}`, 2.3 s wall, `num_turns:2` (schema is delivered through an internal tool turn, `stop_reason:"tool_use"`) [L].
3. `--model <alias|full>` (alias `haiku` worked, resolved to claude-haiku-5-5) [L]; `--effort low|medium|high|xhigh|max` [L help]. Cheap model: yes.
4. Subscription login works (existing design); API key env vars are stripped to force it.
5. Not logged in [L, empty CLAUDE_CONFIG_DIR]: exit 1, stdout JSON `is_error:true`, `result:"Not logged in · Please run /login"`, `terminal_reason:"api_error"`, `subtype` still "success" (so test `is_error`, not subtype). Quota/rate-limit text: not exercised (unverified; `api_error_status` field exists).
6. Rating: **reliable**.

## Codex CLI
1. `codex exec --skip-git-repo-check -s read-only --ephemeral --ignore-user-config - ` (prompt from stdin when `-` or no arg; stdin plus a prompt arg appends a `<stdin>` block) [L help]. `-C <dir>` sets root; `--skip-git-repo-check` avoids the repo requirement. No flag to remove the shell tool: only `-s read-only` sandbox (model could still read files; tool list not disableable on CLI: unverified, `-c` config overrides exist). `--ignore-user-config` skips `~/.codex/config.toml` but auth still uses CODEX_HOME [L help]. Trust: `--trust` only saves trust; run in neutral folder with `--skip-git-repo-check` produced no prompt [L].
2. `--output-schema <FILE>` (JSON Schema of final response) + `-o/--output-last-message <FILE>` writes the final message; `--json` prints JSONL events [L]. Observed events: `thread.started`, `turn.started`, `item.completed{item:{type:"agent_message",text:"{\"answer\":\"pong\"}"}}`, `turn.completed{usage:{input_tokens:14570,cached_input_tokens,output_tokens,reasoning_output_tokens}}`. `-o` file held `{"answer":"pong"}` [L]. 4.0 s wall. Note ~14.5k input tokens of Codex's own system prompt overhead [L].
3. `-m <model>` [L help]; effort via `-c model_reasoning_effort="low"` (config key, from Codex config docs, NOT run here: unverified). Cheap model: yes (a mini model); list not checked.
4. ChatGPT-login works (my run used the existing login, no API key set) [L]; documented in OpenAI Codex auth docs (not fetched here).
5. Bad CODEX_HOME: prints `Error finding codex home...`, non-zero (exit code not captured). Not-logged-in and quota messages not exercised (unverified). A failed turn presumably emits `turn.failed`/`error` JSONL events (unverified).
6. Rating: **reliable-to-workable**: schema + subscription OK; tools cannot be fully off (read-only sandbox is the lockout), 14.5k overhead tokens.

## Antigravity CLI (`agy`)
1. `agy --output-format json --json-schema <schema|file> --model M --print-timeout 15m -p "<prompt>"` [L; repo `outside-review.mjs:188-193`]. Stdin: `-p` takes the prompt as argument; `--input-format text|stream-json` exists for stdin streams; plain-text stdin prompt: unverified (repo passes the prompt as an arg with stdin ignored). Tools: NO flag to disable. The repo locks tools with an installed plugin hook (`deny-hook.mjs`, `~/.gemini/config/plugins`, see `workbench/src/lib.rs`, outside-review.mjs:13) and a throwaway folder; `--mode plan` and `--sandbox` exist [L help] but whether they fully block tools: unverified. Folder: runs in cwd; the repo uses a throwaway worktree folder.
2. JSON [L]: `{"conversation_id","status":"SUCCESS|ERROR","response":"pong\n{...}\n","duration_seconds":6.1,"num_turns":2,"structured_output":{"answer":"pong"},"json_schema":{...},"usage":{input_tokens,output_tokens,thinking_tokens,cache_read_tokens,total_tokens}}`. `response` mixes prose and JSON; read `structured_output`. 8.6 s wall.
3. `--model <id>` (`agy models` lists e.g. `gemini-4-neon-low|medium|high|xhigh`, `gemini-4-argon-*`, `gemini-4-hydrogen-*`: effort is baked into model ids) and `--effort low|medium|high|xhigh|max` [L help]. Cheap model: yes.
4. Subscription login (Google OAuth); no key needed [L]. Gemini CLI sub login ended 2026-06-18 (harness-facts.md section 1).
5. Exit 3 = `--print-timeout` hit (repo comment, outside-review.mjs:242). Not logged in [L, empty HOME]: prints an OAuth URL, WAITS 60 s for a pasted code on stdin, then JSON `{"status":"ERROR","error":"authentication failed or timed out",...,"finished_reason":"error"}` (exit code not captured). So a logged-out agy blocks 60 s: caller needs its own timeout. Quota: repo greps stderr/stdout for `quota|resource.exhausted|rate.limit` (outside-review.mjs:207), no structured field. `agy -p /usage --output-format json` reports remaining quota [repo].
6. Rating: **workable**: schema + subscription proven in this repo, but no tool lockout flag (needs the hook plugin), 60 s logged-out hang, ~6-9 s latency.

## GitHub Copilot CLI (not installed)
1. `copilot -p "<prompt>" -s --output-format=json --available-tools='' ...` ([D](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-programmatic-reference)). Stdin: not documented. Tools: `--available-tools` whitelist, `--excluded-tools`, `--deny-tool`, `--no-ask-user`; whether an empty whitelist disables all: unverified. `--trust-folder` for hooks/MCP/plugin settings of the cwd (one session); neutral folder avoids those.
2. `--output-format=json` = JSONL; per-record shape for a normal prompt run NOT documented (only `workflow.result` for `copilot workflow run`). No schema flag documented. `-s` = text only.
3. `--model=MODEL` / `COPILOT_MODEL`. Effort: no CLI flag; `effortLevel` in `~/.copilot/settings.json` only (affects the user's setting, so it needs a COPILOT_HOME override: unverified).
4. Token env (`COPILOT_GITHUB_TOKEN` > `GH_TOKEN` > `GITHUB_TOKEN`) or `copilot login`; that `-p` uses the login: unverified.
5. `-p`: non-zero if interrupted or the final response is empty; no codes listed. Quota/login signals: undocumented.
6. Rating: **workable** at best (free text or undocumented JSONL, no schema, effort not per call).

## Cursor CLI (`agent`, not installed)
1. `agent -p "<prompt>" --output-format json --mode ask --trust --workspace <dir> --model <m>` ([headless](https://cursor.com/docs/cli/headless), [parameters](https://cursor.com/docs/cli/reference/parameters)). `-p` "has access to all tools, including write and shell"; `--mode ask` (read-only Q&A) and `--mode plan` exist; no way to remove tools. `--trust` skips the workspace trust prompt (headless only). Stdin not documented.
2. `json` = one object, [documented](https://cursor.com/docs/cli/reference/output-format): `type:"result"`, `subtype:"success"`, `is_error:false`, `duration_ms`, `result` (final text), `session_id`, `request_id?`. NO usage field. No schema flag. On failure: non-zero exit, message on stderr, no JSON.
3. `--model <model>`, `--list-models`. No effort flag documented. Cheap model: yes.
4. Docs show `CURSOR_API_KEY` for scripts; `agent login` exists; `-p` on subscription login: unverified (likely).
5. Non-zero exit + stderr; no codes. Latency undocumented.
6. Rating: **workable** (JSON envelope with `result`, parse needed; `ask` mode is a soft lockout).

## OpenCode (installed, 1.19.6)
1. `opencode run "<prompt>" --format json -m provider/model --variant <effort> --dir <dir> --agent <a>` [L help]. Prompt is a positional (`message..`); stdin: unverified. Tools: no flag to disable; `plan` agent is read-only for files ([agents docs](https://opencode.ai/docs/agents/), via harness-facts); `--auto` auto-approves (do not use). Permissions via config; `OPENCODE_CONFIG_CONTENT` inline config: not confirmed in docs (unverified).
2. `--format json` = "raw JSON events" [L help]; event shape not documented in docs read ([cli](https://opencode.ai/docs/cli/) gives only the basic example), not run here. No schema flag.
3. `-m provider/model`, `--variant` ("provider-specific reasoning effort, e.g. high, max, minimal") [L help]. `opencode models` lists many incl. free `opencode/*` models [L].
4. Subscription logins: OpenAI ChatGPT and GitHub Copilot via `/connect` (harness-facts section 4); this machine has 0 credentials [L `opencode auth list`]. Claude login: unverified.
5. Exit codes/failure signals: undocumented.
6. Rating: **painful/flaky** (events undocumented, no tool lockout flag, no schema), though it can ride a ChatGPT/Copilot subscription.

## Cline CLI (not installed)
1. `cline --json "<prompt>"` or `echo ... | cline "<prompt>"`; headless starts on `--json`/piped stdin/redirected stdout ([D](https://docs.cline.bot/cline-cli/overview)). `-c/--cwd <path>`, `--data-dir` isolates state, `-t/--timeout <s>`. Tools: no disable flag; `--auto-approve` defaults true; no shell allow/deny list (docs say use a PreToolUse hook). `-p` is plan mode, not print.
2. `--json` = NDJSON, each line has `type`; `agent_event` lines, final `run_result` line with `text`. Full schema not on that page; usage/errors: unverified. No schema flag.
3. `-m/--model`, `-P/--provider`, `--thinking none|low|medium|high|xhigh`.
4. `cline auth` (Cline provider, ClinePass, or own key); no ChatGPT/Claude subscription path documented.
5. Exit codes: undocumented.
6. Rating: **painful/flaky** (no tool lockout, hooks needed, own provider/credit model).

## Qwen Code CLI (not installed)
1. `qwen -p "<prompt>" -o json --exclude-tools shell,write,edit,... --safe-mode` or prompt on stdin ([D](https://qwenlm.github.io/qwen-code-docs/en/users/features/headless/)). `--exclude-tools` removes built-ins entirely; `--approval-mode plan`; trust of folder in headless: unverified.
2. `-o json` = JSON ARRAY of message objects; last has `type:result`, `subtype`, `is_error`, `duration_ms`, `result`, `usage`, `stats`. `--json-schema` appears in the page (structured_output exempt from `--max-tool-calls`) but its usage is not documented there.
3. `-m/--model` (e.g. qwen3-coder-plus). Effort: no flag found.
4. Auth: not on that page; the free OAuth tier status unverified.
5. Exit codes: 53 turn cap, 55 run budget, 130 SIGINT; success code not stated.
6. Rating: **workable** (good output mode and tool lockout; auth/schema undocumented; it is Qwen models, not the user's Claude/GPT subscription).

## Windsurf / Devin CLI (not installed)
1. `devin -p "<prompt>" --respect-workspace-trust false --model <m> --permission-mode <mode>` ([D](https://docs.devin.ai/cli/reference/commands.md)); `--prompt-file <FILE>`; stdin not documented. Print mode fails in an untrusted dir unless trust check is skipped. Tools: no disable flag; permission modes normal/accept-edits/smart/dangerous/autonomous; no read-only mode documented.
2. No global output-format flag; `--export [PATH]` writes the conversation (ATIF). Only free text. No schema.
3. `--model` / `DEVIN_MODEL`. No effort flag.
4. `devin auth login`; `WINDSURF_API_KEY` only for ACP server. Subscription: login is the documented path.
5. Exit codes: undocumented.
6. Rating: **painful/flaky** (text only, no tool lockout, undocumented failures).

## Proposal: minimal writer abstraction (PROPOSAL, not a decision)
Contract (one trait, one call):
- Input: `system` (string), `user` (string), `schema: Option<JsonSchema>`, `tier: Cheap|Standard`, `effort: Option<Low|Med|High>`, `timeout`.
- Output: `Ok{ text: String, json: Option<Value>, usage: Option<{input, output}> }` or `Err(WriterError)` with kind `NotLoggedIn | QuotaOrRateLimit | Timeout | NotInstalled | Empty | Other(msg)`.
- Rule: if `schema` is set and the harness has no native schema flag, the writer falls back to `json = parse_object(text)` (today's behavior at summary.rs:127); the caller always validates the object itself, so native schema is an optimization, not a requirement.

Per-harness variation, and only this: (a) a command template (program, fixed lockout flags, model/effort flags, how the prompt is passed: stdin vs arg vs file); (b) an extractor from stdout/exit code to the contract (claude: `.result`/`.structured_output`/`.is_error`; codex: `-o` file or last `agent_message`; agy: `.structured_output`, `.status`; cursor: `.result`; qwen: last array element `.result`); (c) a failure classifier (exit code plus regex on `quota|rate.limit|not logged in|auth`); (d) a tier-to-model-id map. Everything else (neutral folder, own process group, timeout kill, NUL stripping, availability tracking, the 300-char error head) stays shared code in `ClaudeWriter::stdout_within`. Tool lockout is a per-harness capability flag (`Off | ReadOnly | None`): Claude `Off`, Qwen `Off`, Codex `ReadOnly` sandbox, Cursor `ReadOnly` (ask mode), agy needs the hook plugin; for harnesses without `Off` the prompt stays "do not use tools", already in summary.rs:429-534, and the neutral empty folder limits the blast radius.
Suggested scope: first-class Claude, Codex, agy (all proven here with JSON and subscription); the rest are the same template shape but unproven.

## Summary table
| harness | command | JSON/schema | tools off | subscription | rating |
|---|---|---|---|---|---|
| Claude Code | `claude --tools "" --safe-mode --model haiku --output-format json --json-schema S -p` (stdin) [L] | json envelope + `structured_output`, usage, `is_error` [L] | yes (`--tools ""`) | yes | reliable |
| Codex CLI | `codex exec --skip-git-repo-check -s read-only --ephemeral --output-schema F -o OUT --json -` (stdin) [L] | schema + last-message file + JSONL usage [L] | no, read-only sandbox only | yes (ChatGPT login) | reliable/workable |
| Antigravity `agy` | `agy --output-format json --json-schema S --model M -p "..."` [L] | json + `structured_output`, usage [L] | no flag; repo uses hook plugin | yes (Google OAuth), 60 s hang if logged out | workable |
| Copilot CLI | `copilot -p "..." -s --output-format=json --available-tools=...` [D] | JSONL, shape undocumented; no schema | whitelist (empty unverified) | token env or login (unverified) | workable |
| Cursor CLI | `agent -p "..." --output-format json --mode ask --trust` [D] | json `result`, no usage; no schema | no (ask mode read-only) | `CURSOR_API_KEY` documented; login unverified | workable |
| OpenCode | `opencode run "..." --format json -m p/m --variant E` [L help] | raw events, undocumented; no schema | no flag (plan agent) | ChatGPT/Copilot logins | painful/flaky |
| Cline CLI | `cline --json "..."` [D] | NDJSON `run_result.text`; no schema | no | own provider/ClinePass | painful/flaky |
| Qwen Code | `qwen -p "..." -o json --exclude-tools ... --safe-mode` [D] | json array, `result`+`usage`; `--json-schema` mentioned | yes (`--exclude-tools`) | unverified | workable |
| Windsurf/Devin CLI | `devin -p "..." --respect-workspace-trust false` [D] | text only | no | `devin auth login` | painful/flaky |
