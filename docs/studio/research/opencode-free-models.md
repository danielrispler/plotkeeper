# opencode's free models as routing candidates

Research for the model-routing grilling (draft spec `spec-model-routing.md`, "Candidates"), 26 November 2026. Question: can the free models in Daniel's opencode picker, or OpenCode Go, join `scripts/agents/routing.json`'s tiers, and what would their CLI entry and `quota()` look like?

Every claim is marked **verified** (with its source) or **unverified**. No credential or token file was read (`~/.local/share/opencode/auth.json`, `~/.config/opencode/opencode.jsonc` and the Keychain were left alone). Raw working notes: `local-evidence/2026-11-25-model-routing-grilling/reports/research-opencode.md`.

## How this was read

- **Local CLI.** `opencode --version` → `1.19.6` (Homebrew `anomalyco/tap`). `opencode --help`, `opencode run --help`, `opencode models`, `opencode models opencode --verbose`, `opencode debug skill` (run in the repo). `herdr --version` → `0.9.5`, `herdr agent start --help`.
- **One free probe.** `opencode run --pure -m opencode/hy4-flash-free --format json "Reply with exactly the word OK…"` in an empty scratch folder, with `OPENCODE_DISABLE_CLAUDE_CODE=1 OPENCODE_DISABLE_EXTERNAL_SKILLS=1 PLOTKEEPER_INTERNAL=1` so no personal config left the machine. It cost nothing (`cost: 0`). Result in §4.
- **opencode source.** `anomalyco/opencode` (`sst/opencode` redirects there). Docs at `dev` `c85b83e` (25 Nov 2026); code at tag `v1.19.6` (`f4e363c`), the version installed. Short forms:
  - `zen:` = `services/site/src/docs/content/console/models.mdx` (the live opencode.ai/docs/zen page; a WebFetch of it matched)
  - `go:` = `…/content/console/go.mdx`
  - `v1docs:` = `…/docs/v1/content/` (the docs for 1.x)
  - `src:` = `packages/` at `v1.19.6`
- **Catalog.** `https://models.dev/api.json`, fetched 26 Nov 2026. It's the catalog opencode loads (`opencode models --help`: "refresh the models cache from models.dev").
- **Model makers.** Hugging Face model cards (raw `README.md`) and Reflection's model card. Blog write-ups found by search were used only to find those primary pages.

## Summary table

"Data" is the free variant's terms from `zen:498-513` and `zen:446-458`. Zen says "All our models are hosted in the US and EU" (`zen:500`). Benchmarks are the maker's own numbers, unless noted.

