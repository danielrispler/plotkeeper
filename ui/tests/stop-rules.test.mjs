// The stop rules (spec danielrispler/plotkeeper#306, ticket #314): the one-time review is due once Studio has seen 14 active days and 20
// merged PRs, and lists only the modules below their thresholds. Pure logic, run on the `tsc` output.
import { test } from "node:test";
import assert from "node:assert/strict";
import { tauri } from "./dom.mjs";

const calls = [];
tauri(calls);
const { dismissReview, judge, localDay, logStop, reviewOf, reviewWords, table, unused } = await import("../../dist/stop-rules.js");

// The rows that wait 5 calendar weeks (#392) are kept by default, so a test of the other rows, judged today, holds after the wait ends.
const kept = { "refresher-entered": 2, "stale-entered": 1 };
const facts = (extra = {}) => ({ active_days: 14, merged_prs: 20, engaged_days: 0, scrubs: 0, proofs: 0, knows: 0, dismissed: [], briefs_offered: 0, briefs_opened: 0, chapters_done: 0, counts: kept, began: "2026-11-01", ...extra });

test("the review waits for both 14 active days and 20 merged PRs, whichever comes last", () => {
  assert.deepEqual(unused(facts()), ["map", "history", "proof"]);
  assert.equal(unused(facts({ active_days: 13 })), null);
  assert.equal(unused(facts({ merged_prs: 19 })), null);
  assert.equal(unused(facts({ active_days: 40, merged_prs: 19 })), null);
  assert.equal(unused(facts({ active_days: 13, merged_prs: 90 })), null);
});

test("the Map view passes at 4 engaged days in the window, and a day it was only open on does not count", () => {
  assert.ok(unused(facts({ engaged_days: 3 })).includes("map"));
  assert.ok(!unused(facts({ engaged_days: 4 })).includes("map"));
  assert.ok(!unused(facts({ engaged_days: 9 })).includes("map"));
  // Open on every active day of the window with no entered action: no engaged day.
  assert.ok(unused(facts({ active_days: 14, engaged_days: 0 })).includes("map"));
});

test("the history strip needs 2 scrubs and the proof drawers 2 opens", () => {
  assert.deepEqual(unused(facts({ engaged_days: 4, scrubs: 1, proofs: 2 })), ["history"]);
  assert.deepEqual(unused(facts({ engaged_days: 4, scrubs: 2, proofs: 1 })), ["proof"]);
});

test("nothing is listed when every module passed, and \"I know this\" has no threshold", () => {
  assert.equal(unused(facts({ engaged_days: 4, scrubs: 2, proofs: 2 })).length, 0);
  assert.equal(unused(facts({ engaged_days: 4, scrubs: 2, proofs: 2, knows: 0 })).includes("know"), false);
});

test("a dismissed module is not shown again while the logging goes on, and the others stay", () => {
  assert.deepEqual(unused(facts({ dismissed: ["map", "history", "proof", "briefs", "skips"] })), []);
  assert.deepEqual(unused(facts({ dismissed: ["map"] })), ["history", "proof"]);
  assert.deepEqual(unused(facts({ dismissed: ["history"], scrubs: 50 })), ["map", "proof"]);
});

// Two modules of a later ticket, to drive the table's exceptions: one waits 5 calendar weeks from its rollout, one waits for 3 offers.
const later = [
  { module: "refresher", name: "the refresher", kept: (f) => (f.counts["refresher-entered"] ?? 0) >= 2, wait: { from: "2026-12-01", weeks: 5 } },
  { module: "guard", name: "Guard with a law", kept: (f) => (f.counts["guard-started"] ?? 0) >= 1, offers: { kind: "guard-offered", least: 3 } },
];

test("a module with a calendar wait is not judged before its date, counted from the later of its rollout and the project's first event", () => {
  assert.deepEqual(judge(later.slice(0, 1), facts({ counts: {} }), "2027-01-04"), []);
  assert.deepEqual(judge(later.slice(0, 1), facts({ counts: {} }), "2027-01-05"), ["refresher"]);
  assert.deepEqual(judge(later.slice(0, 1), facts({ counts: { "refresher-entered": 2 } }), "2027-01-05"), []);
  // A project whose log began after the rollout waits from its own start.
  assert.deepEqual(judge(later.slice(0, 1), facts({ counts: {}, began: "2027-01-01" }), "2027-01-05"), []);
  assert.deepEqual(judge(later.slice(0, 1), facts({ counts: {}, began: "2027-01-01" }), "2027-02-05"), ["refresher"]);
});

test("a module with an offer count is not judged before that many offers", () => {
  assert.deepEqual(judge(later.slice(1), facts({ counts: { "guard-offered": 2 } }), "2027-01-05"), []);
  assert.deepEqual(judge(later.slice(1), facts({ counts: { "guard-offered": 3 } }), "2027-01-05"), ["guard"]);
  assert.deepEqual(judge(later.slice(1), facts({ counts: { "guard-offered": 3, "guard-started": 1 } }), "2027-01-05"), []);
});

