---
name: release
description: Prepare the next npm release of plotkeeper as a Release PR, from what merged since the last tag. Runs only when Daniel types it.
disable-model-invocation: true
---

Daniel typed this, so he wants the next release prepared (spec #512; `docs/studio/development.md`, "Cutting a release"). You end at an open Release PR. Daniel merges it (through `/ship`) and starts the publish himself; you never publish, tag, or dispatch a workflow.

## Steps

1. **Since the tag.** `git fetch origin --tags`. The last release is the newest `v*` tag by version (`git tag --list 'v*' --sort=-v:refname | head -1`). When it prints nothing, no release is tagged: say "no `v*` tag on origin, so there is no release to start from (the tags are backfilled by #516)", and stop. List every commit since it: `git log --first-parent --format='%h %s' <tag>..origin/main`. PRs are squash-merged, so a subject ending in `(#N)` is PR N: read each one's title and body (`gh pr view N --json title,body,files`). A commit with no PR number is read from its own diff. Done when every commit in the list has a verdict: a draft note line, or "none" with its reason.
2. **Notes**, by the notes rule below. When no line remains, say "nothing to release since `<tag>`" with the verdicts, and stop.
3. **Level**, by the level rule below: `patch` or `minor`, with the reason (the line that decides it).
4. **Propose** the level, the version it makes, and the draft section in chat, and wait. Daniel may revise either; carry on only when he agrees, with what he agreed to.
5. **Worktree.** Create a worktree under `.claude/worktrees/` on a new branch `release-vX.Y.Z`, the agreed version, from `origin/main` (`git worktree add -b release-vX.Y.Z .claude/worktrees/release-vX.Y.Z origin/main`), and do the rest there.
6. **Bump.** `node scripts/npm/release.mjs bump <patch|minor>`. It rewrites `npm/package.json`, `server/Cargo.toml` and `Cargo.lock` and prints the new version, which must be the agreed one. The script owns the version: leave those three files as it wrote them.
7. **Changelog.** Insert the agreed section into `CHANGELOG.md` as `## X.Y.Z`, above the newest version's section and below the intro.
8. **README.** Read the root `README.md` against the release's changes, and list each sentence a change made wrong, quoted, with the PR that made it wrong, or one line "README: no sentence contradicted". This goes into the PR body; leave the README unedited, since its wording is Daniel's.
9. **Check.** `npm ci`, then `npm run check` from the worktree root, until green. Its node test fails when `CHANGELOG.md` lacks the new version's section.
10. **PR.** Commit as `Release vX.Y.Z`, push, and run the outside review as `AGENTS.md` says; a Release PR's diff gets `skipped: no risk match`, which is the PR's "Outside review" line. No correctness pass is needed: the diff has no Rust or TypeScript. Open the PR titled `Release vX.Y.Z` against `main`, its body written with `$pr`, holding the notes and the README list.
11. **Hand-off.** Your last message gives the PR link and, for after Daniel has merged it:

        gh workflow run release.yml --ref main -f publish=true

    The publish must run on the Release PR's own commit: when anything merges to `main` after it, the release commit guard fails the run ("main moved past the Release PR, run /release again").

## Notes rule

- A change gets a line only when a user of the npm package would notice it: the page, the `plotkeeper` CLI and its flags, `plotkeeper install`'s output and what it wires, the data folder, the package itself (its README and LICENSE on npm count). Agent docs, skills, CI and workflows, tests, snap fixtures, refactors, the system map and the development docs get none.
- Group lines under Keep a Changelog headings, in this order and only those used: `Breaking`, `Added`, `Changed`, `Fixed`.
- Each line is one sentence, in the user's words, saying what changed and why it matters to them, ending with `(#N)`, the PR number. PRs that make one change share one line, ending with all their numbers (`(#468, #470)`).
- Match the tone of the sections already in `CHANGELOG.md`.

## Level rule

- `minor` when any line is a new user-visible feature or a breaking change: a CLI flag removed or changed, `plotkeeper install`'s output changed, the data folder's format changed. A breaking change also gets its line under `Breaking`.
- `patch` when every line is a fix or polish.
- The bump is always `patch` or `minor`, so the version stays below 1.0; `major` and 1.0.0 are Daniel's decision, and the script refuses `major`.
