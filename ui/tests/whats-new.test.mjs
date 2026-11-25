// What's new (#527): after an update to a new version, one toast names it and links to its GitHub Release; a first install and a
// same-version load or a downgrade show nothing, a refusing storage fails nothing, and the version is stored only once the toast showed while
// the tab was visible. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";

const listeners = new Set();
const doc = { visibilityState: "visible", addEventListener: (_, f) => listeners.add(f), removeEventListener: (_, f) => listeners.delete(f) };
globalThis.document = doc;
const store = new Map();
globalThis.localStorage = { getItem: (k) => store.get(k) ?? null, setItem: (k, v) => store.set(k, String(v)) };
const { whatsNew, releaseUrl, startWhatsNew } = await import("../../dist/whats-new.js");

test("a changed version shows the toast, and stores it when the tab is visible", () => {
  assert.deepEqual(whatsNew("0.1.2", "0.2.0", true), { toast: true, store: true });
});

test("no stored version, a first install, stores the current one and shows nothing", () => {
  assert.deepEqual(whatsNew(null, "0.2.0", true), { toast: false, store: true });
  assert.deepEqual(whatsNew(null, "0.2.0", false), { toast: false, store: true });
});

test("the same version shows nothing and stores nothing", () => {
  assert.deepEqual(whatsNew("0.2.0", "0.2.0", true), { toast: false, store: false });
});

test("a changed version shown while the tab is hidden is not stored yet", () => {
  assert.deepEqual(whatsNew("0.1.2", "0.2.0", false), { toast: true, store: false });
});

test("the link is the version's GitHub Release, from its tag", () => {
  assert.equal(releaseUrl("0.2.0"), "https://github.com/danielrispler/plotkeeper/releases/tag/v0.2.0");
});

test("a load in a hidden tab shows the toast and stores the version at the tab's first visible moment, so the next load is quiet", () => {
  const shown = [];
  const show = (version, url) => shown.push([version, url]);
  store.clear();
  startWhatsNew("0.1.2", show);
  doc.visibilityState = "hidden";
  startWhatsNew("0.2.0", show);
  assert.deepEqual(shown, [["0.2.0", "https://github.com/danielrispler/plotkeeper/releases/tag/v0.2.0"]]);
  doc.visibilityState = "visible";
  for (const f of [...listeners]) f();
  startWhatsNew("0.2.0", show);
  assert.equal(shown.length, 1);
  assert.equal(listeners.size, 0);
});

test("a downgrade, an older version than the stored one, shows nothing and stores the current one", () => {
  assert.deepEqual(whatsNew("0.3.0", "0.2.0", true), { toast: false, store: true });
  assert.deepEqual(whatsNew("0.10.0", "0.9.1", false), { toast: false, store: true });
  // Compared as numbers, not text: 0.10.0 is newer than 0.9.1.
  assert.deepEqual(whatsNew("0.9.1", "0.10.0", true), { toast: true, store: true });
});

test("a browser whose storage throws still starts, showing nothing and failing nothing", () => {
  const kept = globalThis.localStorage;
  globalThis.localStorage = {
    getItem: () => {
      throw new Error("blocked");
    },
    setItem: () => {
      throw new Error("blocked");
    },
  };
  const shown = [];
  try {
    assert.doesNotThrow(() => startWhatsNew("0.2.0", (version) => shown.push(version)));
  } finally {
    globalThis.localStorage = kept;
  }
  assert.deepEqual(shown, []);
});
