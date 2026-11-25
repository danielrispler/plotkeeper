# `go-imports` indexes every module and does not follow `replace`

`go-imports` resolves an import path to the local module that provides it (go.dev/ref/mod, "Resolving a package to a module"). Which local modules exist is a choice the go command never makes alone: it builds from one working directory, whose main modules, `go.work` and `replace` directives decide which copy of a path it uses. A check that reads text has no working directory. So `go-imports` indexes the `module` line of every tracked `go.mod` outside `testdata/`, `vendor/` and folders starting with `_` or `.`, whether or not a `go.work` or a `replace` reaches it, and treats every tracked `go.mod` as a module boundary (spec #569, "The module index"; grilling of 2026-11-25, round 1, Q2, with Daniel's pushback that this is a semantic choice, not a consequence of ADR 0002's text-only rule).

`replace` is not read. A local replace's target has its own `go.mod`, whose `module` line already indexes it, and which copy `go` builds depends on the importing module and the working directory, which a text reader cannot copy (`docs/studio/research/go-extractor.md`, "Against the starting points"). A path two indexed modules declare is doubtful, and an import of it makes the importing file Unsure, never a pair, except that a file's own module always wins for its own path. `go.work` only breaks such a tie, when exactly one declarer is in a tracked, non-excluded `go.work`'s `use` list (round 2, Q7; round 3, Q12). It never adds or removes a module from the index. That tie-break lands with the `go.work` ticket (#579); until then a path declared twice is always doubtful.

## Considered Options

- **Every tracked module, `replace` not read** (chosen). A monorepo of many modules gets edges between all of them, and a doubt is reported rather than guessed.
- **Only the modules a root `go.work` uses.** Rejected: a repository without `go.work`, the common case, would index nothing beyond its root module, and a `go.work` is often a developer's local file.
- **Follow `replace` like the go command.** Rejected: the answer depends on which module is the main one, so one import could resolve two ways in one repository, and a check must give one answer.

## Consequences

- An import resolves to the local source even where `go` would build a published version or a vendored copy. `docs/studio/behaviour/map.md`'s `go-imports` section states it as an over-count.
- A copied example module that declares another module's path makes imports of that path from a third module Unsure, until a `go.work` settles it (#579).
- Reading `replace`, or narrowing the index to what `go.work` uses, reverses this record and needs a new one.
