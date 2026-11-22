# The system map

`system-map.json` at the repository's root describes the system in plain words: its parts and the files each owns, the outside actors it talks to, the links between them, the files that belong to no part on purpose, and the laws that must stay true. `system-map check` holds the file to the repository, and `npm run check`, and so CI, runs it. The command and the library are the `system-map/` crate (`studio-system-map`); it assumes nothing about plotkeeper.

## The file

JSON, one object. A field the checker does not know fails the check.

- `version`: `1`.
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
- `typescript-imports`: relative import specifiers resolved to tracked files. A specifier that resolves to no tracked file is skipped; a dynamic import or a path alias is not seen.

The checker runs only `git` and, when the map names `cargo`, `cargo`; it reads no file outside the repository and needs no network.

## Which parts a change touches

`system-map touched [--json] [--root <dir>] [path...]` names the parts that own the paths, with the ignored paths and the unowned ones listed apart. It takes paths as arguments or one per line on standard input, so `git diff --name-only | system-map touched` works. It matches by pattern alone, so a deleted or moved path is still placed.

## plotkeeper's map

Eight parts: the agent plugins, the hook, the core, the shell (`src-tauri/`), understanding, the window (`ui/`), the terminal daemon and workbench; three actors: the agents, the writer and GitHub; one flow, Turn writing. `ignored` holds what is not the running system: the docs, the spikes, `scripts/`, the tooling crates (`wayfinder/`, `test-support/`, `system-map/`) and the repository's own configuration at the root. A change that adds, moves or removes files, changes what a part does or how parts connect, or changes a law changes the map in the same PR (`AGENTS.md`).
