# The system map

`system-map.json` at the repository's root describes the system in plain words: its parts and the files each owns, the outside actors it talks to, the links between them, the files that belong to no part on purpose, and the laws that must stay true. `system-map check` holds the file to the repository, and `npm run check`, and so CI, runs it. The command and the library are the `system-map/` crate (`studio-system-map`); it assumes nothing about plotkeeper.

## The file

JSON, one object. A field the checker does not know fails the check.

- `version`: `1`. Any other value fails the check, and `touched` too, with a problem that names the value and says this checker reads version 1.
- `extractors`: optional, the names of the extractors to run (below). A map that names none gets no computed edges.
- `parts`: each has `id`, `name`, `caption` (a few words), `description` (plain language), `tier` (`core`, `interface` or `black-box`: the part's default; Daniel's own choice is outside the repository), `paths` (the ownership patterns) and an optional `zone`.
- `actors`: each has `id` and `name`. An outside actor owns no files.
- `links`: each has `id`, `from` and `to`, a part or an actor at each end.
- `ignored`: path patterns that belong to no part on purpose.
- `laws`: each has `id`, `on` (the id of a part, a link or a flow), `backing` (`check` or `trust`) and `text`, one plain sentence.
- `flows`: optional. Each has `id`, `name` and `hops`, each hop a `link` id and one plain sentence `text`. A hop may run its link backwards; the sentence says which way.

Ids are lowercase words joined by hyphens, unique across parts, actors, links, laws and flows. An id never changes when a name does. The checker only reads the file; edit it with ordinary tools.

## Who owns a file

"The files" are what git tracks; an untracked or gitignored file is never counted. A pattern is relative to the repository's root: one ending in `/` owns everything under that folder, `*` matches within one path segment, `**` matches across segments, and anything else names one file. Among parts the last pattern that matches a file wins, in the file's order. A file an `ignored` pattern matches belongs to no part, whatever the parts say.

## Laws and tags

A law with `backing: "check"` has a test that stands behind it; one with `"trust"` has none. A test is tied to its law by the text `@law: <id>` in a comment of its own line in the test's source, such as `// @law: delete-keeps-live-terminal` above the test. One test may carry several tags and one law may have several tests. Every tracked file is searched, ignored paths included. A tag proves a comment exists, not that the test passes.

## What the check does

`system-map check [--json] [--root <dir>]` prints one line per problem and exits non-zero when there is one; the root is the git top level of the current folder unless given. It fails, naming the file, pattern, id or law and what to change, when:

