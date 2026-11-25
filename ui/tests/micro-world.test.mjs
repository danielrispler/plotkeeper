// Micro-worlds on the page (spec #378, ticket #399), run on the `tsc` output: what "Explore this" offers in each phase, and the
// locked-down frame a checked page is shown in.
import { test } from "node:test";
import assert from "node:assert/strict";
import { El, fakeDocument } from "./dom.mjs";

globalThis.document = fakeDocument({ body: new El("body") });
const { exploreOffers, exploreNode, exploreRow, frameNode, paintWorld, worldNodes } = await import("../../dist/micro-world.js");

const view = (phase, extra = {}) => ({ phase, page: false, stale: false, line: null, prompt: null, ...extra });
const worlds = (on, entries = {}) => ({ on, worlds: entries });
const words = (offers) => offers.map((offer) => [offer.label, offer.act]);

test("without microWorlds.dshVisual, Explore this offers only the prompt to copy", () => {
  assert.deepEqual(words(exploreOffers(worlds(false), "flow:turn")), [["Copy prompt", "explore"]]);
  assert.deepEqual(words(exploreOffers(undefined, "flow:turn")), [["Copy prompt", "explore"]], "a payload from before micro-worlds");
  // A page built before the setting was removed still opens.
  assert.deepEqual(words(exploreOffers(worlds(false, { "flow:turn": view("ready", { page: true }) }), "flow:turn")), [["Open", "open"]]);
});

test("Explore this starts a run; a building world waits; a ready one opens; an out-of-date one offers Rebuild", () => {
  assert.deepEqual(words(exploreOffers(worlds(true), "flow:turn")), [["Explore this", "explore"]]);
  const building = exploreOffers(worlds(true, { "flow:turn": view("building") }), "flow:turn");
  assert.deepEqual(words(building), [["Building…", null]]);
  assert.deepEqual(words(exploreOffers(worlds(true, { "flow:turn": view("ready", { page: true }) }), "flow:turn")), [["Open", "open"]]);
  const stale = worlds(true, { "flow:turn": view("ready", { page: true, stale: true }) });
  assert.deepEqual(words(exploreOffers(stale, "flow:turn")), [["Open", "open"], ["Rebuild", "explore"]]);
  // A rebuild that runs keeps the older page to open.
  assert.deepEqual(words(exploreOffers(worlds(true, { "flow:turn": view("building", { page: true, stale: true }) }), "flow:turn")), [["Open", "open"], ["Building…", null]]);
});

test("a world that failed twice offers its prompt to copy and another try, and says why", () => {
  const failed = worlds(true, { "flow:turn": view("failed", { line: "verify.mjs failed", prompt: "In the repository…" }) });
  assert.deepEqual(words(exploreOffers(failed, "flow:turn")), [["Copy prompt", "copy"], ["Try again", "explore"]]);
  const calls = [];
  const node = /** @type {any} */ (exploreNode(failed, "flow:turn", { explore: () => calls.push("explore"), open: () => calls.push("open"), copy: (prompt) => calls.push(["copy", prompt]) }));
  assert.match(node.text, /verify\.mjs failed/);
  node.find("button")[0].listeners.click();
  assert.deepEqual(calls, [["copy", "In the repository…"]]);
});

test("an out-of-date world says it may be out of date", () => {
  const node = /** @type {any} */ (exploreNode(worlds(true, { "flow:turn": view("ready", { page: true, stale: true }) }), "flow:turn", { explore() {}, open() {}, copy() {} }));
  assert.match(node.text, /may be out of date/);
});

test("the page is shown by srcdoc in a frame with allow-scripts and never allow-same-origin", () => {
  const html = `<!doctype html><meta http-equiv="Content-Security-Policy" content="default-src 'none'"><p>world</p>`;
  const frame = /** @type {any} */ (frameNode(html));
  assert.equal(frame.tag, "iframe");
  assert.equal(frame.getAttribute("sandbox"), "allow-scripts");
  assert.equal(frame.getAttribute("srcdoc"), html);
  assert.equal(frame.getAttribute("src"), null, "never a URL of plotkeeper's");
  assert.equal(frame.getAttribute("referrerpolicy"), "no-referrer");
  // The overlay holds that frame, and no other way in.
  const nodes = /** @type {any} */ (worldNodes("A turn", html, view("ready", { page: true }), { rebuild() {}, close() {} }));
  const frames = nodes.flatMap((node) => node.find("iframe"));
  assert.equal(frames.length, 1);
  for (const each of frames) assert.doesNotMatch(each.getAttribute("sandbox"), /allow-same-origin/);
});

