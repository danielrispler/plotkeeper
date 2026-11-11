## Studio rules

These rules apply to work under `apps/studio`, the control room tracked by map #266. The shared rules in the root `AGENTS.md` also apply; its **Engine rules** do not.

## Hard rules

- **Never open scored runs** (`"scored": true` in `result.json`), per #228. This is stricter than the engine's lifted "Scored-batch inspection" rule; the studio follows the stricter rule.
- **Never read, copy or send credential files or tokens of any CLI** (Claude Code, `gh`, Codex, `gcloud` and the like). Running a CLI as a normal user (for example `gh issue list`) is fine. Reading its stored credentials or config that may hold tokens (such as `~/.claude.json`, `~/.claude/.credentials.json`, the keychain, `~/.config/gh/hosts.yml`) is not. The one exception: the skills browser reads only the `enabledPlugins` key of Claude Code's settings files (#592).

## Implementation language

Rust only does not bind the studio. The stack is Tauri 2: a Rust shell with a TypeScript frontend ([scope decision](https://github.com/danielrispler/Falinks/issues/320#issuecomment-6465695482), "Stays as decided").

## Checks

Before handing off a studio change, run `npm ci` once, then `npm run check` from `apps/studio`. It builds the UI first, which the Tauri crate needs to compile, then runs the same checks as `.github/workflows/studio.yml`.

In a worktree, share the main checkout's cargo target dir so the build starts warm: prefix cargo and `npm run check` with `CARGO_TARGET_DIR="$(git rev-parse --path-format=absolute --git-common-dir)/../apps/studio/target"`. Parallel workers sharing it wait briefly on cargo's lock but still finish sooner than cold builds (#549).

## Context

Studio vocabulary is `docs/studio/CONTEXT.md`. Standing preferences, decisions and findings live in the Notes of map #266; read them there.
