# Grilling: delivering the understanding layer beyond the desktop app

The decisions of the grilling of 2026-11-22, round by round, with the facts found between rounds. Research: `delivery-surfaces.md` and `cross-harness-capture.md` in this folder; fact reports: `delivery-harness-facts.md`, `delivery-writer-facts.md`, `delivery-web-ui-coupling.md`.

## Settled before round 1 (Daniel, 2026-11-22)
- Developers won't switch IDEs; the product adds a capability on top of what they use.
- Capture connects to the agent harness, not the editor.
- v1 harnesses: Antigravity (agy), Codex, Claude Code, plus the 5 most popular others: 8 in all.
- Display must work for any IDE or terminal; herdr was only an example.

## Facts found
- Cards, summaries and briefs are written by `claude -p` on Daniel's subscription (`understanding/src/lib.rs:757`, `:783`). A user without Claude Code gets no cards today.
- Only `claude` and `agy` deserialize as agents (`core/src/event.rs:240-245`); `TranscriptFormat` has two variants (`:249-254`).
- Harness facts for the 5 picks, the writer and the install path: `delivery-harness-facts.md` (2026-11-22). Key points: Gemini CLI lost consumer logins on 2026-06-18 (moved to Antigravity CLI); Copilot is 67.9% of SO 2025 agent users and reads `.claude/settings.json`; OpenCode 228k stars, `opencode run` works on ChatGPT and Copilot logins; Cursor, Copilot, Droid have plugin marketplaces; Kiro, Amp, Droid, Gemini (consumer) headless modes need an API key.

## Round 1 (2026-11-22): Daniel's answers
- **Q1 (changed from the recommendation):** freeze and archive the Tauri desktop app, don't delete it; mark it unmaintained/legacy so it can be revisited. No second front end. Daniel works in a full IDE and dogfoods the same standalone experience other developers get. Challenge: what complexity drops once Tauri isn't supported beside the web page?
- **Q2:** a local web page is the v1 base. No TUI companion. Challenge: verify how coupled `ui/` is to Tauri and what serving it from a local server takes, before the spec.
- **Q3:** exactly the 8 harnesses. Unsupported ones get no cards and no git-only fallback; say plainly they are unsupported.
- **Q4:** cards are written on the user's own harness or subscription, not Claude only. Challenge: one minimal writer abstraction, not 8 bespoke pipelines; flag which harnesses have reliable headless one-shot modes with structured output and which would be flaky.
- **Q5:** hooks plus session files only; no LLM proxy, no OpenTelemetry in v1.
- Standing rule (Daniel): when the agent disagrees during the grilling, it stops and says so.

## Facts found after round 1
- `ui/` touches Tauri at one seam: `window.__TAURI__` in `ui/dom.ts:8-44` (`invoke`, `listen`, `log`). No file imports `@tauri-apps`. About 75 command names and 14 event names go through it.
- The snap fixtures already run `ui/` in headless Chrome against a stub `window.__TAURI__` (`scripts/snap/page.mjs:1-7`).
- `src-tauri/src/` is 4,456 lines: `main.rs`, `terminal.rs`, `answer.rs`, `presence.rs`, `restart.rs`, `settings.rs`, `skills.rs`.
- AGENTS.md's workflow sends auto-mode-blocked commands to Daniel on a studio card with Allow and Decline; freezing the app removes that path.
- `delivery-web-ui-coupling.md` (2026-11-22): ~80 commands in `Commands` (`ui/types.ts:449-531`; I counted 80 single-line entries, the report says 82), 12 events. Tauri leaks into `ui/` at `ui/dom.ts:8-44`, `ui/terminal.ts:248` (drag-drop), `ui/index.html:21`. ~30 commands are learning layer, 11 session view, ~40 workbench. The hook appends to `events.jsonl` in the data dir with no app running (`hook/src/lib.rs:29`, `hook/src/main.rs:15-19`, verified); the app replays it on start. Only the Tauri process writes cards, runs `claude -p`, the `prs` thread and retests. Hardest parts: pulling `follow`/`Sent`/`ShellEffects` off `AppHandle`; who starts and owns a server with no app; "seen" and notification rules that use window focus.

