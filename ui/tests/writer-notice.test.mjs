// The no-writer notice (danielrispler/plotkeeper#51): one line in the notice line, beside others, cleared when a call works.
import { test } from "node:test";
import assert from "node:assert/strict";
import { GATE, withGateNotice, withWriterNotice } from "../../dist/writer-notice.js";

const off = "Summaries are off: `claude` did not start (No such file).";

test("the notice shows once and keeps the other notices", () => {
  const shown = withWriterNotice("settings.json: bad", off);
  assert.equal(shown, `settings.json: bad\n${off}`);
  assert.equal(withWriterNotice(shown, off), shown);
});

test("clearing removes only that line", () => {
  assert.equal(withWriterNotice(`settings.json: bad\n${off}`, null), "settings.json: bad");
  assert.equal(withWriterNotice(off, null), "");
});

test("summaries opening clears only the gate, and keeps a writer's failure notice", () => {
  const gated = withGateNotice("settings.json: bad", true);
  assert.equal(gated, `settings.json: bad\n${GATE}`);
  assert.equal(withGateNotice(gated, false), "settings.json: bad");
  assert.equal(withGateNotice(`settings.json: bad\n${off}`, false), `settings.json: bad\n${off}`);
});
