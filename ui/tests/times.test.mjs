// Times (danielrispler/Falinks#473): durations, the session view's times line and a check's "of usually". Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { checkTime, duration, estimateText, times } from "../../dist/times.js";

const MIN = 60_000;

test("durations read in seconds, minutes, then hours and minutes", () => {
  assert.equal(duration(45_000), "45s");
  assert.equal(duration(12 * MIN + 59_000), "12m");
  assert.equal(duration(125 * MIN), "2h 05m");
});

test("a running check shows its time against its typical time; an ended one how long it took", () => {
  /** @type {import("../../dist/types.js").ListedCheck} */
  const line = { statement: "cargo run --bin ci-checks", status: "running", started_at: 0, ended_at: null, typical_ms: 7 * MIN };
  assert.equal(checkTime(line, 3 * MIN + 10_000), "3m of usually 7m");
  assert.equal(checkTime({ ...line, typical_ms: null }, 3 * MIN), "3m");
  assert.equal(checkTime({ ...line, ended_at: 8 * MIN + 30_000, status: "failed" }, 99 * MIN), "8m");
});

// Each time as its text at `now` and its hover.
const drawn = (parts, now) => parts.map(([title, draw]) => [draw(now), title]);

test("the times count running spans to now, each labelled with hover text, empty in their first minute, and stop an ended session at its last event", () => {
  /** @type {Pick<import("../../dist/types.js").Session, "state" | "started_at" | "last_event_at" | "turn_span">} */
  const session = { state: "running", started_at: 0, last_event_at: 30 * MIN, turn_span: { start: 50 * MIN, end: null } };
  assert.deepEqual(drawn(times(session, { start: 61 * MIN, end: null }), 61 * MIN + 45_000), [
    ["Session 1h 01m", "Time since the session started"],
    ["Turn 11m", "Time on the current prompt"],
    ["", "Time on the current step"],
  ]);
  /** @type {typeof session} */
  const ended = { ...session, state: "ended", turn_span: { start: 10 * MIN, end: 20 * MIN } };
  assert.deepEqual(drawn(times(ended, null), 999 * MIN), [
    ["Session 30m", "Time since the session started"],
    ["Turn 10m", "Time on the current prompt"],
  ]);
});

test("the estimate line names the phase, then a range marked as an estimate, nothing, or depends on you in place of a time", () => {
  const work = (phase, range, depends_on_you = false) => ({
    phases: phase ? [{ phase, at: 0, agent_ms: 0 }] : [],
    estimate: { range, depends_on_you },
  });
  assert.equal(estimateText(work(null, null)), "");
  assert.equal(estimateText(work("implementing", [16 * MIN, 29 * MIN])), "Implementing · est. 16m–29m left");
  assert.equal(estimateText(work("waiting_on_ci", null)), "Waiting on CI");
  assert.equal(estimateText(work("opening_pr", [MIN, 5 * MIN], true)), "Opening the PR · depends on you");
  assert.equal(estimateText(work("orienting", null, true)), "Orienting · depends on you");
  assert.equal(estimateText(work("merged", [0, 0])), "Merged");
});

test("an estimate whose lower bound has passed reads up to its upper bound; one past its upper bound says so in words (spec #658)", () => {
  /** @type {import("../../dist/types.js").Phase} */
  const phase = "implementing";
  const work = (range) => ({ phases: [{ phase, at: 0, agent_ms: 0 }], estimate: { range, depends_on_you: false } });
  assert.equal(estimateText(work([0, 12 * MIN])), "Implementing · est. up to 12m left");
  assert.equal(estimateText(work([30_000, 12 * MIN])), "Implementing · est. up to 12m left");
  assert.equal(estimateText(work([0, 30_000])), "Implementing · past its estimate");
  assert.equal(estimateText(work([0, 0])), "Implementing · past its estimate");
  assert.equal(estimateText(work([MIN, 12 * MIN])), "Implementing · est. 1m–12m left");
});
