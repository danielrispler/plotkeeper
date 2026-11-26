# Frozen stored-file samples

Never edit a sample in this folder once it has landed. Each one is a file as some build of Studio wrote it, and a test
checks that today's code still loads it, so a change that can no longer read an older file fails
`cargo nextest` before any review. They are hand-made, never copied from a real data folder.

- `full/` holds the seven files `CardStore::open` reads, written as today's types write them, every field set.
- `minimal/` holds the same seven with only the fields that are required today.

A sample that fails is fixed in the reading code (a `#[serde(default)]`, an `Option`, a tolerant enum), never by
rewriting the sample. For a deliberate format bump, test that the old file is moved aside instead of loading it.

When a stored type changes (a field added, renamed or made required), add a new folder here (for example
`full-2/` and `minimal-2/`) with samples written in the new shape, add a test for it beside the existing ones, and keep the
old folders: every format the app ever wrote stays covered. The card store's and the risk notes' samples (`full*/`,
`minimal*/`) are loaded in `understanding/tests/stored_fixtures.rs`; the PR brief's (`pr-brief-v*/`) in
`understanding/tests/pr_brief.rs`.

`full-2/` and `minimal-2/` each hold one file, `risk-notes.json`, the store of the risk notes (#298), which `RiskNotes::open`
reads on its own beside the card store: the full one with two sessions and two levels, the minimal one with one note and only
its required fields. They also freeze the hash that keys a note (the command, a NUL and its folder, hashed with FNV-1a).

`pr-brief-v4/` holds a stored PR brief and a stored part catch-up as written once chapters carry a gist (#397): `gist` and its
`gist_flags` on a Core chapter, an empty gist on a chapter of no Core part. `understanding/tests/pr_brief.rs` loads them beside
`pr-brief-v3/`, whose replies and stored briefs have no gist and still read.

`pr-brief-v5/` holds a stored PR brief as written once a bug fix's brief carries Guard with a law's offer (#401): `guard` with the parts
the PR touched. `understanding/tests/pr_brief.rs` loads it; the older samples, with no `guard`, read with none.

`pr-brief-v6/` holds `pr-brief-records.jsonl`, the PR brief records `BriefStore::records` reads, as written once each call's usage
carries the tokens read from and written to the cache (#450): the first record from before, its calls with no
`cache_read_tokens` or `cache_write_tokens`, which read as 0; the second with both on each call. `understanding/tests/pr_brief.rs`
loads it.

`pr-brief-v7/` holds a stored PR brief as written once a brief stores what it was written without (#488, spec #477's "Partial
facts"): `partial_facts` with a cut diff's cut and partial files and its bytes sent and cut, the claim check's error and the
extensions no extractor reads. `understanding/tests/pr_brief.rs` loads it; the older samples, with no `partial_facts`, read with
nothing missing.

`pr-brief-v8/` holds a stored PR brief as written once code sorts Also touched by kind (#489, spec #477): `also_groups` with a No part,
a Tests, an Ignored and an Other group, a file with its part, an ignored one and a tagged test with its law, and no model `also` lines.
`understanding/tests/pr_brief.rs` loads it; the older samples, with `also` lines or none, read with no groups.

`pr-brief-v9/` holds a stored PR brief as written once a brief stores the import sites behind each link its PR adds (#499, spec #477
"Link evidence"): `link_evidence` with an import link whose sites sit on three lines of two files, and a cargo link whose one site has no
line. `understanding/tests/pr_brief.rs` loads it; the older samples, with no `link_evidence`, read with none.

`pr-brief-v10/` holds a stored PR brief as written once a brief stores the map check's problems at its PR's head (#641, spec #634 "The
head's map check"): `map_problems` with two problems. `understanding/tests/pr_brief.rs` loads it; the older samples, with no
`map_problems`, read with none.

`pr-brief-v11/` holds a short brief (#644, spec #634 "Short brief"), made by code for a PR that changes the map but fogs nothing:
`short` set, one chapter of one step, "The map in this PR", whose body counts the Map diff and which lights the parts its marks are on,
no model, and the head's `map_problems`.
`understanding/tests/pr_brief.rs` loads it; the older samples, with no `short`, read as written by the writer.
