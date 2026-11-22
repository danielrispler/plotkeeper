// Broken laws and the inline proof drawer's pure logic (spec danielrispler/plotkeeper#306, ticket #312), run on the `tsc` output.
import { test } from "node:test";
import assert from "node:assert/strict";
import { brokenParts, brokenText, ciDrawer, ciLine, commitDrawer, proofDrawer, scrubBanner, sheetCi, stripOf } from "../../dist/map-model.js";

const ci = (status, extra = {}) => ({ status, run_url: "https://github.com/o/r/actions/runs/9", commit: "abc1234def", earlier: false, parts: [], laws: [], tail: "", note: null, ...extra });
const broken = ci("broken", { parts: ["shell"], laws: ["wire-types-agree"], tail: "FAIL wire_types_match\nSummary 1 failed" });

test("only a run that failed in a test that backs a law marks parts", () => {
  assert.deepEqual([...brokenParts(broken)], ["shell"]);
  for (const status of ["unknown", "passed", "not-run", "unreadable"]) assert.equal(brokenParts(ci(status)).size, 0, status);
  assert.equal(brokenParts(null).size, 0);
});

test("each outcome has its own words, and a missing mark never says every law test passed", () => {
  const lines = ["passed", "broken", "not-run", "unreadable", "unknown"].map((status) => ciLine(ci(status)));
  assert.equal(new Set(lines).size, 5);
  assert.equal(ciLine(broken), brokenText);
  assert.match(ciLine(ci("not-run")), /law tests did not run/);
  assert.match(ciLine(ci("unreadable")), /log cannot be read/);
  assert.match(ciLine(ci("passed")), /passed/);
  assert.equal(ciLine(null), ciLine(ci("unknown")));
});

test("a newer run in progress says the result is for an earlier commit", () => {
  assert.match(ciLine(ci("passed", { earlier: true })), /earlier commit/);
  assert.match(ciDrawer(ci("passed", { earlier: true })).note ?? "", /earlier commit/);
  assert.doesNotMatch(ciLine(ci("passed")), /earlier/);
});

test("the sheet shows CI only on a part with a check-backed law or a broken law, and the broken line is the loud one", () => {
  assert.equal(sheetCi(broken, "window", false), null);
  assert.equal(sheetCi(null, "shell", false), null);
  assert.deepEqual(sheetCi(broken, "shell", false), { line: brokenText, broken: true, drill: true });
  assert.deepEqual(sheetCi(ci("passed"), "shell", true), { line: ciLine(ci("passed")), broken: false, drill: true });
  // Nothing was read: a line, but nothing to drill into.
  assert.equal(sheetCi(null, "shell", true)?.drill, false);
  assert.equal(sheetCi(ci("unknown"), "shell", true)?.drill, false);
});

test("See the proof shows each tagged test's code with a jump to the editor at its tag", () => {
  const proofs = [{ file: "tests/wire.rs", line: 3, test: "wire_types_match", first: 3, snippet: "// @law: wire-types-agree\n#[test]\nfn wire_types_match() {}" }];
  const drawer = proofDrawer("wire-types-agree", proofs);
  assert.match(drawer.title, /wire-types-agree/);
  assert.equal(drawer.blocks.length, 1);
  assert.equal(drawer.blocks[0].code, proofs[0].snippet);
  assert.match(drawer.blocks[0].label ?? "", /tests\/wire\.rs:3/);
  assert.deepEqual(drawer.blocks[0].jumps.map((jump) => [jump.label, jump.to]), [["Open in editor", { file: "tests/wire.rs", line: 3 }]]);
  assert.deepEqual(drawer.jumps, []);
});

test("a law with no test found says so instead of showing an empty drawer", () => {
  const drawer = proofDrawer("gone", []);
  assert.equal(drawer.blocks.length, 1);
  assert.match(drawer.blocks[0].code, /No test/);
  assert.deepEqual(drawer.blocks[0].jumps, []);
});

test("the broken-law drill-down shows the log's tail and opens the run on GitHub", () => {
  const drawer = ciDrawer(broken);
  assert.equal(drawer.title, brokenText);
  assert.equal(drawer.blocks[0].code, broken.tail);
  assert.deepEqual(drawer.jumps.map((jump) => [jump.label, jump.to]), [["Open on GitHub", { url: broken.run_url }]]);
  // No log to show: the status still says why, and there is no block.
  const quiet = ciDrawer(ci("passed"));
  assert.equal(quiet.blocks.length, 0);
  assert.match(quiet.title, /passed/);
  assert.equal(ciDrawer(ci("unreadable", { run_url: null })).jumps.length, 0);
});

const entry = (commit, extra = {}) => ({ commit, short: commit.slice(0, 7), label: `change ${commit}`, fogged: false, ...extra });

test("the strip marks the commit being viewed and the ones Daniel has not caught up on, oldest first", () => {
  const rows = stripOf([entry("aaaaaaaa"), entry("bbbbbbbb", { fogged: true }), entry("cccccccc", { fogged: true })], "bbbbbbbb");
  assert.deepEqual(rows.map((row) => [row.commit, row.current, row.fogged]), [["aaaaaaaa", false, false], ["bbbbbbbb", true, true], ["cccccccc", false, true]]);
  assert.ok(stripOf([entry("aaaaaaaa")], null).every((row) => !row.current));
  // Fogged and caught-up rows tell themselves apart in words too, not only by colour.
  assert.notEqual(rows[0].title, rows[2].title);
});

test("the banner names the commit and offers the way back", () => {
  assert.deepEqual(scrubBanner({ commit: "abcdef0123", short: "abcdef0", label: "#4 Thing" }), { text: "Viewing abcdef0", back: "Back to now" });
});

test("a commit's drill-down lists its files with their diffs and opens the commit on GitHub", () => {
  const found = { commit: "abc1234def", short: "abc1234", label: "#7 Edit a", url: "https://github.com/o/r/commit/abc1234def", files: [{ path: "a/x", diff: "-one\n+two" }, { path: "b/y", diff: "+n" }] };
  const drawer = commitDrawer(found);
  assert.equal(drawer.title, "#7 Edit a");
  assert.match(drawer.note ?? "", /abc1234/);
  assert.match(drawer.note ?? "", /2 files/);
  assert.deepEqual(drawer.blocks.map((block) => [block.label, block.code]), [["a/x", "-one\n+two"], ["b/y", "+n"]]);
  assert.deepEqual(drawer.jumps.map((jump) => [jump.label, jump.to]), [["Open on GitHub", { url: found.url }]]);
  const local = commitDrawer({ ...found, url: null, files: [] });
  assert.deepEqual(local.jumps, []);
  assert.match(local.blocks[0].code, /No files/);
});