- the file is missing, is not JSON or does not match the shape above (an unknown field, a duplicate id, a link end, a law's `on` or a hop that names nothing, a tier outside the three);
- a tracked file is owned by no part and matched by no `ignored` pattern;
- a pattern of a part or of `ignored` matches no tracked file;
- a law says `check` and no tag names it, or says `trust` and a tag names it, or a tag names a law the file does not have.

It also prints, without failing, a `note:` for each computed edge between two parts that no link joins, in either direction. A computed edge is a hint: a link that no extractor can see (the window and the shell talk at run time) is still a link. `--json` prints the parts with their files, the links, each law with its tags (file and line), the unlinked edges and the problems.

## The extractors

An extractor reads which tracked file depends on which; the checker turns each pair into an edge between the parts that own the two files and drops pairs inside one part or touching an ignored file.

- `cargo`: the workspace's crates and the workspace crates each depends on, from `cargo metadata --no-deps --offline`. Dev-dependencies are left out.
- `rust-modules`: inside a crate, `mod` declarations and `use crate::` paths resolved to the files they name.
- `typescript-imports`: relative import specifiers resolved to tracked files. A specifier that resolves to no tracked file is skipped; a literal `import('./x')` is read, but a computed specifier or a path alias is not seen.

The checker runs only `git` and, when the map names `cargo`, `cargo`; it reads no file outside the repository, so it never reads through a tracked symlink (the link still counts as a tracked file for ownership), and needs no network.

## Which parts a change touches

`system-map touched [--json] [--root <dir>] [path...]` names the parts that own the paths, with the ignored paths and the unowned ones listed apart. It takes paths as arguments or one per line on standard input, so `git diff --name-only | system-map touched` works. It matches by pattern alone, so a deleted or moved path is still placed. A map that fails the shape check gets no answer: it fails with every problem `check` would report, one per line on standard error, and exit status 2.

## plotkeeper's map

Eight parts: the agent plugins, the hook, the core, the shell (`src-tauri/`), understanding, the window (`ui/`), the terminal daemon and workbench; three actors: the agents, the writer and GitHub; one flow, Turn writing. `ignored` holds what is not the running system: the docs, the spikes, `scripts/`, the tooling crates (`wayfinder/`, `test-support/`, `system-map/`) and the repository's own configuration at the root. A change that adds, moves or removes files, changes what a part does or how parts connect, or changes a law changes the map in the same PR (`AGENTS.md`).

## The Map view

The window's Map view (`ui/map.ts`, `src-tauri/src/map.rs`) draws a project's map on a quiet chart: parts as plates (name, caption, a mark per law: filled when a check backs it, hollow when it rests on trust), actors as labels, links from rim to rim. It takes the story rail's and the terminal's columns and leaves the session list and the top bar live; `studio::ShowMap` (shift-cmd-m by default) opens it on the selected session's project, else the first project with a map, and the same key leaves it.

- **Source.** The shell reads `system-map.json` with `git show` from `origin`'s default branch as last fetched, never the working tree. With `origin/HEAD` unset it reads the local `main` or `master` and says so quietly. It fetches `origin` when the view opens and when the window gains focus while it is open, and when a task is first seen merged; never on a timer. A repository is fetched at most once a minute, and never twice at once.
- **Daniel's own state** (`map-state.json` in the data folder, never the repository): a saved `(x, y)` per part or actor id and a tier per part id, which beats the map's.
- **Placement.** A part with no saved position is placed once, near the parts it links to (else the parts of its `zone`), clear of every plate, then saved; a placed part never moves on its own. A plate drags past 6 px of pointer movement, so a click never nudges it, and its place is saved when it is let go.
- **Esc** peels the layers, in order: a drawer, a scrubbed commit (back to now), a part's sheet, then the view.
- **Ready for review** (`src-tauri/src/prs.rs`, `ui/map-tray.ts`). The shell asks `gh pr view <branch> --json number,title,isDraft,state,headRefOid,body,closingIssuesReferences` for each listed session's branch: at once when a turn ends, and when the window gains focus or the Map view opens through one cooldown of a minute per repository with an in-flight guard; never on a timer. A PR is ready when it is open and not a draft; its head is kept in memory and replaced by a later lookup. The tray at the map's foot lists the project's ready PRs (title, number), each saying its brief is not written yet. Any PR number found still names its session ("PR #N").
- **No map.** A project without the file shows a calm empty state, and "Draft a map" opens New session in that project with a drafting prompt in its quick start.
- **Fog** (`src-tauri/src/fog.rs`). The first time the view opens a project, the shell records the default branch's tip, so every part starts clear. After that it walks the new first-parent commits and files each part's changes in `map-state.json`: **thick** when a law on the part, a link at it, a flow through it or its `paths` changed, or the part was added; **thin** when files it owns changed (placed by the union of the commit's map and its parent's, so a moved file touches both parts) or only its name, caption or description did; nothing for `zone`, `ignored` or `tier`. A removed part leaves the live map. What shows is by Daniel's effective tier: a Black box part never, an Interface-only part only thick. Strength is never a count.
- **Mist.** A fogged plate has a cool-grey mist behind its text, so the name stays at full strength; thick is denser, with a rim and a slow drift. "I know this" on the part's sheet clears that part's fog and lifts the mist (no motion under reduced motion), and is logged as its own event (Stop rules).
- **The part sheet** (`ui/map-sheet.ts`) opens beside the map on a click that was not a drag: the plain description, the tier (Core, Interface-only, Black box; changed in place and kept in `map-state.json`), the laws (backed by a check or on trust), the fog status and "I know this". The first Esc closes it; the next leaves the view.
- **Home's mark.** A project's row on Home carries one quiet dot, with no count, while a part whose effective tier is Core is fogged or a law is broken (below); pressing it opens that project's Map view. The shell reads it from local refs when Home is entered and when the window gains focus.
- **Live agents.** A session in `Running` or `Needs you` and each of its running Workers show as a chip on every part the current turn touched: the changed files of the turn's trees (a Worker's are the ones its name is on) plus an Edit or Write in flight, placed by `studio-system-map` through `map_place`. A `Needs you` chip keeps the loud `--needs-you` pill. A session with no changed files yet, an agent that reports no turn trees, a status-only one (Codex) or one whose edits are all ignored or owned by no part shows in a quiet **not placed** list beside the map. A Helper, a plain shell and a session in `Your turn`, `Idle` or `Ended` are never on the map. Pressing any chip selects its session or Worker and leaves the view. `hideAgents(true)` (`ui/map-agents.ts`) clears every chip and the list, for a view of a past commit.
- **Broken laws** (`src-tauri/src/ci_laws.rs`). The shell asks `gh` for the latest finished run of `studio.yml` on the default branch when the view opens, when the window gains focus while it is open and when a task is first seen merged, never on a timer, and keeps the answer in memory. While a newer run is going it keeps the last finished one and says so ("for an earlier commit"). When that run failed it fetches the failed log and reads the failing tests of `cargo nextest` (`FAIL [..] binary name`) and of Node's test runner (TAP `not ok`, or the spec reporter's `✖`). A law's test is the function right after its `@law: <id>` tag, read from the run's commit; only a failure of that test marks the owning part: a red dot and rim on its plate, and one line, "A test that backs laws here failed on main", in its sheet and in Home's dot; the law is never named. Another test failing in the same file marks nothing. A missing mark means only "no known failure": a run that failed before the tests ran (fmt, clippy, build) or in untagged tests says "the law tests did not run", one whose log cannot be fetched or holds nothing says so, and a passing run says "passed"; none of these marks a part. Home's dot reads the answer in memory, so a project shows it once its Map view has been opened since Studio started. The dot counts as seen (below) when it is in view on Home.
- **The drawer** (`ui/map-drawer.ts`) opens read-only under the part sheet and shows blocks of code or text with jumps out of it. "See the proof" on a law a check backs shows the tagged test from its `@law:` line to its closing line, read from the default branch, with "Open in editor"; "See the CI log" on a part that holds a check-backed law or a broken one shows the run's status and the end of its failed log, with "Open on GitHub". Esc closes the drawer before the sheet. `openDrawer(drawer)` takes any `Drawer` (`ui/map-model.ts`), which is how the history strip shows a commit's files.
- **The history strip** (`src-tauri/src/history.rs`, `ui/map-history.ts`) sits at the foot of a part's sheet: the first-parent commits on the default branch, from the commit that introduced `system-map.json`, that touched the part (placed by the union of the commit's map and its parent's, or that added or removed it), oldest first, each as `#PR title` when the subject ends `(#N)`, else the subject. A commit Daniel has not caught up on (its fog entries) has a filled dot, an older one a hollow dot. Clicking a commit shows the map as of it, read leniently so a field of another time never breaks it and a deleted part still shows: read-only, with no fog, no broken-law mark, no live agents, no not placed list, and the sheet without tier change, proof, fog or CI. A quiet banner says "Viewing <commit> · Back to now". A commit's "Files" opens its changed files and diffs in the drawer, with "Open on GitHub" when `origin` is on GitHub. A scrub is one call, `scrubTo` (`ui/map-history.ts`), which asks the shell's `map_at`.

