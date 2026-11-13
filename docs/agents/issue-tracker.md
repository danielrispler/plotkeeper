# Issue tracker: query and claim

Issues and specs for `danielrispler/plotkeeper` live as GitHub issues. Use the `gh` CLI for all operations. `gh` infers the repo from `git remote -v` when run inside a clone.

This file covers lookups, claims and Wayfinder startup. To create, edit or resolve tickets, read `docs/agents/ticket-authoring.md`.

## Read and list

- **Read an issue with its comments**: `gh issue view <number> --json title,url,body,labels,assignees,comments`. Drop `comments` when you need only the body.
- **Read a PR with its comments and reviews**: `gh pr view <number> --json title,url,body,comments,reviews`. Use `gh pr diff <number>` for the diff.
- **Read one linked resolution**: `gh api repos/danielrispler/plotkeeper/issues/comments/<comment-id> --jq .body`.
- **List issues**: `gh issue list --state open --limit 100 --json number,title,url,labels,assignees`, with `--label` and `--state` filters. Load bodies and comments on demand.

GitHub shares one number space across issues and PRs, so a bare `#42` may be either. Try `gh pr view 42`, then `gh issue view 42`. A reference written `danielrispler/Falinks#N` is from before the move (`AGENTS.md`, "Where this came from"); read it with `--repo danielrispler/Falinks`.

## When a skill says "fetch the relevant ticket"

Use **Read an issue** above. Keep `comments` when the task depends on them.

Read the relevant linked decision resolutions before loading full issue histories. This also applies when you synthesize a completed map. Expand into issue bodies or comments only to fill a known gap or to check for supersession. Follow replacement resolution links when present.

## Wayfinder startup

Used by `/wayfinder`. The **map** is one issue. Its **child** issues are the tickets.

1. Run `cargo run --bin wayfinder-startup -- [map-number]`.
   - Without a number it uses the newest open issue labelled `wayfinder:map`.
   - With no open map it exits non-zero and lists recent maps. It never falls back to a closed map.
   - An explicit closed or missing map is an error.
   - The studio tickets (the control-room effort, titled `Studio:` and labelled `studio`) are children of map MAP, the newest open map, so no number is needed.
   - It reads the map once, then the open children in map order. Each row shows the assignees, the open blocker numbers (`blocked_by`) and `frontier`. This repo uses native GitHub sub-issues and dependencies; missing blocker metadata is an error.
   - It prints the map title and URL, then the rows. Add `--body` to append the map body, which holds Decisions-so-far and the implementation progress log.
2. Use the named ticket, or the first row with `frontier: true`. Claim it before any work (see **Claim** below). Then work in the worktree path it prints.
3. Load that ticket's body, its reading links and its required skills once. Expand to other decisions or evidence only when the question needs them. Check for superseded decisions. Read triage guidance only when triaging.
4. Open the session with the state and goal brief in `docs/agents/learning.md`. Follow its explanation guidance when planning or asking decision questions.

## Frontier

`cargo run --bin wayfinder-startup -- <map-number>` lists the frontier. A row has `frontier: true` when the ticket is open, unassigned and has no open blockers. The first such row in map order wins. The listing run only reads GitHub. For repo-wide planning (what can run now or in parallel, across every spec), run `cargo run --bin wayfinder-startup -- --all`. It makes one GraphQL query and lists every open issue by number, with its `parent`, `assignees`, open `blocked_by` and `frontier`. A row that is another row's `parent` is a spec, not a ticket. Where native relationships are not available, apply the task-list and `Blocked by` conventions in `docs/agents/ticket-authoring.md` by hand.

## Claim

`cargo run --bin wayfinder-startup -- --claim <n>` is the session's first write.

- It refuses closed tickets, blocked tickets and tickets assigned to someone else.
- It assigns the issue to you.
- It reuses the ticket's worktree, or checks out its existing local or `origin` `issue-<n>-*` branch, or creates branch `issue-<n>-<slug>` from `origin/main`. New worktrees go under `.claude/worktrees/`.
- It prints the worktree path.
- After the PR merges, `--done <n>` run from the main checkout removes that worktree and branch; `--help` lists what it refuses.

Daniel picks how parallel tickets run: separate sessions he starts (one `--claim <n>` command and one prompt per terminal), or `/implement-spec` over tickets whose Touches say `Session: auto mode`. A ticket outside auto mode, or one that stops for Daniel's sign-off, runs in its own session. On an integration branch, implementers do not run `freeze`; it runs once after the last merge. Merge `origin/main` into the integration branch at the start of each pass, so it carries `main`'s fixes. Name it `issue-<spec>-<slug>`: pushes to `issue-*` branches run the studio's CI. Never spawn background implementers unasked.

GitHub stays authoritative.

## Pull requests as a triage surface

**PRs as a request surface: no.** _(Set to `yes` if this repo treats external PRs as feature requests; `/triage` reads this flag.)_

When set to `yes`, PRs run through the same labels and states as issues, using the `gh pr` equivalents:

- **Read a PR**: the PR command in **Read and list** above.
- **List external PRs for triage**: `gh pr list --state open --json number,title,body,labels,author,authorAssociation,comments`. Keep only `authorAssociation` of `CONTRIBUTOR`, `FIRST_TIME_CONTRIBUTOR` or `NONE`. Drop `OWNER`, `MEMBER` and `COLLABORATOR`.
- **Comment, label, close**: `gh pr comment`, `gh pr edit --add-label` / `--remove-label`, `gh pr close`.
