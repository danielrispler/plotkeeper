// Plan usage in the top bar (danielrispler/plotkeeper#163): the glance picks the tightest window across agents, and a tile draws
// one agent's windows or the reason it has none. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { El } from "./dom.mjs";

globalThis.document = {
  createElement: (tag) => Object.assign(new El(tag), { dataset: {}, style: {}, classList: { add() {}, toggle() {}, contains: () => false } }),
};
const { glance, showGlance, tile } = await import("../../dist/usage.js");

const now = 1738400000;
const claude = (five, week, extra = {}) => ({
  agent: "claude",
  five_hour: five === null ? null : { used_percentage: five, resets_at: now + 2 * 3600 + 10 * 60 },
  seven_day: week === null ? null : { used_percentage: week, resets_at: now + 3 * 86400 },
  read_at: now - 4 * 60,
  source: "last Claude session",
  reason: null,
  ...extra,
});
const pick = ({ agent, text }) => ({ agent, text });
const failed = (agent, reason) => ({ agent, five_hour: null, seven_day: null, read_at: null, source: null, reason });

const every = (node, out = []) => {
  if (node instanceof El) {
    out.push(node);
    for (const child of node.childNodes) every(child, out);
  }
  return out;
};
const textOf = (node) => (node instanceof El ? node.childNodes.map(textOf).join("") : (node.text ?? ""));
const has = (root, name) => every(root).filter((node) => node.className.split(" ").includes(name));

test("the glance shows the tightest window across agents, with that agent's logo", () => {
  const usages = [claude(62, 30), { ...claude(10, 80), agent: "codex" }];
  assert.deepEqual(pick(glance(usages, now)), { agent: "codex", text: "wk 80%" });
  assert.deepEqual(pick(glance([claude(62, 30)], now)), { agent: "claude", text: "5h 62%" });
});

test("a window shares are rounded down, as the status line docs print them", () => {
  assert.equal(glance([claude(23.5, null)], now).text, "5h 23%");
});

test("with no data the glance reads Usage", () => {
  assert.equal(glance([], now), null);
  assert.equal(glance([failed("claude", "No Claude session has saved its usage yet")], now), null);
  const button = new El("button");
  showGlance(button, [], now);
  assert.equal(textOf(button), "Usage");
  assert.ok(button.title);
});

test("a window past its reset is hidden, so the next tightest shows", () => {
  const spent = claude(90, 41);
  spent.five_hour.resets_at = now - 1;
  assert.deepEqual(pick(glance([spent], now)), { agent: "claude", text: "wk 41%" });
  spent.seven_day.resets_at = now - 1;
  assert.equal(glance([spent], now), null);
});

test("the glance button holds the logo and the text, and a title", () => {
  const button = new El("button");
  showGlance(button, [claude(62, 30)], now);
  assert.equal(textOf(button), "5h 62%");
  assert.equal(has(button, "logo")[0].getAttribute("data-logo"), "claude");
  assert.match(button.title, /5-hour/);
});

test("a tile draws its logo and name, a bar and reset time per window, and the data's age and source", () => {
  const node = tile(claude(62, 30), now);
  assert.equal(has(node, "logo")[0].getAttribute("data-logo"), "claude");
  assert.match(textOf(has(node, "tile-name")[0]), /Claude/);
  const windows = has(node, "usage-window");
  assert.equal(windows.length, 2);
  assert.match(textOf(windows[0]), /5h/);
  assert.match(textOf(windows[0]), /62%/);
  assert.match(textOf(windows[0]), /resets in 2h 10m/);
  assert.equal(has(windows[0], "bar-fill")[0].style.width, "62%");
  assert.match(textOf(windows[1]), /week/);
  assert.match(textOf(windows[1]), /resets in 3d 0h/);
  assert.equal(textOf(has(node, "tile-foot")[0]), "as of 4 min ago · last Claude session");
});

test("a window past its reset is left out of the tile", () => {
  const spent = claude(62, 30);
  spent.five_hour.resets_at = now - 1;
  assert.equal(has(tile(spent, now), "usage-window").length, 1);
});

test("a tile whose read failed shows its reason and no number, bar or age", () => {
  const node = tile(failed("claude", "No Claude session has saved its usage yet"), now);
  assert.equal(textOf(has(node, "tile-reason")[0]), "No Claude session has saved its usage yet");
  assert.equal(has(node, "usage-window").length, 0);
  assert.equal(has(node, "tile-foot").length, 0);
  assert.doesNotMatch(textOf(node), /%/);
});
