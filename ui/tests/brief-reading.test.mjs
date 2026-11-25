// Two rules of reading a PR brief (port decisions Q6 and Q7 of spec #334). Run: `npm test`.
import { test, mock } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";

const observers = [];
globalThis.IntersectionObserver = /** @type {any} */ (
  class {
    constructor(run) {
      this.run = run;
      observers.push(this);
    }
    observe() {}
    disconnect() {}
  }
);
// The page's document, stood in for with only what seen.ts reads.
/** @type {any} */
const doc = { visibilityState: "visible", hasFocus: () => true };
globalThis.document = doc;
const { watchSeen, recheckSeen } = await import("../../dist/seen.js");
const { chapterGate } = await import("../../dist/map-brief-model.js");

const covering = (observer) =>
  observer.run([{ intersectionRatio: 1, intersectionRect: { width: 100, height: 100 }, rootBounds: { width: 100, height: 100 } }]);

test("a chapter counts as read, and clears its fog, only when its last step was finished in a visible, focused tab", () => {
  mock.timers.enable({ apis: ["setTimeout"] });
  let cleared = 0;
  const gate = chapterGate([], () => cleared++);
  doc.visibilityState = "hidden";
  doc.hasFocus = () => false;
  watchSeen(/** @type {any} */ ({}), { active: () => true, onSeen: () => gate.sawLast() });
  covering(observers.at(-1));
  mock.timers.tick(5000);
  assert.equal(cleared, 0, "a hidden tab reads nothing");
  doc.visibilityState = "visible";
  recheckSeen();
  mock.timers.tick(5000);
  assert.equal(cleared, 0, "visible but not focused reads nothing");
  doc.hasFocus = () => true;
  recheckSeen();
  mock.timers.tick(500);
  assert.equal(cleared, 1, "visible and focused: finished, once");
  mock.timers.reset();
});

test("a brief raises no browser notification and no tab-title count", () => {
  for (const file of ["map-brief.ts", "map-tray.ts", "rail-brief.ts", "map-ratify.ts", "ratify.ts"]) {
    const source = fs.readFileSync(new URL(`../${file}`, import.meta.url), "utf8");
    assert.doesNotMatch(source, /alerts\.js|notifyIfAway|showTitle|document\.title|new Notification/, file);
  }
});
