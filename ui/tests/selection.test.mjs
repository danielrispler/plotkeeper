// One selection everywhere (danielrispler/Falinks#567): the Home key and the links into a session's turns. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";

test("the Home key flips between a session and Home (#239)", async () => {
  const { homeToggle } = await import("../../dist/selection.js");
  const listed = [{ id: "a" }, { id: "b" }];
  // From a session it remembers that session and shows Home.
  assert.deepEqual(homeToggle("a", null, listed), { show: "a" });
  // On Home it selects the remembered session.
  assert.deepEqual(homeToggle(null, "a", listed), { select: "a" });
  // From a view with no session selected where Home is not shown (History, a full diff, a new terminal), it shows Home.
  assert.deepEqual(homeToggle(null, "a", listed, false), { show: null });
});

test("on Home with the remembered session gone, the Home key does nothing (#239)", async () => {
  const { homeToggle } = await import("../../dist/selection.js");
  assert.equal(homeToggle(null, "gone", [{ id: "b" }]), null);
  assert.equal(homeToggle(null, null, [{ id: "b" }]), null);
});

test("the session left is remembered on every way to Home, and kept while a session is selected (#226)", async () => {
  const { leftSession } = await import("../../dist/selection.js");
  // Home key, a click on the list's empty space or a Delete all end the selection on none.
  assert.equal(leftSession("a", null, null), "a");
  // The last session left wins over a stale one.
  assert.equal(leftSession("b", null, "a"), "b");
  // Moving between sessions, or staying on Home, keeps what was remembered.
  assert.equal(leftSession("a", "b", "x"), "x");
  assert.equal(leftSession(null, null, "x"), "x");
  assert.equal(leftSession(null, "a", "x"), "x");
});
