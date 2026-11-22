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
- **Esc** peels the layers, in order: a scrubbed commit, a drawer, a part's sheet, then the view.
- **No map.** A project without the file shows a calm empty state, and "Draft a map" opens New session in that project with a drafting prompt in its quick start.
- **Live agents.** A session in `Running` or `Needs you` and each of its running Workers show as a chip on every part the current turn touched: the changed files of the turn's trees (a Worker's are the ones its name is on) plus an Edit or Write in flight, placed by `studio-system-map` through `map_place`. A `Needs you` chip keeps the loud `--needs-you` pill. A session with no changed files yet, an agent that reports no turn trees, a status-only one (Codex) or one whose edits are all ignored or owned by no part shows in a quiet **not placed** list beside the map. A Helper, a plain shell and a session in `Your turn`, `Idle` or `Ended` are never on the map. Pressing any chip selects its session or Worker and leaves the view. `hideAgents(true)` (`ui/map-agents.ts`) clears every chip and the list, for a view of a past commit.
