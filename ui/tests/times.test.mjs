// Times (#473): durations, the session view's times line and a check's "of usually". Run: `npm test`
// in apps/studio.
import { test } from "node:test";
import assert from "node:assert/strict";
import { checkTime, duration, estimateText, timesText } from "../../dist/times.js";

const MIN = 60_000;

test("durations read in seconds, minutes, then hours and minutes", () => {
  assert.equal(duration(45_000), "45s");
  assert.equal(duration(12 * MIN + 59_000), "12m");
  assert.equal(duration(125 * MIN), "2h 05m");
});

test("a running check shows its time against its typical time; an ended one how long it took", () => {
  const line = { command: "cargo run --bin ci-checks", passed: null, started_at: 0, ended_at: null, typical_ms: 7 * MIN };
  assert.equal(checkTime(line, 3 * MIN + 10_000), "3m of usually 7m");
  assert.equal(checkTime({ ...line, typical_ms: null }, 3 * MIN), "3m");
  assert.equal(checkTime({ ...line, ended_at: 8 * MIN + 30_000, passed: true }, 99 * MIN), "8m");
});

test("the times line counts running spans to now and stops an ended session at its last event", () => {
  const session = { state: "running", started_at: 0, last_event_at: 30 * MIN, turn_span: { start: 50 * MIN, end: null } };
  assert.equal(timesText(session, { start: 61 * MIN, end: null }, 61 * MIN + 45_000), "Session 1h 01m · Turn 11m · Step 45s");
  const ended = { ...session, state: "ended", turn_span: { start: 10 * MIN, end: 20 * MIN } };
  assert.equal(timesText(ended, null, 999 * MIN), "Session 30m · Turn 10m");
});

test("the estimate line names the phase, then a range marked as an estimate, no estimate, or depends on you in place of a time", () => {
  const work = (phase, range, depends_on_you = false) => ({
    phases: phase ? [{ phase, at: 0, agent_ms: 0 }] : [],
    estimate: { range, depends_on_you },
  });
  assert.equal(estimateText(work(null, null)), "");
  assert.equal(estimateText(work("implementing", [16 * MIN, 29 * MIN])), "Implementing · est. 16m–29m left");
  assert.equal(estimateText(work("waiting_on_ci", null)), "Waiting on CI · no estimate");
  assert.equal(estimateText(work("opening_pr", [MIN, 5 * MIN], true)), "Opening the PR · depends on you");
  assert.equal(estimateText(work("orienting", null, true)), "Orienting · depends on you");
  assert.equal(estimateText(work("merged", [0, 0])), "Merged");
});