test("the overlay of an out-of-date world offers Rebuild beside its ×, named Close (#571)", () => {
  const calls = [];
  const nodes = /** @type {any} */ (worldNodes("A turn", "<p>x</p>", view("ready", { page: true, stale: true }), { rebuild: () => calls.push("rebuild"), close: () => calls.push("close") }));
  const buttons = nodes.flatMap((node) => node.find("button"));
  assert.deepEqual(buttons.map((b) => b.text), ["Rebuild", "×"]);
  assert.equal(buttons[1].getAttribute("aria-label"), "Close");
  buttons.forEach((b) => b.listeners.click());
  assert.deepEqual(calls, ["rebuild", "close"]);
  assert.match(nodes.map((node) => node.text).join(" "), /may be out of date/);
});

test("Copy prompt shows its outcome at once, and the prompt to copy by hand when the clipboard refuses it, though the map is unchanged", async () => {
  const failed = worlds(true, { "flow:turn": view("failed", { line: "verify.mjs failed", prompt: "In the repository…" }) });
  const row = /** @type {any} */ (exploreRow("/w/app", failed, "flow:turn", "A turn", { kind: "flow", id: "turn" }));
  // No clipboard here, as when a browser refuses it; nothing on the server changes, so the map is not drawn again.
  const copy = () => row.find("button").find((b) => b.text === "Copy prompt").listeners.click();
  copy();
  await new Promise((done) => setTimeout(done, 10));
  assert.match(row.text, /Copy this prompt and paste it into your agent/);
  assert.deepEqual(row.find("textarea").map((area) => area.value), ["In the repository…"]);
  // A second copy shows one note, not two.
  copy();
  await new Promise((done) => setTimeout(done, 10));
  assert.equal(row.find("textarea").length, 1);
});

test("a map drawn while a world is open redraws its head and keeps its frame, so the world keeps its state", () => {
  const box = new El("section");
  const act = { rebuild() {}, close() {} };
  paintWorld(box, "A turn", "<p>x</p>", view("ready", { page: true }), act);
  const [frame] = box.find("iframe");
  paintWorld(box, "A turn", "<p>x</p>", view("ready", { page: true, stale: true }), act);
  assert.match(box.text, /may be out of date/, "the head follows the map");
  assert.equal(box.find("iframe").length, 1);
  assert.equal(box.find("iframe")[0], frame, "the same frame, not a new one that reloads the page");
  // Another page is a new frame.
  paintWorld(box, "A turn", "<p>y</p>", view("ready", { page: true }), act);
  assert.notEqual(box.find("iframe")[0], frame);
});

test("a copy says its line once, in the page's action region; the row's note is drawn again with the map and is not live (#465)", async () => {
  const all = (node) => [node, ...node.childNodes.filter((child) => child instanceof El).flatMap(all)];
  const statuses = (node) => all(node).filter((each) => each.getAttribute("role") === "status");
  const failed = worlds(true, { "flow:world-465": view("failed", { line: "verify.mjs failed", prompt: "In the repository…" }) });
  const row = /** @type {any} */ (exploreRow("/w/app", failed, "flow:world-465", "A turn", { kind: "flow", id: "world-465" }));
  row.find("button").find((b) => b.text === "Copy prompt").listeners.click();
  await new Promise((done) => setTimeout(done, 10));
  assert.match(row.text, /Copy this prompt and paste it into your agent/);
  assert.deepEqual(statuses(row), [], "the row's note is not live");
  const [live] = statuses(document.body);
  assert.ok(live, "the page has an action region");
  assert.equal(live.text, "", "the region is empty before the words");
  await new Promise((done) => setTimeout(done, 150));
  assert.equal(live.text, "Copy this prompt and paste it into your agent");
  // The map drawn again draws the row and its note anew; the region keeps its node and says nothing new.
  const again = /** @type {any} */ (exploreRow("/w/app", failed, "flow:world-465", "A turn", { kind: "flow", id: "world-465" }));
  assert.match(again.text, /Copy this prompt and paste it into your agent/);
  assert.deepEqual(statuses(again), []);
  assert.deepEqual(statuses(document.body), [live]);
  assert.equal(live.text, "Copy this prompt and paste it into your agent");
});