## Seen

One helper, `watchSeen` (`ui/seen.ts`), decides when something counts as seen: it covers at least half of itself or at least half of the viewport (so a card taller than twice the viewport can be seen) for 500 ms without a break, in a visible and focused window, with the view that holds it open (the story rail, Home or the Map view). It fires once. The story rail's cards use it, so a card is told as shown when seen and not when it is built; stored `shown` times are not rewritten. Home's quiet mark on a project's row uses it for the stop rules.

## Stop rules

Studio always keeps a quiet count of what Daniel uses of the map, with no setting and nothing shown (`src-tauri/src/stop_rules.rs`, `ui/stop-rules.ts`). The window tells the shell, which appends one line per event to `stop-rules.jsonl` in the data folder; a line this build cannot read is skipped, so an older or newer log never breaks it. Events, each with its local calendar day and, bar the first, its project:

- `session`: the session list had a session that day (once a day).
- `map-open`: the Map view opened on the project (once a day).
- `mark-seen`: Home's mark on the project's row was seen (once a day, by `watchSeen`).
- `scrub`: a commit of a part's history strip was shown (`scrubTo`).
- `proof`: the drawer opened, from "See the proof", the CI log or a commit's files.
- `know`: "I know this" cleared a part's fog. Kept on its own, with no threshold in project 2.
- `dismissed`: the one-time review was dismissed for the project.

An **active day** is a local calendar day with a `session` or `map-open` event. A project's merged PRs are the first-parent commits on its default branch whose subject ends `(#N)` or starts `Merge pull request #N`, counted since the first event was logged, so the count starts when tracking does. The shell hands the window these counts (`stop_facts`) and the window evaluates them (`unused`), only when the Map view opens and when Home is entered, never on a timer.

The review is due once a project has **14 active days and 20 merged PRs**, whichever comes last. It lists only the modules below their threshold: the **Map view** (opened on 4 active days, or on a fifth of the days Home's mark was seen), the **history strip** (scrubbed twice) and **See the proof** (the drawer opened twice). It is one quiet sentence with a Dismiss button, never a score, a streak or a count. It shows in the Map view's head, and on Home too, under the rest, when the Map view itself fell below its threshold, so an unused map still reaches Daniel. A dismissal is kept per project (Home's covers every project that showed it) and the review is never shown again, while the log goes on.
