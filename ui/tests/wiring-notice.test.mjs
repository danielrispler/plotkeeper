// The "not recording" notice (#437): one line in the notice line while no harness is wired, gone once one is.
import { test } from "node:test";
import assert from "node:assert/strict";
import { withWiringNotice } from "../../dist/notice.js";

test("nothing wired shows the notice once, naming plotkeeper install, beside the other notices", () => {
  const shown = withWiringNotice("settings.json: bad", false);
  const lines = shown.split("\n");
  assert.equal(lines.length, 2);
  assert.equal(lines[0], "settings.json: bad");
  assert.match(lines[1], /^Not recording: .*`plotkeeper install`/);
  assert.equal(withWiringNotice(shown, false), shown);
});

test("a harness wired removes only that line", () => {
  const shown = withWiringNotice("settings.json: bad", false);
  assert.equal(withWiringNotice(shown, true), "settings.json: bad");
  assert.equal(withWiringNotice(withWiringNotice("", false), true), "");
});
