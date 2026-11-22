// A scrub to a commit whose map cannot be read still has its way back to now (spec danielrispler/plotkeeper#306, outside review H1).
import { test } from "node:test";
import assert from "node:assert/strict";
import { El, fakeDocument, tauri } from "./dom.mjs";

test("Esc and Back to now leave a scrubbed commit whose map cannot be read", async () => {
  const els = new Map();
  const get = (id) => {
    if (!els.has(id)) {
      const el = new El("div");
      el.matches = () => false;
      el.hidden = false;
      els.set(id, el);
    }
    return els.get(id);
  };
  globalThis.document = fakeDocument({ getElementById: get, body: { classList: { contains: () => true, add() {}, remove() {}, toggle() {} } } });
  tauri([]);
  const { setScrub, currentScrub, currentMap, mapEscape } = await import("../../dist/map.js");
  const { scrubTo } = await import("../../dist/map-history.js");
  setScrub({ commit: "abc", short: "abc", label: "x", source: "not a map" });
  assert.equal(currentMap(), null, "no map could be drawn");
  assert.notEqual(currentScrub(), null);
  mapEscape();
  assert.equal(currentScrub(), null, "Esc returns to now");
  setScrub({ commit: "abc", short: "abc", label: "x", source: "not a map" });
  await scrubTo(null);
  assert.equal(currentScrub(), null, "Back to now returns to now");
});
