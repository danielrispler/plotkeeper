# `go-imports` indexes every module and does not follow `replace`

`go-imports` resolves an import path to the local module that provides it (go.dev/ref/mod, "Resolving a package to a module"). Which local modules exist is a choice the go command never makes alone: it builds from one working directory, whose main modules, `go.work` and `replace` directives decide which copy of a path it uses. A check that reads text has no working directory. So `go-imports` indexes the `module` line of every tracked `go.mod` outside `testdata/`, `vendor/` and folders starting with `_` or `.`, whether or not a `go.work` or a `replace` reaches it, and treats every tracked `go.mod` as a module boundary (spec #569, "The module index"; grilling of 2026-11-25, round 1, Q2, with Daniel's pushback that this is a semantic choice, not a consequence of ADR 0002's text-only rule).

`replace` is not read. A local replace's target has its own `go.mod`, whose `module` line already indexes it, and which copy `go` builds depends on the importing module and the working directory, which a text reader cannot copy (`docs/studio/research/go-extractor.md`, "Against the starting points"). A path two indexed modules declare is doubtful, and an import of it makes the importing file Unsure, never a pair, except that a file's own module always wins for its own path. `go.work` only breaks such a tie, when exactly one declarer is in a tracked, non-excluded `go.work`'s `use` list (round 2, Q7; round 3, Q12). It never adds or removes a module from the index. Two `go.work` files that pick different winners leave the path doubtful.

A `use` path is joined to its `go.work`'s folder and cleaned. One that names a folder outside the repository, an absolute path, or a folder with no tracked, indexed `go.mod` declares nothing and is no problem. That is a deliberate departure from Go, which fails there with "cannot load module %s listed in go.work file" (`cmd/go/internal/modload/init.go:1076` on golang/go master, 2026-11-25): a developer's `go.work` often uses a sibling repository beside the git root, and that must not break the check (spec #569, R4). A `go.work` that Go's parser would reject (an unclosed block, an unknown directive or block kind, a line that does not tokenise, or both declarers of one path used) is a problem naming its file and line, and settles no tie.

## Considered Options

- **Every tracked module, `replace` not read** (chosen). A monorepo of many modules gets edges between all of them, and a doubt is reported rather than guessed.
- **Only the modules a root `go.work` uses.** Rejected: a repository without `go.work`, the common case, would index nothing beyond its root module, and a `go.work` is often a developer's local file.
- **Follow `replace` like the go command.** Rejected: the answer depends on which module is the main one, so one import could resolve two ways in one repository, and a check must give one answer.

## Consequences

- An import resolves to the local source even where `go` would build a published version or a vendored copy. `docs/studio/behaviour/map.md`'s `go-imports` section states it as an over-count.
- A copied example module that declares another module's path makes imports of that path from a third module Unsure, until a `go.work` settles it.
- A `go.work` whose `use` Go would fail on still passes the check, so `go-imports` can accept a workspace `go` cannot load.
- Reading `replace`, or narrowing the index to what `go.work` uses, reverses this record and needs a new one.
