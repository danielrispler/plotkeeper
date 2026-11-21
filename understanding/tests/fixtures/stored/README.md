# Frozen stored-file samples

Never edit a sample in this folder once it has landed. Each one is a file as some build of Studio wrote it, and
`stored_fixtures.rs` checks that today's code still loads it, so a change that can no longer read an older file fails
`cargo nextest` before any review. They are hand-made, never copied from a real data folder.

- `full/` holds the seven files `CardStore::open` reads, written as today's types write them, every field set.
- `minimal/` holds the same seven with only the fields that are required today.

A sample that fails is fixed in the reading code (a `#[serde(default)]`, an `Option`, a tolerant enum), never by
rewriting the sample. For a deliberate format bump, test that the old file is moved aside instead of loading it.

When a stored type changes (a field added, renamed or made required), add a new folder here (for example
`full-2/` and `minimal-2/`) with samples written in the new shape, add a test for it beside the existing ones in
`stored_fixtures.rs`, and keep the old folders: every format the app ever wrote stays covered.
