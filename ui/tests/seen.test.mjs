// Seen (danielrispler/plotkeeper#306): an element is seen when it covers half of itself or half of the viewport for 500 ms, in a
// visible, focused window, with its view open. Run: `npm test`.
import { afterEach, beforeEach, test, mock } from "node:test";
import assert from "node:assert/strict";

const observers = [];
// The stand-ins for the browser's observer, document and elements, typed as any at this boundary.
globalThis.IntersectionObserver = /** @type {any} */ (
  class {
    constructor(run) {
      this.run = run;
      observers.push(this);
    }
    observe() {}
    disconnect() {
      this.gone = true;
    }
  }
);
const doc = { visibilityState: "visible", hasFocus: () => true };
globalThis.document = /** @type {any} */ (doc);
const { watchSeen, recheckSeen, covers } = await import("../../dist/seen.js");

// Reports the element as covering `element` of itself and `viewport` of the viewport (a 100 x 100 viewport).
const report = (observer, element, viewport) =>
  observer.run([{ intersectionRatio: element, intersectionRect: { width: 100, height: viewport * 100 }, rootBounds: { width: 100, height: 100 } }]);
let open = true;
let seen = 0;
const watch = () => watchSeen(/** @type {any} */ ({}), { active: () => open, onSeen: () => seen++ });
beforeEach(() => {
  observers.length = 0;
  doc.visibilityState = "visible";
  doc.hasFocus = () => true;
  open = true;
  seen = 0;
  mock.timers.enable({ apis: ["setTimeout"] });
});

test("an element off screen is never seen", () => {
  watch();
  report(observers[0], 0, 0);
  mock.timers.tick(5000);
  assert.equal(seen, 0);
});

test("half of the element for 500 ms is seen, once", () => {
  watch();
  report(observers[0], 0.5, 0.1);
  mock.timers.tick(499);
  assert.equal(seen, 0);
  mock.timers.tick(1);
  assert.equal(seen, 1);
  report(observers[0], 1, 1);
  mock.timers.tick(5000);
  assert.equal(seen, 1);
});

test("a card taller than twice the viewport is seen once half the viewport is covered", () => {
  watch();
  report(observers[0], 0.3, 0.5);
  mock.timers.tick(500);
  assert.equal(seen, 1);
  assert.equal(covers({ element: 0.2, viewport: 0.49 }), false);
});

test("a scroll through shorter than 500 ms is not seen", () => {
  watch();
  report(observers[0], 1, 1);
  mock.timers.tick(300);
  report(observers[0], 0, 0);
  mock.timers.tick(5000);
  assert.equal(seen, 0);
});

test("a hidden or unfocused window does not see, and seeing starts when it returns", () => {
  watch();
  doc.visibilityState = "hidden";
  report(observers[0], 1, 1);
  mock.timers.tick(5000);
  assert.equal(seen, 0);
  doc.visibilityState = "visible";
  doc.hasFocus = () => false;
  recheckSeen();
  mock.timers.tick(5000);
  assert.equal(seen, 0);
  doc.hasFocus = () => true;
  recheckSeen();
  mock.timers.tick(500);
  assert.equal(seen, 1);
});

test("a closed view does not see, and leaving it within 500 ms cancels", () => {
  open = false;
  watch();
  report(observers[0], 1, 1);
  mock.timers.tick(5000);
  assert.equal(seen, 0);
  open = true;
  recheckSeen();
  mock.timers.tick(300);
  open = false;
  recheckSeen();
  mock.timers.tick(5000);
  assert.equal(seen, 0);
});

test("a stopped watcher never fires", () => {
  const stop = watch();
  report(observers[0], 1, 1);
  stop();
  mock.timers.tick(5000);
  assert.equal(seen, 0);
  assert.equal(observers[0].gone, true);
});

afterEach(() => mock.timers.reset());

// An interaction with the element (#331): a card counts as seen only after Daniel pointed at, pressed, scrolled or focused it.
const heard = [];
const touchable = () =>
  /** @type {any} */ ({
    addEventListener: (type, run) => heard.push([type, run]),
    removeEventListener: (type) => heard.splice(0, heard.length, ...heard.filter(([t]) => t !== type)),
  });
const interact = (type) => heard.filter(([t]) => t === type).forEach(([, run]) => run());

test("with interaction asked, a visible focused card in view is not seen until Daniel touches it", () => {
  heard.length = 0;
  watchSeen(touchable(), { active: () => open, onSeen: () => seen++, interaction: true });
  report(observers[0], 1, 1);
  mock.timers.tick(5000);
  assert.equal(seen, 0);
  interact("pointermove");
  mock.timers.tick(499);
  assert.equal(seen, 0);
  mock.timers.tick(1);
  assert.equal(seen, 1);
});

test("an interaction in a hidden window does not make a card seen", () => {
  heard.length = 0;
  watchSeen(touchable(), { active: () => open, onSeen: () => seen++, interaction: true });
  doc.visibilityState = "hidden";
  report(observers[0], 1, 1);
  interact("keydown");
  mock.timers.tick(5000);
  assert.equal(seen, 0);
});