## Q1b (2026-11-22): settled
- Soft freeze of `src-tauri/` now: no new work there, and Tauri-only workbench behaviour is not preserved while decoupling.
- Hard archive of `src-tauri/` and the AGENTS.md update as soon as the local web page shows cards, quizzes and the map (Daniel aims for 2026-11-23).
- The blocked-command card is dropped from v1; blocked commands fall back to Claude Code's own prompt.
- `delivery-writer-facts.md` (2026-11-22): claude, codex, agy run here with JSON schema output (logged-out cases used an empty config dir or HOME, not Daniel's login). Ratings: Claude reliable; Codex reliable to workable (read-only sandbox, no tool-off); agy workable (no tool-off flag; 60 s hang when logged out); Copilot, Cursor, Qwen workable; OpenCode, Cline, Devin painful. Proposed contract: system, user, optional schema, model tier, effort, timeout, returning text, JSON and usage or a typed error; per harness only a command template, an extractor, a failure classifier and a tier-to-model map.

## Round 2 (asked 2026-11-22)
Q6 the 5 harnesses; Q7 writer independent of the captured harness; Q8 server lifecycle; Q9 card-ready nudge; Q10 "seen" rule; Q11 installer; Q12 platform; Q13 distribution; Q14 local server security.

## Round 2 (2026-11-22): Daniel's answers
- Timeline: no new workbench work in `src-tauri/`; the learning/map redesign continues while the server/UI decoupling runs in parallel; archive `src-tauri/` at web parity.
- **Q6 (changed):** drop Gemini CLI (consumer logins moved to agy). Replace it with the cleaner of Windsurf or Qwen Code (agent's call, round 3). Two waves: wave 1 = Claude Code, Codex, agy, Copilot, Cursor + the replacement; wave 2 = OpenCode, Cline.
- **Q7 (changed):** capture and writing stay separate behind one contract; OpenCode and Cline are not writers. Challenges: (1) an onboarding/settings UI that detects available writers (and API keys) and lets the user pick writer and model; (2) include BYOK API key in v1, since a direct HTTP call with structured output is more reliable than wrapping CLIs; (3) consider leaning on a tier-routing model instead of per-CLI hacks.
- **Q8 (changed): (a) manual start.** `plotkeeper` is started by the user; no spawned background daemon. The hook keeps appending to `events.jsonl`; a later start replays and catches up.
- **Q9:** browser notifications plus the tab-title count only.
- **Q10 (amended):** "seen" requires the tab visible AND focused or interacted with (`document.hasFocus()` or user activity), because a browser window on a second screen reports visible while the user works in the IDE.
- **Q11:** user-level hooks only; `plotkeeper install` and `uninstall` simple and fully reversible.
- **Q12-Q14:** macOS only; npm with prebuilt macOS binaries; 127.0.0.1 + random token in a cookie + Host check, and `plotkeeper` opens the browser on the token URL itself.

## Round 3 (2026-11-22): Daniel's answers
- **Q6:** Qwen Code is wave 1's sixth harness; Windsurf is deferred to a later wave after its desktop payload is tested.
- **Q15 (tightened):** fast, deterministic replay on start; auto-write cards only for turns of the last 1-2 hours (a setting) AND at most 10 turns; everything older is written on open. Check: earlier cards are never deleted or overwritten by replay.
- **Q16 (plus):** Anthropic + OpenAI-compatible HTTP writers, key pasted in settings, kept in the macOS Keychain; no reading other CLIs' configs or env vars. The OpenAI-compatible writer has an optional Base URL, and its key is optional (local Ollama).
- **Q17:** writers detected by `command -v` plus "add an API key"; no automatic login checks; the user picks writer and model and presses Test once before cards are written.
- **Q18:** one model and effort per writer; no routing by task.
- **Q19 (plus):** v1 page = Home, session list, turns with headlines, summaries, cards and quizzes, and the map; workbench dropped. Guard: a session killed without a `Stop` hook must not stay `running` forever.
- **Q20 (changed, agent conceded):** use `~/Library/Application Support/plotkeeper/` from v1, migrating Daniel's data once now, while he is the only user.

## Facts found after round 3
- Cards and the other stores are JSON files beside the event log, written by temp-file-then-rename (`understanding/src/store.rs:1346-1351`); a store file that doesn't parse is an error and is never overwritten (`store.rs` `read`, ~line 523). Whether a replay can rewrite an existing card is not checked.
- Today a closed terminal makes the app write a synthetic `SessionEnd` line (`src-tauri/src/main.rs:782-802`): this is how killed sessions end now, and it leaves with Tauri. A session's leader pid is kept (`core/src/lib.rs:417-419`); which harnesses report it is not checked.

## Round 4 (2026-11-22): Daniel's answers
- **Q21:** process-alive check about every 30 s writes a synthetic `SessionEnd`; with no pid, "idle?" after 2 hours without events, revived by the next event. Check while writing the spec: for IDE-hosted harnesses (Cursor, Copilot in VS Code), whether the reported pid is the IDE's; if so, those use the "idle?" path.
- **Q22:** law in `system-map.json`: replay never rewrites, replaces or deletes a stored card, summary or quiz answer; a double-replay test tagged `@law: <id>`.
- **Q23:** two specs. Spec 1: standalone plotkeeper for Daniel's daily use. Spec 2: other developers (remaining harnesses, API-key writers, installer, npm). Daniel moves his daily work to herdr. Spec 1 must cover: (1) his Claude and agy hooks working when launched from herdr, with no Tauri-injected env, before `plotkeeper install` exists; (2) the one-time move of his data from `dev.falinks.studio` to `plotkeeper`.

## Facts found after round 4 (bear on spec 1)
- The hook drops every agy event outside a studio terminal: no `STUDIO_TERMINAL_ID` plus a camelCase `hookEventName` returns early (`hook/src/main.rs:26-31`, #43 decision 9). Under herdr, agy sessions would record nothing until this filter goes.
- Claude Code loads the studio plugin through `CLAUDE_CODE_PLUGIN_DIRS`, which the app's "Fix it" writes into `~/.zshrc`, pointing at the data folder's `checkout` link (`src-tauri/src/terminal.rs:331-399`). Herdr shells read `~/.zshrc`, so Claude sessions may already record there, but the data-folder move must re-point that link.

## Grilling status
Frontier empty after round 4; awaiting Daniel's confirmation of shared understanding before `/to-spec`.