| Picker name (id) | Maker · size | Zen ctx / out | Data, free variant | Coding evidence | Verdict |
|---|---|---|---|---|---|
| **Beam Free** (`beam-free`) | Reflection AI (US) · 501B / 23B active, open weights | 1M / 131K | Not trained on; kept 30 days | SWE-bench Verified 81.6, Terminal-Bench 2.1 80.1, DeepSWE 43.3 | **cheap**, second (#592). Medium confidence |
| **DeepSeek V4.1 Flash Free** (`deepseek-v4.1-flash-free`) | DeepSeek (China) · 552B / 8–16B, MIT | 200K / 32K | Zero retention, no training | TB 2.1 90.6, DeepSWE 74.2. In the **OpenCode** scaffold: 65.5 / 85.0 | **cheap**, first (#592). Medium |
| **MiMo-V2.6-Flash Free** (`mimo-v2.6-flash-free`) | Xiaomi (China) · 309B / 15B, MIT | 200K / 32K | Not listed as an exception, so Zen's default (zero retention) applies. But "the team is using this time to collect feedback" | DeepSWE 67.9, TB 2.1 87.6, TB 4.0 28.8 | **cheap**, third. Low–medium |
| **Nemotron 3.5 Super VL Free** | NVIDIA (US) · 120B / 12B | 1M / 131K | NVIDIA trial endpoint: "do not submit personal or confidential data. Your use is logged" | SWE-bench Verified 71.1, TB 2.1 60.7 | cheap only if Daniel accepts logging. Low |
| **Hy4 flash Free** (`hy4-flash-free`) | Tencent (China) · 220B / ~12B | 1M / 65K | Zero retention, no training | DeepSWE 64.6, TB 4.0 19.2. Tencent recommends full Hy4 for long-horizon SWE | **unsuitable** (weakest coder of the ZDR set). Medium |
| **Flint 40B Free** (`flint-40b-free`) | Recursive · 39.8B / 2.5B, Apache-2.0 | 262K / 65K | "May be used to improve the model", kept 30 days | Self-reported SWE-bench Verified 74.2, TB 2.1 54.9 | unsuitable (trains on code, tiny active size). Low |
| **Nemotron 3.5 Lightning Free** | NVIDIA · 30B / 3B | 262K / 262K | NVIDIA trial logging, as above | SWE-bench Verified 51.6, TB 2.1 24.6 | **unsuitable**. High |
| **Iceman Free** | stealth, maker unknown | 262K / 131K | "Retained and used to improve the model" | none published | **unsuitable**. High |
| **Tidepool Alpha Free** (new, not in Daniel's list) | stealth, unknown | 1M / 131K | Retained and used to improve | none | **unsuitable**. High |
| **Big Pickle** (`big-pickle`) | stealth, unknown. Catalog release 2025-10-17 | 200K / 32K | "May be used to improve the model" | none | **unsuitable**. High |
| Inkling 2 Small, LongCat 2.5, GLM-5.3-Flash (Free) | Thinking Machines (US); Meituan (China); Zhipu (China) | — | — | — | **gone from Zen.** See §1 |

## 1. The models, one by one

**What Zen offers today.** Verified: `opencode models` lists ten free models: `beam-free`, `big-pickle`, `deepseek-v4.1-flash-free`, `flint-40b-free`, `hy4-flash-free`, `iceman-free`, `mimo-v2.6-flash-free`, `nemotron-3.5-lightning-free`, `nemotron-3.5-super-vl-free`, `tidepool-alpha-free`. All are served at `https://opencode.ai/inference/openai/v1` with `cost: 0` (`--verbose`). The docs list the same ten, plus Jev 1.13 Free, a decision model rather than a chat model (`zen:136-145`, `zen:297-307`).

**Three models are gone.** Inkling 2 Small Free, LongCat 2.5 Free and GLM-5.3-Flash Free are missing from both `opencode models` and the Zen docs. models.dev marks all three `status: deprecated`. Verified. Daniel's picker probably shows a cached catalog. That last part is unverified: the TUI's cache was not inspected.

**Every one is temporary.** Each entry says "free on OpenCode for a limited time" (`zen:448-458`). When a promotion ends, the server answers "Free promotion has ended for {model}. You can continue using the model by subscribing to OpenCode Go" (`src:console/app/src/routes/zen/util/handler.ts:564-569`, string at `src:console/app/src/i18n/en.ts:433`). Verified.

**Who serves them.** Zen is the server; it proxies to upstream providers it doesn't name per model. The docs say "Our providers follow a zero-retention policy and do not use your data for model training, with the following exceptions" (`zen:500`). Verified.

- **NVIDIA's free endpoints** serve both Nemotron free models. NVIDIA's text applies: "Trial use only — do not submit personal or confidential data. Your use is logged for security purposes and to improve NVIDIA products and services" (`zen:506-507`).
- **Beam's provider** blocks a list of countries that includes China and Russia (`zen:455`).

**China hosting.** Zen's free models are US/EU-hosted by the docs (`zen:500`). Only OpenCode Go has a China-hosted model, LongCat 2.5: "Hosted in China. Requires Global regions" (`go:343`). Go's DeepSeek also needs the **Global** region (`go:51`). So Zen's free DeepSeek may run on different infrastructure from Go's. The docs don't say which. Unverified.

**Per model, from the makers' cards:**

- **Beam.** Reflection AI. 501.3B total, 23.4B active, 1M context, knowledge cutoff 30 June 2026. Reflection lists software engineering and agentic coding as intended uses ([model card](https://reflection.ai/beam-model-card)). It reports SWE-bench Verified 81.6%, SWE-bench Pro v1 66.1%, Terminal-Bench 2.1 80.1%, DeepSWE v1.1 43.3% and MCP Atlas 80.8%, without naming a harness. Verified that the card says this. The DeepSWE figure is far below its peers', so the set is internally uneven.
- **DeepSeek V4.1 Flash.** 552B backbone, 8B/16B activated, MIT, 1M context ([card](https://huggingface.co/deepseek-ai/DeepSeek-V4.1-Flash)). Terminal-Bench 2.1 90.6, TB 3.0 30.0, TB 4.0 31.2, DeepSWE v1.1 74.2.
  - Its scaffold table is the only first-party number measured in **opencode** itself: DeepSWE 65.5 and TB 2.1 85.0, against 69.8 / 88.0 in Claude Code.
  - Zen's free variant is the `-1017` build (catalog release 2026-10-17), capped at 200K context and 32K output (`--verbose`). Verified.
- **MiMo-V2.6-Flash.** Xiaomi. 309B/15B, MIT, 1M context ([card](https://huggingface.co/XiaomiMiMo/MiMo-V2.6-Flash-RL)). DeepSWE 67.9, Terminal-Bench 2.1 87.6, TB 4.0 28.8, ProgramBench 26.0, Toolathlon-Verified 73.6. Zen caps it at 200K / 32K.
- **Hy4 flash.** Tencent. 220B including 64B Engram, about 12B active, 1M context, Tencent Hy License ([card](https://huggingface.co/tencent/Hy4-flash)). The card's benchmark image gives DeepSWE 64.6, Terminal-Bench 4.0 19.2 and ProgramBench 16.0. The same image has DeepSeek-V4.1-Flash-1017 at 73.8 / 40.0. Tencent itself says: "For complex tasks such as long-horizon software engineering, we recommend Hy4."
- **Nemotron 3.5 Super VL.** 120B/12B, released 15 Oct 2026 ([card](https://huggingface.co/nvidia/NVIDIA-Nemotron-3.5-Super-VL-120B-A12B-BF16)). SWE-bench Verified 71.10, SWE-bench Pro 51.78, Terminal-Bench 2.1 60.67. Offline it scores TB 4 1 and DeepSWE 24.65.
- **Nemotron 3.5 Lightning.** 30B/3B, released 11 Aug 2026 ([card](https://huggingface.co/nvidia/NVIDIA-Nemotron-3.5-Lightning-30B-A3B-BF16)). SWE-bench Verified 51.56, Terminal-Bench 2.1 24.58.
- **Flint.** Recursive. 39.8B with about 2.5B active, Apache-2.0, 262,144 context, published on Hugging Face 21 Oct 2026 ([card](https://huggingface.co/RecursiveSI/flint)). "Terminal-Bench 2.1 54.9%, SWE-bench Pro v2 89.7%, SWE-bench Verified 74.2%." Self-reported; Terminal-Bench ran in Terminus 2 and SWE-bench in mini-swe-agent.
- **Stealth models.** Zen calls Big Pickle, Iceman and Tidepool Alpha stealth models (`zen:452-454`). opencode's own data page lists Iceman's lab as "Unknown" ([page](https://opencode.ai/data/unknown/iceman)). Big Pickle has been "free for a limited time" since 2025-10-17 (models.dev). A user asked "Is zen/big-pickle glm 4.6?" (anomalyco/opencode#4276); this note found no first-party answer. Its identity is **unverified**.
- **The three removed models**, for the record:
  - Inkling 2 Small: Thinking Machines, 594B/17B, 1M context, DeepSWE 72.1% ([card](https://huggingface.co/thinkingmachines/Inkling-2-Small)).
  - LongCat 2.5: Meituan, 1.6T / about 48B, TB 2.1 88.4 ([card](https://huggingface.co/meituan-longcat/LongCat-2.5)).
  - GLM-5.3-Flash: Zhipu, 320B/18B ([card](https://huggingface.co/zai-org/GLM-5.3-Flash)).

**Rust and TypeScript.** No card reports Rust- or TypeScript-specific coding scores. Multilingual SWE-bench is the closest: Nemotron Super VL scores 67.72, Beam 78.3. **Unverified** for plotkeeper's stack until a spike.

## 2. Fitness as a coding worker

- **Tool use.** All ten models declare `toolcall: true` and `reasoning: true` (`--verbose`). Agentic numbers differ widely by scaffold. DeepSeek's DeepSWE ranges from 65.5 to 74.2 across eight scaffolds, and opencode's own scaffold is near the bottom (DeepSeek card, scaffold table). Expect maker numbers to overstate how these models do inside opencode. Medium confidence.
- **Benchmarks aren't comparable across makers.** Each uses its own harness, and SWE-bench Verified and DeepSWE are reported by different subsets of makers. The rows that share one table are the fairest comparisons:
  - Tencent's: Hy4 flash < GLM-5.3-Flash < DeepSeek-V4.1-Flash
  - NVIDIA's: Nemotron Super VL < Inkling-Small < GLM-5.3-Flash on SWE-bench Verified
- **Context caps.** The 200K / 32K output caps on DeepSeek and MiMo are enough for a three-file cheap ticket (unverified). They are shorter than the models' native 1M.
- **Recommendations** are in the table's Verdict column. The reasons, in short:
  - **Beam:** strongest self-reported SWE-bench, a US maker, no training on prompts.
  - **DeepSeek:** strongest Terminal-Bench, zero retention, measured in opencode.
  - **MiMo:** close behind DeepSeek, but its data terms are vaguer.
  - **Nemotron Super:** decent scores, but NVIDIA logs every prompt.
  - **Out:** Hy4 flash on its maker's own advice. Flint, Iceman, Tidepool and Big Pickle on data terms or missing evidence. Lightning on scores.
- **Tier.** None is a **medium** candidate yet. The spec puts unfamiliar models into `cheap` by default (user story 18), and nothing here is measured on plotkeeper.

## 3. OpenCode Go

- **Price.** Go is $10 a month, Go Plus $40, Go Teams $20 per member with a two-member minimum (`go:8-12`). Verified.
- **What it covers.** It's a separate provider, `opencode-go/<model-id>` (`go:181-188`). Its catalog includes GLM-5.3, Kimi K3 / K3.1 Pro, DeepSeek V4 Pro / V4.1 Flash, Qwen4 Max, MiMo-V2.6-Pro/Flash, MiniMax M3, Gemini 4 Argon, Claude Haiku 5.5, GPT-6 Luna and others, plus free entries: Beam Free, Hy4 flash Free, Iceman Contributor Free and HungerG Free (models.dev `opencode-go`). Verified. It isn't connected on this machine: `opencode models` lists no `opencode-go/*`.
- **Limits.** Paid models share one allowance:
  - a 5-hour limit, 20% of the monthly allowance
  - a weekly limit, 50% of the allowance, resetting Monday 00:00 UTC
  - the monthly allowance itself (`go:118-122`)

  After the limit you can switch to a free model, or turn on Extra Usage to spend Console credit (`go:124-125`, `go:156-160`). Verified.
- **Privacy.** Workspace settings block endpoints that train on data, and allow only Europe and US regions, by default (`go:45-47`). Retention per model is in `go:302-344`: most open models 0 days; Beam 30 days; Muse Spark and Iceman Contributor "Not ZDR".
- **Can a script see usage left? Yes.** `GET https://opencode.ai/inference/go/v1/usage` with the Go key returns `{"usage":{"rolling"|"weekly"|"monthly":{"status":"ok"|"rate-limited","percent":0-100,"resetsAt":…}}}`, where `percent` is how much is **used** (`go:262-289`). Without a valid key it returns 401; without Go, 403 `EntitlementError` (`go:291-296`). Verified from the docs, not called: there's no Go key.

## 4. Quota visibility and how a limit shows up

- **Zen free models have no readable quota.** Verified:
  - The server limits anonymous free use **per IP per UTC day**. New IPs get double for their first period (`src:console/app/src/routes/zen/util/ipRateLimiter.ts:8-40`).
  - The numbers come from a secret, `Resource.ZEN_LIMITS` (`src:console/core/src/subscription.ts:44-51`), so they aren't public.
  - No usage endpoint for the free tier is documented (searched `content/` and `v1/content/`). `opencode stats` shows local token counts, not quota.
  - Every agent step is one request, so a long ticket uses many. How many tickets a day fit is **unverified**.
- **A limit hit returns HTTP 429** with a `retry-after` header set to the seconds until UTC midnight (`ipRateLimiter.ts:40,52-54`; `handler.ts:507-532`). The body is `{"type":"error","error":{"type":"FreeUsageLimitError","message":"Rate limit exceeded. Please try again later."}}` (`en.ts:405`). Verified from the source at `v1.19.6`. The deployed server may differ: the console code is no longer in the public `dev` tree.
- **opencode 1.19.6 then waits instead of failing.** Verified from the source; not observed:
  - `retryable()` turns a `FreeUsageLimitError` body into a retry with the message "Free usage exceeded, subscribe to Go" and `action.reason: "free_tier_limit"` (`src:opencode/src/session/retry.ts:10,99-111`).
  - The wait follows `retry-after` (`retry.ts:47-78`), for up to 5 retries (`retry.ts:31`). The session's status becomes `{type:"retry", message, next}` (`src:opencode/src/session/processor.ts:675-686`).
  - So a worker that hits the free limit can sit until midnight UTC. Users report the same: "Free usage exceeded stucked forever" (anomalyco/opencode#59338).
  - Go's limit, `GoUsageLimitError`, takes the same path, with "… usage limit reached. It will reset in …" (`retry.ts:112-143`).
- **What `opencode run` shows.** It exits `1` when the session reports an error and `0` otherwise (`src:opencode/src/cli/cmd/run.ts:827-863`).
  - `--format json` prints one JSON event per line: `tool_use`, `step_start`, `step_finish`, `text`, `reasoning`, `error` (`run.ts:668-779`). The run ends when the session goes idle (`run.ts:784`).
  - It **does not print retry status**, so a quota wait looks like silence on stdout. Verified from the source.
  - The probe's success looked like this: exit 0, empty stderr, three lines, ending with `{"type":"step_finish",…"reason":"stop","cost":0,"tokens":{"input":7687,"output":3,…}}`. Verified.
- **Are 1.x builds still served?** An opencode member's pinned issue says "v1 no longer supports free models and only gets fixes for critical bugs", and tells people to move to v2 (anomalyco/opencode#59387, updated 2026-11-25). But the probe on 1.19.6 worked today. Treat 1.x's free access as liable to stop. v2's CLI differs: the variant goes in the model id, `--model provider/model#variant` (`content/models.mdx:81-91`).

## 5. opencode as a worker CLI (1.19.6)

- **Instructions.** opencode loads the nearest `AGENTS.md` between the working folder and the worktree root. `CLAUDE.md` is read only when there's no `AGENTS.md` (`v1docs:rules.mdx:77-103`; `src:opencode/src/session/instruction.ts:59-66,121-130`). It doesn't follow `@` imports (`rules.mdx:137`). The repo's `AGENTS.md` is what gets loaded. Verified.
  - **Global rules:** `~/.config/opencode/AGENTS.md`, or else `~/.claude/CLAUDE.md`. There's no `~/.config/opencode/AGENTS.md` here (`ls`), so a worker also gets Daniel's global Claude instructions. Verified.
- **Skills.** opencode searches `.opencode/skills`, `~/.config/opencode/skills`, `.claude/skills`, `~/.claude/skills`, `.agents/skills` and `~/.agents/skills` (`v1docs:skills.mdx:16-30`). Run in the repo, `opencode debug skill` found 58, among them `~/.agents/skills/modern-web-guidance`, `~/.agents/skills/herdr` and the repo's `ship`, `release` and `web-design-guidelines`. Verified. They load through a `skill` tool, so a `$skill` mention in `AGENTS.md` depends on the model calling it. Unverified.
- **Permissions.** Most actions are allowed by default, `bash` and `edit` included. `external_directory` and `doom_loop` ask, and reading `.env` asks (`v1docs:permissions.mdx:172-191`).
  - `--auto`, on both the TUI and `run`, approves every request not explicitly denied (`permissions.mdx:22-38`). `--yolo` and `--dangerously-skip-permissions` are hidden aliases of it (`run.ts:243-275`).
  - **Without `--auto`, `opencode run` auto-rejects anything that asks** (`run.ts:791-810`).
  - You can set allow, ask or deny rules per tool and pattern in `opencode.json`, or inline through `OPENCODE_PERMISSION` (`v1docs:cli.mdx:761`).
- **No sandbox.** The shell tool runs each command with a plain child-process spawn (`src:opencode/src/tool/shell.ts:342,484`), after a permission check (`shell.ts:270,283`). The v1 docs mention "sandbox" only for a third-party Daytona plugin. Verified.
  - A worker has Daniel's full user access; #592 leaves sandboxing out of scope. AGENTS.md's credential-file rule rests on `permission` deny rules alone, and bash patterns are easy to sidestep.
  - Each command times out after 2 minutes by default (`shell.ts:347`). `OPENCODE_EXPERIMENTAL_BASH_DEFAULT_TIMEOUT_MS` changes that (`src:opencode/src/effect/runtime-flags.ts:53`); a cold `npm run check` is about 84 s.
- **Model and effort per run.** `-m opencode/<id>`, plus `--variant` for reasoning effort (`opencode run --help`). Variants differ by model (`--verbose`):
  - Beam and Flint: `low…max`
  - DeepSeek: `low|high|max`, with no `medium`
  - Hy4 flash: `none|high`
  - Nemotron Super: `none…xhigh`
  - MiMo, Big Pickle, Iceman: none

  The TUI has no `--variant` flag; it cycles variants with a key (`v1docs:models.mdx:198-200`). Verified.
- **In Herdr.** `herdr agent start --kind opencode` is supported in 0.9.5. Verified. plotkeeper has no opencode hook yet: "OpenCode and Cline are planned for v2" (`README.md:42`). A worker in a pane won't appear as a Studio session.

## 6. Fit for plotkeeper routing

**What was chosen.** Spec #592 puts two entries in `cheap`, DeepSeek first, then Beam, and keeps Gemini (agy) out of the workers: it stays for the outside review and the critic. The list is filled since #601, when `delegate.mjs run` shipped.

```json
"cheap": [
  { "cli": "opencode", "model": "opencode/deepseek-v4.1-flash-free", "variant": { "medium": null, "high": "high" } },
  { "cli": "opencode", "model": "opencode/beam-free", "variant": { "medium": "medium", "high": "high" } }
]
```

MiMo stays a later candidate, behind its own check. The spike's findings (#599) are in §7.

**Privacy caveat per model** (for Daniel to decide):

- **Beam:** prompts kept 30 days, not trained on, US maker.
- **DeepSeek, MiMo:** Chinese makers. Zen says they are hosted in the US/EU with zero retention. MiMo's "collect feedback" wording is vague.
- **Nemotron Super:** logged by NVIDIA, "do not submit … confidential data". Add only if Daniel is fine with plotkeeper's code being logged.
- **Out on data terms:** Flint, Iceman, Tidepool and Big Pickle, whose free use trains on prompts.

**`quota()`:**

- **Free Zen models:** no reader is possible (§4). Under the spec they are always eligible (#592, "`routing.json`"). `delegate.mjs` must spot the free-limit wait itself and treat it as a **quota stop**, handing the ticket on to the next candidate, not as a failure. In a Herdr pane, look for "Free usage exceeded, subscribe to Go". Through `opencode serve` and its event stream, look for `session.status` `retry` with `action.reason: "free_tier_limit"`.
- **Go:** a real reader is possible: `1 - max(percent)/100` across `rolling`, `weekly` and `monthly` (§3). It needs the Go key, which `delegate.mjs` may not take from opencode's `auth.json` (AGENTS.md, Hard rules). The key would need its own plotkeeper Keychain account. That's a new exception to the hard rules, and Daniel's call.

**What the spike should check:**

1. Whether to use 1.19.6 or v2, given #59387. Re-check the flags on v2.
2. One real cheap ticket with `beam-free`, then with `deepseek-v4.1-flash-free`, in a Herdr pane with `--auto`. Does it read `AGENTS.md`, run `npm run check` to green within the bash timeout, and commit nothing it shouldn't?
3. Requests used per ticket, and what the free-limit wait looks like in the pane.
4. How to map route effort to `--variant` when a model has no `medium` (DeepSeek).
5. What a `permission` deny set for credential paths blocks, since there's no sandbox.
6. Whether Daniel's global `~/.claude/CLAUDE.md` should be kept out (`OPENCODE_DISABLE_CLAUDE_CODE_PROMPT=1`).
7. That the missing opencode hook keeps the worker out of the Studio. `PLOTKEEPER_INTERNAL=1` can't be the way: it must not reach the worker's `npm run check` (§7).

## 7. The spike (#599, 26 November 2026)

One real ticket's work (#595, the Route check in `wayfinder/src/publish.rs`) by `opencode run --auto --format json` on 1.19.6, each worker in a fresh worktree from `origin/main`, with #592's worker env. Evidence: `local-evidence/2026-11-26-issue-599-opencode-spike/README.md` (Daniel's machine).

- **1.19.6 still serves both free models**, with the flags `run --auto --format json -m <model> [--variant <v>]`. Verified.
- **Both read `AGENTS.md` first and committed nothing.** DeepSeek's work was green under `npm run check` (1649 + 635 tests) in 14 minutes; Beam's was red only on `cargo fmt` after 84 s, and it never ran `npm run check`. Verified.
- **`PLOTKEEPER_INTERNAL=1` must not reach the check.** With it set, the hook's tests fail (`hook/src/main.rs:45` drops every event), so a worker or `delegate.mjs` that exports it gets a red check on correct work. Verified.
- **`OPENCODE_PERMISSION` denies `git push` and `gh`** before they run. Verified.
- **`OPENCODE_DISABLE_CLAUDE_CODE_PROMPT` and `OPENCODE_DISABLE_EXTERNAL_SKILLS`** keep the global `CLAUDE.md` and the external skills out (the only skill left is the built-in `customize-opencode`) while `AGENTS.md` loads. Verified.
- **`--session <id>` continues the session.** Verified.
- **The free limit was not hit**, so its output, tickets per day and whether the models share it stay unverified.
- **`agy --mode plan` writes nothing in the repository** (it saves its plan under `~/.gemini/`). `-p` takes the next word as its prompt, so the prompt must come right after `-p`. Verified.
- **Auto mode did not block** a stub at `scripts/agents/delegate.mjs run …`, nor `opencode run` started from Claude Code. Verified for this session.
