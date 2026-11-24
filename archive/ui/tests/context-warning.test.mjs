// Removed in #416 with the context share and its warning: their tests and the share-shaped fixtures. Fragments, kept
// verbatim, grouped by the file each came from.

// From ui/tests/naming.test.mjs:

test("the context share is labelled, and its hover gives the tokens and the status line's own left (danielrispler/Falinks#517, decision 3; #519)", () => {
  const context = { tokens: 202_740, window: 1_000_000, share: 20, left: 80 };
  assert.equal(contextText(context), "context 20%");
  assert.equal(contextTitle(context), "203k of 1,000k tokens used · Claude's status line shows this as 80% left");
  assert.equal(contextText(null), "");
  assert.equal(contextTitle(null), "");
});

test("with no snapshot, the context shows its tokens only and the hover says the window is unknown (danielrispler/Falinks#517, decision 2)", () => {
  const context = { tokens: 84_800, window: null, share: null, left: null };
  assert.equal(contextText(context), "85k");
  assert.equal(contextTitle(context), "85k tokens used · the window size is unknown");
});

// From ui/tests/notes.test.mjs:

import { contextNote, crossings, finishedNote } from "../../dist/notes.js";
import { pastWarning } from "../../dist/list.js";

const session = (id, share, rest = {}) => ({

  transcript: { intention: `${id} did it`, context: share === null ? null : { tokens: 1000, window: 200000, share, left: 100 - share } },

test("a share past the threshold warns, not one at it, and 0 never warns", () => {
  assert.equal(pastWarning({ share: 31 }, 30), true);
  assert.equal(pastWarning({ share: 30 }, 30), false);
  assert.equal(pastWarning({ share: 99 }, 0), false);
  assert.equal(pastWarning({ share: null }, 30), false);
  assert.equal(pastWarning(null, 30), false);
});

test("a session warns once as it passes the threshold, and again only after falling back", () => {
  const armed = new Map();
  const at = (share) => crossings([session("C", share)], 30, armed).map((s) => s.id);
  assert.deepEqual(at(28), []);
  assert.deepEqual(at(31), ["C"]);
  assert.deepEqual(at(45), []);
  // `/compact` brings it back under, which re-arms it.
  assert.deepEqual(at(12), []);
  assert.deepEqual(at(31), ["C"]);
});

test("a session already past when its share first shows raises no note, and warns after falling back", () => {
  const armed = new Map();
  const at = (share) => crossings([session("C", share)], 30, armed).map((s) => s.id);
  assert.deepEqual(at(null), []);
  assert.deepEqual(at(40), []);
  assert.deepEqual(at(20), []);
  assert.deepEqual(at(35), ["C"]);
});

test("an unknown share keeps the session's place, and an ended session is left as it is", () => {
  const armed = new Map([["C", false]]);
  assert.deepEqual(crossings([session("C", null)], 30, armed), []);
  assert.equal(armed.get("C"), false);
  assert.deepEqual(crossings([session("D", 50, { state: "ended" })], 30, armed), []);
  assert.ok(!armed.has("D"));
});

test("arming every session again warns each one already past, once", () => {
  const armed = new Map([["A", false], ["B", false]]);
  const sessions = [session("A", 40), session("B", 10)];
  for (const id of armed.keys()) armed.set(id, true);
  assert.deepEqual(crossings(sessions, 30, armed).map((s) => s.id), ["A"]);
  assert.deepEqual(crossings(sessions, 30, armed), []);
});

test("the context note names the first session, its share and how many passed", () => {
  const one = contextNote([session("Fix parser", 34)], 30);
  assert.equal(one.session.id, "Fix parser");
  assert.equal(one.line, "Context passed 30%: now 34%");
  assert.equal(one.count, "");
  assert.equal(contextNote([session("A", 34), session("B", 40)], 30).count, "2 past 30%");
  assert.equal(contextNote([], 30), null);
});

// From ui/tests/settings.test.mjs:

test("the context warning is a whole percent from 0 to 100, and the removed notifications key is ignored", () => {
  const warning = (value) => reload(initial, { settings: `{ "context_warning": ${value} }`, keymap: null });
  assert.equal(warning(20).settings.context_warning, 20);
  assert.equal(warning(0).settings.context_warning, 0);
  assert.equal(warning(150).settings.context_warning, 100);
  assert.equal(warning(-5).settings.context_warning, 0);
  assert.equal(warning(32.6).settings.context_warning, 33);
  assert.equal(warning(`"high"`).settings.context_warning, 30);
  const old = reload(initial, { settings: `{ "notifications": { "while_focused": true } }`, keymap: null });
  assert.deepEqual(old.errors, []);
});

  const settings = `{ "context_warning": "high", "summaries": { "model": 3 } }`;
  const loaded = reload(initial, { settings, keymap: null });
  assert.equal(loaded.settings.context_warning, 30);
  assert.equal(loaded.settings.summaries.model, "haiku");
  assert.deepEqual(loaded.errors, ["settings.json: summaries.model: expected a string", "settings.json: context_warning: expected a number"]);
  // The other keys still apply.
  assert.equal(reload(initial, { settings: `{ "context_warning": 40, "summaries": { "on_open": "yes" } }`, keymap: null }).settings.context_warning, 40);
