## Studio rules

These rules apply to work under `apps/studio`, the control room tracked by map #266. The shared rules in the root `AGENTS.md` also apply; its **Engine rules** do not.

## Hard rules

- **Never open scored runs** (`"scored": true` in `result.json`), per #228. This is stricter than the engine's lifted "Scored-batch inspection" rule; the studio follows the stricter rule.
- **Never read, copy or send credential files or tokens of any CLI** (Claude Code, `gh`, Codex, `gcloud` and the like). Running a CLI as a normal user (for example `gh issue list`) is fine. Reading its stored credentials or config that may hold tokens (such as `~/.claude.json`, `~/.claude/.credentials.json`, the keychain, `~/.config/gh/hosts.yml`) is not.

## Implementation language

Rust only does not bind the studio: a TypeScript frontend is allowed. The stack follows the form-factor decision on map #266.

## Context

Studio vocabulary is `docs/studio/CONTEXT.md`. Standing preferences, decisions and findings live in the Notes of map #266; read them there.
