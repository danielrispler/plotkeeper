## Agent compatibility

These docs are shared by Codex and Claude Code; Claude Code reads them through `CLAUDE.md`, which imports this file. `$skill` means `/skill` in Claude Code (plugin skills may appear as `/mattpocock-skills:<name>`).

## Where this came from

The studio moved here from `danielrispler/Falinks`'s `apps/studio` and `docs/studio` (danielrispler/Falinks#605), with its history. A bare `#N` in this repo means this repo's issue. Issues and PRs from before the move are cited as `danielrispler/Falinks#N`; the ones that moved here redirect from their old links.

## Local evidence

Raw evidence from finished real runs (run directories, screenshots, terminal output) lives on Daniel's machine in `local-evidence/` at the root of the main checkout, `/Users/danielrispler/work/plotkeeper/local-evidence/`. It is gitignored, so clones and CI lack it, and it is never pushed: it holds Daniel's real sessions and third-party copies. Before looking for a past run's files, read its index, `local-evidence/README.md`. When a real run finishes, move its directory there and add a row to the index.

## Hard rules

- **Never open scored runs** (`"scored": true` in `result.json`), per danielrispler/Falinks#228. This is stricter than the engine's lifted "Scored-batch inspection" rule; the studio follows the stricter rule.
- **Never read, copy or send credential files or tokens of any CLI** (Claude Code, `gh`, Codex, `gcloud` and the like). Running a CLI as a normal user (for example `gh issue list`) is fine. Reading its stored credentials or config that may hold tokens (such as `~/.claude.json`, `~/.claude/.credentials.json`, the keychain, `~/.config/gh/hosts.yml`) is not. This binds agents and the studio's code alike, and covers key-scoped reads (such as `jq '.someKey'`) of those files too. The one exception: the studio's skills browser reads only the `enabledPlugins` key of Claude Code's settings files (danielrispler/Falinks#592).

## Workflow

- **Grill, spec, ticket, then build.** By default, work moves through `$grilling`, then `$to-spec`, then `$to-tickets`, and only then to implementation, of one ticket or a whole spec. `$to-spec` and `$to-tickets` are user-invoked: they load when Daniel types them. Without them, follow `docs/agents/ticket-authoring.md`.
- **Implement in a worktree.** Each change gets its own worktree under `.claude/worktrees/`, on a new branch from `origin/main`; the main checkout stays on `main`. Work in the main checkout only when Daniel says so.
- **Finish with a PR.** When an implementation is done and `npm run check` passes, push its branch and open a PR against `main`, its body written with `$pr`. Before opening it, run the correctness pass in `docs/agents/issue-tracker.md` (Claim) if the PR changes Rust or TypeScript. Then, with the work committed and before `gh pr create`, run the outside review from the worktree: `node scripts/review/outside-review.mjs --out <scratchpad>`. It decides whether Gemini reviews the diff and prints the verdict first (`docs/studio/development.md`); a review takes up to 15 minutes, so run it as a background task. On `reviewed`, check each finding in its JSON file against the code and fix the valid ones; if you fixed any, run it once more with `--since <first fix commit>` and stop after that round. The PR body gets an "Outside review" section: the model, the rounds, and each finding marked fixed or rejected with its reason. For `skipped` or `review unavailable` the section is that one line, and the PR goes ahead. An auto-mode session ran the script unblocked (2026-11-22); if auto mode blocks it, it reaches Daniel's card like any blocked command. Daniel reviews the PR; merge only when he asks, by the merge procedure in `docs/agents/issue-tracker.md` (`/ship <pr>`).
- **Blocked commands go to Daniel.** When auto mode blocks a Bash command you need, it reaches Daniel on a studio card with Allow and Decline; wait for his click, and on Allow run that exact command once yourself, with the same quoting and folder. Hand over only a command that needs his keyboard: write it into `<scratchpad>/run-<name>.sh`, end your reply with that path, and he runs `bash <path>` in the terminal panel. Other blocks (a tool other than Bash, a permission rule) keep Claude Code's own notice.

## Implementation language

