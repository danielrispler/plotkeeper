// The Map view's "Ready for review" tray (ticket #336), run on the `tsc` output.
import { test } from "node:test";
import assert from "node:assert/strict";
import { trayRows } from "../../dist/map-tray.js";

test("a ready PR is a row with its title and number, and says its brief is not written yet", () => {
  const rows = trayRows([{ number: 7, title: "Fog on the map", head: "aaa" }, { number: 9, title: "", head: "bbb" }]);
  assert.deepEqual(rows.map((r) => [r.number, r.title]), [[7, "Fog on the map"], [9, "PR #9"]]);
  assert.match(rows[0].note, /#7 .*not written yet/);
});

test("with nothing ready there is no row", () => {
  assert.deepEqual(trayRows([]), []);
});
