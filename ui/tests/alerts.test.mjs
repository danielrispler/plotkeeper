// Notices (danielrispler/plotkeeper#331): the tab title carries the unseen count, and a browser notification fires only when no tab
// is visible and focused, its permission asked once. Run: `npm test`.
import { beforeEach, test } from "node:test";
import assert from "node:assert/strict";

// The page's globals, stood in for with only what alerts.ts reads.
/** @type {any} */
const doc = { visibilityState: "visible", hasFocus: () => true };
globalThis.document = doc;
const store = new Map();
globalThis.localStorage = /** @type {any} */ ({ getItem: (k) => store.get(k) ?? null, setItem: (k, v) => store.set(k, String(v)) });
const shown = [];
let asked = 0;
class StandInNotification {
  static permission = "default";
  // biome-ignore lint/suspicious/useAwait: a stand-in for an async API, so a throw in it rejects as the real one's would
  static requestPermission = async () => {
    asked++;
    return "granted";
  };
  constructor(title, options) {
    shown.push([title, options]);
  }
}
globalThis.Notification = /** @type {any} */ (StandInNotification);
const Notification = StandInNotification;
const { titleFor, notifyIfAway, askOnce } = await import("../../dist/alerts.js");

beforeEach(() => {
  doc.visibilityState = "visible";
  doc.hasFocus = () => true;
  Notification.permission = "default";
  shown.length = 0;
  store.clear();
  asked = 0;
});

test("the title carries the unseen count, and none when there is none", () => {
  assert.equal(titleFor(0), "plotkeeper");
  assert.equal(titleFor(3), "(3) plotkeeper");
});

test("a notification fires when the tab is hidden or unfocused, and not when it is visible and focused", () => {
  Notification.permission = "granted";
  assert.equal(notifyIfAway({ title: "Needs you", body: "api" }), false);
  doc.hasFocus = () => false;
  assert.equal(notifyIfAway({ title: "Needs you", body: "api" }), true);
  doc.hasFocus = () => true;
  doc.visibilityState = "hidden";
  assert.equal(notifyIfAway({ title: "Your turn", body: "web" }), true);
  assert.deepEqual(shown.map(([t]) => t), ["Needs you", "Your turn"]);
});

test("no notification without the permission", () => {
  doc.hasFocus = () => false;
  Notification.permission = "denied";
  assert.equal(notifyIfAway({ title: "x", body: "y" }), false);
  assert.equal(shown.length, 0);
});

test("the permission is asked once, from the page", async () => {
  await askOnce();
  await askOnce();
  assert.equal(asked, 1);
  Notification.permission = "granted";
  store.clear();
  await askOnce();
  assert.equal(asked, 1);
});
