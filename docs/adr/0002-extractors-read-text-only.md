# Extractors read text only; `cargo metadata` is the one exception

The system map's link check holds the map's `import` links to the edges its extractors compute (`docs/studio/behaviour/map.md`, "The link check"). Many languages have a toolchain that would answer "which file depends on which" more exactly than a reader of the text: `go list`, `mvn dependency:tree`, `dotnet msbuild`, a Python interpreter's import machinery. But a check that runs one gives a different answer on a machine without that toolchain, or with another version of it, or offline, and CI would need every language's toolchain installed to check one map. So an extractor reads the repository's tracked files as text and runs no toolchain (spec #529, "Invariants for every extractor", decided in the grilling of 2026-11-25). The one exception is `cargo metadata --no-deps --offline`, which both Rust extractors run: `cargo` for the workspace's crates and their dependencies, and `rust-modules` for each crate's library name, root file and normal dependencies. It is allowed because Rust is plotkeeper's own toolchain, already present wherever plotkeeper's check runs.

## Considered Options

- **Each language's toolchain.** Exact answers, at the cost of a check that depends on what is installed. Rejected: the check must give the same answer on any machine and in CI.
- **Text only, with `cargo metadata` as the one exception** (chosen). Each extractor owns a small lexer and reads its configs by hand, or with a small targeted crate where hand reading is error-prone (each language's spec decides).
- **Text only, with no exception.** Rejected for now: reading Cargo's workspace members, renames and target paths by hand repeats what `cargo metadata` already gives, and the Rust extractors predate this rule.

## Consequences

- A dependency the text does not show, such as Java code using its own package, is a silent under-count, and its extractor's doc section lists it. An import the text shows but the extractor cannot resolve exactly is reported, never guessed.
- `cargo metadata` reads the working tree, so the Rust extractors are the only ones a future commit-mode read (#498) cannot serve from git alone; #498 decides what it does for them.
- Adding a toolchain call to any other extractor reverses this record and needs a new one.