The Tauri 2 stack is a settled decision ([scope decision](https://github.com/danielrispler/Falinks/issues/320#issuecomment-6465695482), "Stays as decided"); don't propose switching.

## Checks

Before handing off a change, run `npm ci` once, then `npm run check` from the repository's root; it needs `cargo nextest` (`brew install cargo-nextest`). It builds the UI first, which the Tauri crate needs to compile, then runs the checks of `.github/workflows/studio.yml`'s `macos` job. It leaves out the snap fixtures, which that workflow renders in Chrome in a job of its own (`npm run snap:all`).

The app embeds `dist/` when it is built, so a merged change reaches Daniel's open Studio only after a rebuild. After merging a change to the app, tell him to quit Studio and run `npm run start` from the main checkout.

To prove native window or key behaviour without Daniel pressing keys, read `docs/agents/app-probe.md`: a background debug build that never takes his focus.

A worktree builds in its own `target/`, cargo's default: never point `CARGO_TARGET_DIR` at the main checkout's target or at a dir another worktree has used, and that holds for `/implement-spec` implementers and mergers too. A workspace crate's build lands at the same path whichever checkout built it, and cargo trusts it while the sources are older than that build, so a shared or reused dir builds another checkout's copy of a crate: in #26's run every parallel implementer hit errors that didn't match its source, and a check can pass against code the worktree doesn't hold ([resolution](https://github.com/danielrispler/plotkeeper/issues/38#issuecomment-6955167603), which also lists the options rejected). A worktree's first check starts cold, about 84 s against 50 s warm (danielrispler/Falinks#549). A run has at most 3 implementers at once (`docs/agents/issue-tracker.md`), so give each worker's builds and tests a third of the cores: prefix them with `CARGO_BUILD_JOBS=$(( $(sysctl -n hw.ncpu) / 3 )) NEXTEST_TEST_THREADS=$(( $(sysctl -n hw.ncpu) / 3 ))` (#141).

Wait on a check through its own process: run it in the foreground with a long timeout, or start it as a background task and wait for its completion notice, so a finished worker leaves no polling loop behind. A wait that matches the check's text, such as `until ! pgrep -f "npm run check"; do sleep 5; done`, matches its own shell's command line and never ends: in spec #102's run three implementers sat an hour past their green checks that way (#141).

Never edit a frozen sample in `understanding/tests/fixtures/stored/`: a failing one is fixed in the reading code, and a changed stored type adds a new sample beside the old (its `README.md`).

A red `snap` job on a fixture the diff didn't touch gets one rerun of the failed job (`gh run rerun <id> --failed`), said in the PR; a second failure is real. `snap:all` has no automatic retry. A failing `snap` prints the tail of Chrome's own stderr.

Rust is formatted by `cargo fmt`. TS, JS and Markdown have no formatter, so never run one (Prettier, Biome, …).

## The system map

A change that adds, moves or removes files, changes what a part does or how parts connect, or changes a law, changes `system-map.json` in the same PR. `npm run check` runs `system-map check` and fails on what it can see; `docs/studio/behaviour/map.md` says how to edit the map and tag a test with `@law: <id>`.

## Agent skills

### Learning

Before planning, grilling or explanations, read `docs/agents/learning.md`. During session openings, planning, grilling, decision questions and explanations, it overrides any terse or brevity output mode. Follow its state/goal brief and explanation guidance during grilling, Wayfinder, and ordinary planning. After a meaningful ticket that merges a diff, suggest `$dsh-explain-diff` in the final message. Explanations are optional and never block ticket resolution or progress; run the skill only when explicitly invoked.

### Issue tracker

Track issues and specs in GitHub Issues for `danielrispler/plotkeeper`. To read, list or claim issues, read `docs/agents/issue-tracker.md`. To create, edit or resolve issues, read `docs/agents/ticket-authoring.md`. Label every issue `studio`.

### Wayfinder

For Wayfinder sessions, run `cargo run --bin wayfinder-startup -- [map-number]`. The default is the newest open `wayfinder:map` issue, the control-room map. Claim a ticket with `--claim <n>`. Follow the startup steps in `docs/agents/issue-tracker.md` and the ticket-context rules in `docs/agents/ticket-authoring.md`.

### Triage labels

Use the five default triage labels. Before triaging issues, read `docs/agents/triage-labels.md`.

### Domain docs

Studio vocabulary is `docs/studio/CONTEXT.md`. Before exploring the codebase, read `docs/agents/domain.md`. Standing preferences, decisions and findings live in the Notes of the control-room map; settled decisions are indexed in its Decisions-so-far or a spec's Implementation Decisions. Read the linked resolution comment before proposing a design change.
