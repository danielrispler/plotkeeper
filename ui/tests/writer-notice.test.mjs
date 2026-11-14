// The no-writer notice (danielrispler/plotkeeper#51): one line in the notice line, beside others, cleared when a call works.
import { test } from "node:test";
import assert from "node:assert/strict";
import { withWriterNotice } from "../../dist/writer-notice.js";

const off = "Cards and briefs are off: `claude` did not start (No such file).";

test("the notice shows once and keeps the other notices", () => {
  const shown = withWriterNotice("settings.json: bad", off);
  assert.equal(shown, `settings.json: bad\n${off}`);
  assert.equal(withWriterNotice(shown, off), shown);
});

test("clearing removes only that line", () => {
  assert.equal(withWriterNotice(`settings.json: bad\n${off}`, null), "settings.json: bad");
  assert.equal(withWriterNotice(off, null), "");
});
