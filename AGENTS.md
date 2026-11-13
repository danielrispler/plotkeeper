## Agent compatibility

These docs are shared by Codex and Claude Code. `$skill` means `/skill` in Claude Code (plugin skills may appear as `/mattpocock-skills:<name>`). During session openings, planning, grilling, decision questions and explanations, `docs/agents/learning.md` overrides any terse or brevity output mode.

## Where this came from

The studio moved here from `danielrispler/Falinks`'s `apps/studio` and `docs/studio` (danielrispler/Falinks#605), with its history. A bare `#N` in this repo means this repo's issue. Issues and PRs from before the move are cited as `danielrispler/Falinks#N`; the ones that moved here redirect from their old links.

## Local evidence

Raw evidence from finished real runs (run directories, screenshots, terminal output) lives on Daniel's machine in `local-evidence/` at the root of the main checkout, `/Users/danielrispler/work/plotkeeper/local-evidence/`. It is gitignored, so clones and CI lack it, and it is never pushed: it holds Daniel's real sessions and third-party copies. Before looking for a past run's files, read its index, `local-evidence/README.md`. When a real run finishes, move its directory there and add a row to the index.

## Hard rules

- **Never open scored runs** (`"scored": true` in `result.json`), per danielrispler/Falinks#228. This is stricter than the engine's lifted "Scored-batch inspection" rule; the studio follows the stricter rule.
- **Never read, copy or send credential files or tokens of any CLI** (Claude Code, `gh`, Codex, `gcloud` and the like). Running a CLI as a normal user (for example `gh issue list`) is fine. Reading its stored credentials or config that may hold tokens (such as `~/.claude.json`, `~/.claude/.credentials.json`, the keychain, `~/.config/gh/hosts.yml`) is not. The one exception: the skills browser reads only the `enabledPlugins` key of Claude Code's settings files (danielrispler/Falinks#592).

## Implementation language

The stack is Tauri 2: a Rust shell with a TypeScript frontend ([scope decision](https://github.com/danielrispler/Falinks/issues/320#issuecomment-6465695482), "Stays as decided"). The toolchain is pinned in `rust-toolchain.toml`.

## Checks

Before handing off a change, run `npm ci` once, then `npm run check` from the repository's root. It builds the UI first, which the Tauri crate needs to compile, then runs the same checks as `.github/workflows/studio.yml`.

In a worktree, share the main checkout's cargo target dir so the build starts warm: prefix cargo and `npm run check` with `CARGO_TARGET_DIR="$(git rev-parse --path-format=absolute --git-common-dir)/../target"`. Parallel workers sharing it wait briefly on cargo's lock but still finish sooner than cold builds (danielrispler/Falinks#549).

## Agent skills

### Learning

Before planning, grilling or explanations, read `docs/agents/learning.md`. Follow its state/goal brief and explanation guidance during grilling, Wayfinder, and ordinary planning. After meaningful tickets, suggest `$dsh-explain-diff` in the final message. Explanations are optional and never block ticket resolution or progress; run the skill only when explicitly invoked.

### Issue tracker

Track issues and specs in GitHub Issues for `danielrispler/plotkeeper`. To read, list or claim issues, read `docs/agents/issue-tracker.md`. To create, edit or resolve issues, read `docs/agents/ticket-authoring.md`. Label every issue `studio`.

### Wayfinder

For Wayfinder sessions, run `cargo run --bin wayfinder-startup -- [map-number]`. The default is the newest open `wayfinder:map` issue, the control-room map. Claim a ticket with `--claim <n>`. Follow the startup steps in `docs/agents/issue-tracker.md` and the ticket-context rules in `docs/agents/ticket-authoring.md`.

### Triage labels

Use the five default triage labels. Before triaging issues, read `docs/agents/triage-labels.md`.

### Domain docs

Studio vocabulary is `docs/studio/CONTEXT.md`. Before exploring the codebase, read `docs/agents/domain.md`. Standing preferences, decisions and findings live in the Notes of the control-room map; settled decisions are indexed in its Decisions-so-far or a spec's Implementation Decisions. Read the linked resolution comment before proposing a design change.
