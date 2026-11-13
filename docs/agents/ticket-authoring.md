# Issue tracker: ticket authoring

Read this file before you create, edit or resolve an issue. For lookups, claims and Wayfinder startup, read `docs/agents/issue-tracker.md`.

## Write commands

- **Create an issue**: `gh issue create --title "..." --body-file <path>`
- **Comment on an issue**: `gh issue comment <number> --body-file <path>`
- **Edit an issue body**: `gh issue edit <number> --body-file <path>`
- **Apply or remove labels**: `gh issue edit <number> --add-label "..."` / `--remove-label "..."`
- **Close**: `gh issue close <number> --comment "$(cat <path>)"`. `gh issue close` has no `--body-file`, so read the comment from the file.

Route every issue or comment body through a file. Write the body to a scratchpad file, change it with the Edit tool, and publish it from the file. Do not post a body inline or rewrite it with a script: inline bodies break on shell quoting, and scripted string replacement fails on stale text.

## When a skill says "publish to the issue tracker"

Before you publish a labelled issue, run the preflight in `docs/agents/triage-labels.md`. Then create a GitHub issue.

### Repository-specific `to-spec` confirmation

Synthesize the complete draft independently from the settled conversation and repo evidence. Before publication, confirm the proposed testing seams with one focused question unless they were already explicitly agreed in the current conversation. Show the proposed checks and their limits, and wait for the answer before publishing. This confirmation concerns testing seams; preserve the settled design rather than reopening a design interview. The installed skill remains unchanged.

## Spec and ticket constraints

- **Sourcing.** Each number, code reference and decision claim that a decision, scope or estimate depends on cites its source: a `path:line` at a named commit SHA, a comment permalink, or a command that reproduces the number. A decision claim links the resolution comment, not a bare issue number. A decision recorded in a spec body has no resolution comment; cite that spec's section by name instead (e.g. "#45, Out of Scope"). Prose reasoning and background figures that no decision rests on need no citation.
- **Session logs, retros only.** A retro may cite a session log as `<session-id> L<line>`. Logs live only on the maintainer's machine, so quote or summarise the cited content inline. Other specs cite durable sources only.
- **Nested agents.** A ticket that launches nested agents (`claude -p`, Codex) needs a session outside auto mode. Auto mode blocks these runs. Say so in the ticket, and raise it with the user before you start the work.

## Ticket context

Resolve meaningful tickets when their agreed work and verification are complete. Follow `docs/agents/learning.md` for the optional explanation suggestion; explanation and readiness confirmation are not closure prerequisites.

Every new Wayfinder ticket must include `## Reading` before linking it to the map. List relevant current decision resolutions and research artifacts by name, with one short reason per link. Use comment permalinks for specific resolutions. When no prior material applies, write `None — no prior decisions or research required.` Reading links are a starting set, not a limit on investigation; preserve the ticket's question, scope and stop condition.

Every new ticket must also include `## Touches`, so parallel planning can spot conflicts without reading the code. List two things, one line each:

- **Files:** the main files or modules it edits. This is a best-effort planning hint, not a scope limit.
- **Session:** `outside auto mode` when it launches nested agents (see **Nested agents** above), else `auto mode`.

Add or refresh `## Touches` alongside `## Reading` before starting an existing ticket.

A criterion that asks for a screenshot asks for it saved under `local-evidence/<batch>/screenshots/` and listed by path in the PR. Agents cannot attach images to a PR.

Before starting an existing ticket, add or refresh its reading section. When resolving, reopening or changing a decision, refresh reading links on affected open tickets and on tickets entering the frontier; replace superseded pointers without copying the decision detail. When a ticket depends on a repository rule, link the rule's doc section rather than restating it, so a rule change leaves the ticket current. Ticket creation and resolution are complete only after these context updates. Backfill the currently eligible tickets first; other existing tickets are updated as their prerequisites settle or before work begins.

## Wayfinder tracker operations

- **Map**: a single issue labelled `wayfinder:map`, holding the Notes / Decisions-so-far / Fog body. `gh issue create --label wayfinder:map`.
- **Child ticket**: an issue linked to the map as a GitHub sub-issue (`gh api` on the sub-issues endpoint). Where sub-issues aren't enabled, add the child to a task list in the map body and put `Part of #<map>` at the top of the child body. Labels: `wayfinder:<type>` (`research`/`prototype`/`grilling`/`task`). Once claimed, the ticket is assigned to the driving dev.
- **Blocking**: GitHub's **native issue dependencies**, the canonical, UI-visible representation. Add an edge with `gh api --method POST repos/<owner>/<repo>/issues/<child>/dependencies/blocked_by -F issue_id=<blocker-db-id>`, where `<blocker-db-id>` is the blocker's numeric **database id** (`gh api repos/<owner>/<repo>/issues/<n> --jq .id`, _not_ the `#number` or `node_id`). GitHub reports `issue_dependencies_summary.blocked_by` (open blockers only, the live gate). Where dependencies aren't available, fall back to a `Blocked by: #<n>, #<n>` line at the top of the child body. A ticket is unblocked when every blocker is closed.
- **Resolve**: `gh issue comment <n> --body-file <path>`, then `gh issue close <n>`, then append a context pointer (gist + link) to the map's Decisions-so-far.
