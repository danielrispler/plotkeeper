// The overlays' hover and the seen rule (danielrispler/plotkeeper#128), on a clock the test moves.
import { test } from "node:test";
import assert from "node:assert/strict";
import { hover, shownGate } from "../../dist/hover.js";

// A clock whose time moves by `tick`, running each timer that falls due.
function clock() {
  let now = 0;
  let next = 1;
  const timers = new Map();
  return {
    set(run, ms) {
      timers.set(next, { at: now + ms, run });
      return next++;
    },
    clear: (id) => timers.delete(id),
    tick(ms) {
      const end = now + ms;
      for (;;) {
        const due = [...timers].filter(([, t]) => t.at <= end).sort((a, b) => a[1].at - b[1].at)[0];
        if (!due) break;
        timers.delete(due[0]);
        now = due[1].at;
        due[1].run();
      }
      now = end;
    },
  };
}
const machine = () => {
  const time = clock();
  const changes = [];
  return { time, changes, h: hover(time, (open) => changes.push(open)) };
};

test("a rest of 299 ms leaves it closed, and 300 ms opens it", () => {
  const { time, h } = machine();
  h.enter("spine");
  time.tick(299);
  assert.equal(h.open, false);
  time.tick(1);
  assert.equal(h.open, true);
});

test("leaving before the rest is over opens nothing", () => {
  const { time, h } = machine();
  h.enter("spine");
  time.tick(200);
  h.leave("spine");
  time.tick(1000);
  assert.equal(h.open, false);
});

test("leaving for 399 ms keeps it open, and 400 ms closes it", () => {
  const { time, h } = machine();
  h.enter("spine");
  time.tick(300);
  h.leave("spine");
  time.tick(399);
  assert.equal(h.open, true);
  time.tick(1);
  assert.equal(h.open, false);
});

test("coming back within the delay keeps it open", () => {
  const { time, h } = machine();
  h.enter("spine");
  time.tick(300);
  h.leave("spine");
  time.tick(300);
  h.enter("layer");
  time.tick(5000);
  assert.equal(h.open, true);
});

test("moving from a target into the layer keeps it open", () => {
  const { time, h } = machine();
  h.enter("spine");
  time.tick(300);
  h.enter("layer");
  h.leave("spine");
  time.tick(5000);
  assert.equal(h.open, true);
  h.leave("layer");
  time.tick(400);
  assert.equal(h.open, false);
});

test("a held mouse button never opens it, and a release over a target starts the rest", () => {
  const { time, h } = machine();
  h.pressed(true);
  h.enter("spine");
  time.tick(5000);
  assert.equal(h.open, false);
  h.pressed(false);
  time.tick(299);
  assert.equal(h.open, false);
  time.tick(1);
  assert.equal(h.open, true);
});

test("a button pressed during the rest stops the opening", () => {
  const { time, h } = machine();
  h.enter("header");
  time.tick(200);
  h.pressed(true);
  time.tick(1000);
  assert.equal(h.open, false);
});

test("a click pins it open: leaving does not close it, and a second click does", () => {
  const { time, h } = machine();
  h.click();
  assert.equal(h.open, true);
  h.enter("spine");
  h.leave("spine");
  time.tick(5000);
  assert.equal(h.open, true);
  h.click();
  assert.equal(h.open, false);
});

test("a click on a peek pins it", () => {
  const { time, h } = machine();
  h.enter("spine");
  time.tick(300);
  h.click();
  h.leave("spine");
  time.tick(5000);
  assert.equal(h.open, true);
  assert.equal(h.pinned, true);
});

test("Esc or a click outside closes it, pinned or not", () => {
  const { time, h } = machine();
  h.click();
  h.dismiss();
  assert.equal(h.open, false);
  h.enter("spine");
  time.tick(300);
  h.dismiss();
  assert.equal(h.open, false);
  time.tick(1000);
  assert.equal(h.open, false);
});

test("the pointer resting on a target after a dismissal opens it again", () => {
  const { time, h } = machine();
  h.enter("spine");
  time.tick(300);
  h.dismiss();
  h.leave("spine");
  h.enter("spine");
  time.tick(300);
  assert.equal(h.open, true);
});

test("the key toggles it, and a toggled-open one is pinned", () => {
  const { time, h, changes } = machine();
  h.toggle();
  time.tick(5000);
  assert.equal(h.open, true);
  h.toggle();
  assert.equal(h.open, false);
  assert.deepEqual(changes, [true, false]);
});

test("the shell is told the rail is shown only after 2 seconds open: a peek of 1.9 seconds leaves the turn unseen", () => {
  const time = clock();
  const told = [];
  const gate = shownGate(time, (shown) => told.push(shown));
  gate(true, true);
  time.tick(1900);
  gate(false, true);
  time.tick(5000);
  assert.deepEqual(told, [false]);
  gate(true, true);
  time.tick(1999);
  assert.deepEqual(told, [false]);
  time.tick(1);
  assert.deepEqual(told, [false, true]);
});

test("a rail that is not an overlay is reported at once, and closing is too", () => {
  const time = clock();
  const told = [];
  const gate = shownGate(time, (shown) => told.push(shown));
  gate(true, false);
  gate(true, false);
  gate(false, true);
  assert.deepEqual(told, [true, false]);
});