test("dismissing one module leaves a module that becomes due later to be shown", () => {
  const once = facts({ dismissed: ["map", "history", "proof"], counts: {} });
  assert.deepEqual(judge(table, once, "2026-12-10"), []);
  assert.deepEqual(judge(table, once, "2027-01-05"), ["refresher", "staleness"]);
  assert.deepEqual(judge(table, { ...once, dismissed: [...once.dismissed, "refresher"] }, "2027-01-05"), ["staleness"]);
  assert.match(reviewWords(["refresher"]), /the refresher/);
});

test("the refresher needs 2 entries and staleness bands 1 refresher or part catch-up entered from a stale part, each judged from 5 weeks after rollout", () => {
  const due = (counts, began = "2026-11-01") => facts({ dismissed: ["map", "history", "proof"], counts, began });
  assert.deepEqual(judge(table, due({}), "2026-12-28"), []);
  assert.deepEqual(judge(table, due({}), "2026-12-29"), ["refresher", "staleness"]);
  assert.deepEqual(judge(table, due({ "refresher-entered": 1 }), "2026-12-29"), ["refresher", "staleness"]);
  assert.deepEqual(judge(table, due({ "refresher-entered": 2 }), "2026-12-29"), ["staleness"]);
  assert.deepEqual(judge(table, due({ "stale-entered": 1 }), "2026-12-29"), ["refresher"]);
  // A project whose log began after the rollout waits 5 weeks from its own start.
  assert.deepEqual(judge(table, due({}, "2026-12-01"), "2027-01-04"), []);
  assert.deepEqual(judge(table, due({}, "2026-12-01"), "2027-01-05"), ["refresher", "staleness"]);
  assert.match(reviewWords(["refresher", "staleness"]), /the refresher and staleness bands/);
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
  logStop("map-action", "/a");
  logStop("map-action", "/a");
  logStop("scrub", "/a");
  logStop("scrub", "/a");
  const sent = calls.map(([command, args]) => `${command} ${args.kind} ${args.project}`);
  assert.deepEqual(sent, ["stop_log map-open /a", "stop_log map-open /b", "stop_log map-action /a", "stop_log scrub /a", "stop_log scrub /a"]);
  assert.ok(calls.every(([, args]) => args.day === localDay()));
});

test("the review is read from the shell's counts, and a dismissal tells it every project and the modules shown", async () => {
  calls.length = 0;
  tauri(calls, () => facts({ engaged_days: 4, scrubs: 2 }));
  assert.deepEqual(await reviewOf("/a"), ["proof"]);
  await dismissReview(["/a", "/b"], ["proof"]);
  assert.deepEqual(calls.at(-1)[1].projects, ["/a", "/b"]);
  assert.deepEqual(calls.at(-1)[1].modules, ["proof"]);
});

test("PR briefs are listed when opened for under a quarter of the merged PRs that had one", () => {
  const passed = { engaged_days: 4, scrubs: 2, proofs: 2 };
  assert.deepEqual(unused(facts({ ...passed, briefs_offered: 8, briefs_opened: 1 })), ["briefs"]);
  assert.deepEqual(unused(facts({ ...passed, briefs_offered: 8, briefs_opened: 2 })), []);
  assert.deepEqual(unused(facts({ ...passed, briefs_offered: 3, briefs_opened: 0 })), ["briefs"]);
  // No brief offered, nothing to judge.
  assert.deepEqual(unused(facts({ ...passed })), []);
  // Not due yet: nothing is listed however little was opened.
  assert.equal(unused(facts({ ...passed, active_days: 3, briefs_offered: 8 })), null);
});

test("the review notes when \"I know this\" cleared more fog than finished chapters, only where briefs exist", () => {
  const passed = { engaged_days: 4, scrubs: 2, proofs: 2, briefs_offered: 4, briefs_opened: 4 };
  assert.deepEqual(unused(facts({ ...passed, knows: 5, chapters_done: 2 })), ["skips"]);
  assert.deepEqual(unused(facts({ ...passed, knows: 2, chapters_done: 2 })), []);
  assert.deepEqual(unused(facts({ ...passed, knows: 5, chapters_done: 2, briefs_opened: 0 })), ["briefs", "skips"]);
  assert.deepEqual(unused(facts({ ...passed, knows: 5, briefs_offered: 0, briefs_opened: 0 })), []);
  const words = reviewWords(["briefs", "skips"]);
  assert.match(words, /PR briefs/);
  assert.match(words, /I know this/);
  assert.doesNotMatch(words, /\d/);
  assert.doesNotMatch(reviewWords(["skips"]), /Little used/);
});

test("a brief offered or an unconfirmed mark seen is told once per PR, other brief events every time, each with its PR", () => {
  calls.length = 0;
  logStop("brief-offered", "/a", 7);
  logStop("brief-offered", "/a", 7);
  logStop("brief-offered", "/a", 8);
  logStop("brief-offered", "/b", 7);
  logStop("unconfirmed-seen", "/a", 7);
  logStop("unconfirmed-seen", "/a", 7);
  logStop("brief-opened", "/a", 7);
  logStop("brief-opened", "/a", 7);
  const sent = calls.map(([, args]) => `${args.kind} ${args.project} ${args.pr}`);
  assert.deepEqual(sent, ["brief-offered /a 7", "brief-offered /a 8", "brief-offered /b 7", "unconfirmed-seen /a 7", "brief-opened /a 7", "brief-opened /a 7"]);
});
