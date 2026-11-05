// The plan's usage in the top bar (#481): the snapshot's rate limits as the window shows them. Run:
// `npm test` in apps/studio.
import { test } from "node:test";
import assert from "node:assert/strict";
import { usageText } from "../../dist/usage.js";

const now = 1738400000;
const limits = {
  five_hour: { used_percentage: 23.5, resets_at: 1738425600 },
  seven_day: { used_percentage: 41.2, resets_at: 1738857600 },
};

test("the 5-hour and weekly shares show rounded down, as the status line docs print them", () => {
  assert.equal(usageText(limits, now), "5h 23% · week 41%");
});

test("no snapshot, or one without limits, shows no usage", () => {
  assert.equal(usageText(null, now), "no usage");
  assert.equal(usageText({}, now), "no usage");
});

test("a window past its reset is dropped, as Claude Code drops it", () => {
  assert.equal(usageText(limits, 1738500000), "week 41%");
  assert.equal(usageText(limits, 1738900000), "no usage");
});
