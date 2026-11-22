// The stop rules (spec danielrispler/plotkeeper#306, ticket #314): the one-time review is due once Studio has seen 14 active days and 20
// merged PRs, and lists only the modules below their thresholds. Pure logic, run on the `tsc` output.
import { test } from "node:test";
import assert from "node:assert/strict";
import { tauri } from "./dom.mjs";

const calls = [];
tauri(calls);
const { dismissReview, localDay, logStop, onHome, reviewOf, reviewWords, unused } = await import("../../dist/stop-rules.js");

const facts = (extra = {}) => ({ active_days: 14, merged_prs: 20, map_days: 0, mark_days: 0, map_on_mark_days: 0, scrubs: 0, proofs: 0, knows: 0, dismissed: false, ...extra });

test("the review waits for both 14 active days and 20 merged PRs, whichever comes last", () => {
  assert.deepEqual(unused(facts()), ["map", "history", "proof"]);
  assert.equal(unused(facts({ active_days: 13 })), null);
  assert.equal(unused(facts({ merged_prs: 19 })), null);
  assert.equal(unused(facts({ active_days: 40, merged_prs: 19 })), null);
  assert.equal(unused(facts({ active_days: 13, merged_prs: 90 })), null);
});

test("the Map view passes with 4 days of opens, or with opens on a fifth of the days Home's mark was seen", () => {
  assert.ok(unused(facts({ map_days: 3 })).includes("map"));
  assert.ok(!unused(facts({ map_days: 4 })).includes("map"));
  assert.ok(!unused(facts({ map_days: 1, mark_days: 5, map_on_mark_days: 1 })).includes("map"));
  assert.ok(unused(facts({ map_days: 1, mark_days: 6, map_on_mark_days: 1 })).includes("map"));
  assert.ok(unused(facts({ map_days: 0, mark_days: 0, map_on_mark_days: 0 })).includes("map"));
});

test("the history strip needs 2 scrubs and the proof drawers 2 opens", () => {
  assert.deepEqual(unused(facts({ map_days: 4, scrubs: 1, proofs: 2 })), ["history"]);
  assert.deepEqual(unused(facts({ map_days: 4, scrubs: 2, proofs: 1 })), ["proof"]);
});

test("nothing is listed when every module passed, and \"I know this\" has no threshold", () => {
  assert.equal(unused(facts({ map_days: 4, scrubs: 2, proofs: 2 })).length, 0);
  assert.equal(unused(facts({ map_days: 4, scrubs: 2, proofs: 2, knows: 0 })).includes("know"), false);
});

test("a dismissed review is not shown again while the logging goes on", () => {
  assert.equal(unused(facts({ dismissed: true })), null);
  assert.equal(unused(facts({ dismissed: true, scrubs: 50 })), null);
});

test("Home shows the review only for a project whose Map view missed its threshold", () => {
  const found = { "/a": ["history"], "/b": ["map", "proof"], "/c": null, "/d": [] };
  assert.deepEqual(onHome(found), ["/b"]);
  assert.deepEqual(onHome({ "/a": ["history"] }), []);
});

test("the words name the modules and carry no number", () => {
  const all = reviewWords(["map", "history", "proof"]);
  assert.match(all, /Map view/);
  assert.match(all, /history strip/);
  assert.match(all, /See the proof/);
  assert.doesNotMatch(all, /\d/);
  assert.doesNotMatch(reviewWords(["history"]), /Map view/);
});

test("a day is the local calendar day", () => {
  assert.equal(localDay(new Date(2026, 10, 3, 23, 59)), "2026-11-03");
  assert.equal(localDay(new Date(2026, 0, 9, 0, 1)), "2026-01-09");
});

test("a day-counting event is told once a day per project, and a use every time", () => {
  calls.length = 0;
  logStop("map-open", "/a");
  logStop("map-open", "/a");
  logStop("map-open", "/b");
  logStop("scrub", "/a");
  logStop("scrub", "/a");
  const sent = calls.map(([command, args]) => `${command} ${args.kind} ${args.project}`);
  assert.deepEqual(sent, ["stop_log map-open /a", "stop_log map-open /b", "stop_log scrub /a", "stop_log scrub /a"]);
  assert.ok(calls.every(([, args]) => args.day === localDay()));
});

test("the review is read from the shell's counts, and a dismissal tells it every project", async () => {
  calls.length = 0;
  tauri(calls, () => facts({ map_days: 4, scrubs: 2 }));
  assert.deepEqual(await reviewOf("/a"), ["proof"]);
  await dismissReview(["/a", "/b"]);
  assert.deepEqual(calls.at(-1)[1].projects, ["/a", "/b"]);
});
