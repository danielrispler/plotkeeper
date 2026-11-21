// Plan usage in the top bar (danielrispler/plotkeeper#163): the glance picks the tightest window across agents, and a tile draws
// one agent's windows or the reason it has none. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { El, fakeDocument } from "./dom.mjs";

globalThis.document = fakeDocument();
const { glance, showGlance, tile } = await import("../../dist/usage.js");

const now = 1738400000;
const claude = (five, week, extra = {}) => ({
  agent: "claude",
  groups: [
    {
      name: "",
      windows: [
        ...(five === null ? [] : [{ label: "5h", used_percentage: five, resets_at: now + 2 * 3600 + 10 * 60 }]),
        ...(week === null ? [] : [{ label: "week", used_percentage: week, resets_at: now + 3 * 86400 }]),
      ],
    },
  ],
  read_at: now - 4 * 60,
  source: "last Claude session",
  reason: null,
  ...extra,
});
const pick = ({ agent, text }) => ({ agent, text });
const failed = (agent, reason) => ({ agent, groups: [], read_at: null, source: null, reason });

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
  spent.groups[0].windows[0].resets_at = now - 1;
  assert.deepEqual(pick(glance([spent], now)), { agent: "claude", text: "wk 41%" });
  spent.groups[0].windows[1].resets_at = now - 1;
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
  spent.groups[0].windows[0].resets_at = now - 1;
  assert.equal(has(tile(spent, now), "usage-window").length, 1);
});

test("a tile whose read failed shows its reason and no number, bar or age", () => {
  const node = tile(failed("claude", "No Claude session has saved its usage yet"), now);
  assert.equal(textOf(has(node, "tile-reason")[0]), "No Claude session has saved its usage yet");
  assert.equal(has(node, "usage-window").length, 0);
  assert.equal(has(node, "tile-foot").length, 0);
  assert.doesNotMatch(textOf(node), /%/);
});

test("the timer's refresh asks only for Claude, the popover's open for every agent", async () => {
  const asked = [];
  globalThis.window = { __TAURI__: { core: { invoke: async (command, args) => (asked.push([command, args.agents]), [claude(62, 30)]) } } };
  const { readUsage, timedAgents } = await import("../../dist/usage.js");
  const button = new El("button");
  const popover = new El("div");
  await readUsage(button, popover, timedAgents);
  assert.deepEqual(asked, [["usage", ["claude"]]]);
  assert.equal(popover.childNodes.length, 0);
  await readUsage(button, popover, null);
  assert.deepEqual(asked[1], ["usage", null]);
  assert.equal(popover.childNodes.length, 1);
});

const deferred = () => {
  let resolve, reject;
  const promise = new Promise((a, b) => ((resolve = a), (reject = b)));
  return { promise, resolve, reject };
};
const nowS = () => Date.now() / 1000;
const fresh = (five, readAt) => ({
  agent: "claude",
  groups: [{ name: "", windows: [{ label: "5h", used_percentage: five, resets_at: nowS() + 3600 }] }],
  read_at: readAt,
  source: "last Claude session",
  reason: null,
});

test("a slow read that resolves after a newer one does not overwrite that agent's newer reading", async () => {
  const slow = deferred();
  const calls = [slow.promise, Promise.resolve([fresh(70, nowS())])];
  globalThis.window = { __TAURI__: { core: { invoke: () => calls.shift() } } };
  const { readUsage } = await import("../../dist/usage.js");
  const button = new El("button");
  const popover = new El("div");
  const first = readUsage(button, popover, null);
  await readUsage(button, popover, ["claude"]);
  assert.equal(textOf(button), "5h 70%");
  slow.resolve([fresh(10, nowS() - 600)]);
  await first;
  assert.equal(textOf(button), "5h 70%");
});

test("a read that fails keeps the old data and rejects nothing", async () => {
  globalThis.window = { __TAURI__: { core: { invoke: () => Promise.reject(new Error("no")) } } };
  const { readUsage } = await import("../../dist/usage.js");
  const button = new El("button");
  const popover = new El("div");
  await readUsage(button, popover, ["claude"]);
  await readUsage(button, popover, null);
  assert.equal(popover.childNodes.length > 0, true);
  assert.doesNotMatch(textOf(popover), /Reading/);
});

test("while the popover's read runs its tiles say Reading and no stale age, then show the new data", async () => {
  const pending = deferred();
  globalThis.window = { __TAURI__: { core: { invoke: () => pending.promise } } };
  const { readUsage, tile } = await import("../../dist/usage.js");
  const popover = new El("div");
  const read = readUsage(new El("button"), popover, null);
  assert.match(textOf(popover), /Reading…/);
  assert.doesNotMatch(textOf(popover), /as of/);
  assert.match(textOf(tile(claude(62, 30), now, true)), /Reading…/);
  assert.match(textOf(tile(failed("codex", "x"), now, true)), /Reading…/);
  pending.resolve([fresh(33, nowS() - 120)]);
  await read;
  assert.doesNotMatch(textOf(popover), /Reading…/);
  assert.match(textOf(popover), /as of 2 min ago/);
});

// Codex and agy (danielrispler/plotkeeper#165): the recorded replies as the workbench's seam gives them (`codex-parsed.json` and
// `agy-parsed.json`, which a Rust test keeps equal to the parsed replies), read at the spike's time.
const recorded = (name) => JSON.parse(fs.readFileSync(new URL(`../../workbench/tests/fixtures/usage/${name}`, import.meta.url), "utf8"));
const spikeNow = Date.parse("2026-11-17T16:31:00Z") / 1000;
const codex = () => recorded("codex-parsed.json");
const agy = () => recorded("agy-parsed.json");
const claudeAt = () => claude(10, 20, { read_at: spikeNow - 60 });
const claudeSpike = () => {
  const usage = claudeAt();
  for (const w of usage.groups[0].windows) w.resets_at = spikeNow + 3600;
  return usage;
};

test("the popover shows a tile per agent that was read, and the glance the tightest window with that agent's logo", async () => {
  globalThis.window = { __TAURI__: { core: { invoke: async () => [claudeSpike(), codex(), agy()] } } };
  const { readUsage } = await import("../../dist/usage.js");
  const realNow = Date.now;
  Date.now = () => spikeNow * 1000;
  try {
    const button = new El("button");
    const popover = new El("div");
    await readUsage(button, popover, null);
    assert.deepEqual(has(popover, "usage-tile").map((t) => has(t, "logo")[0].getAttribute("data-logo")), ["claude", "codex", "agy"]);
    assert.equal(has(button, "logo")[0].getAttribute("data-logo"), "agy");
    assert.equal(textOf(button), "wk 78%");
    assert.match(button.title, /Gemini 4 Argon weekly usage is 78%/);
  } finally {
    Date.now = realNow;
  }
});

test("agy's tile shows its three groups most used first, and no group appears in another agent's tile", () => {
  const node = tile(agy(), spikeNow);
  assert.deepEqual(has(node, "group-name").map(textOf), ["Gemini 4 Argon", "Gemini Models", "Claude and GPT models"]);
  assert.equal(has(node, "usage-window").length, 5);
  assert.match(textOf(has(node, "usage-window")[0]), /week.*78%/);
  for (const other of [tile(codex(), spikeNow), tile(claudeSpike(), spikeNow)]) {
    assert.equal(has(other, "group-name").length, 0);
    assert.doesNotMatch(textOf(other), /Argon|Gemini/);
  }
});

test("a group name shows only when the agent has more than one group", () => {
  assert.equal(has(tile(claude(62, 30), now), "group-name").length, 0);
  const one = agy();
  one.groups = one.groups.slice(0, 1);
  assert.equal(has(tile(one, spikeNow), "group-name").length, 0);
});

test("Codex's free plan shows one 30d bar, and the glance can pick it as the tightest", () => {
  const node = tile(codex(), spikeNow);
  const windows = has(node, "usage-window");
  assert.equal(windows.length, 1);
  assert.match(textOf(windows[0]), /^30d/);
  assert.match(textOf(windows[0]), /resets in 30d/);
  assert.doesNotMatch(textOf(node), /no limit|5h|week/);
  assert.deepEqual(pick(glance([codex()], spikeNow)), { agent: "codex", text: "30d 0%" });
  const busy = codex();
  busy.groups[0].windows[0].used_percentage = 91;
  assert.deepEqual(pick(glance([claudeSpike(), busy, agy()], spikeNow)), { agent: "codex", text: "30d 91%" });
});

test("a failed agy tile shows its reason and no bar", () => {
  const node = tile(failed("agy", "agy's usage could not be read: it did not answer in time"), now);
  assert.match(textOf(has(node, "tile-reason")[0]), /did not answer in time/);
  assert.equal(has(node, "usage-window").length, 0);
});

test("a full read drops the tile of an agent the reply no longer has", async () => {
  const replies = [[fresh(10, nowS()), failed("codex", "x")], [fresh(10, nowS())]];
  globalThis.window = { __TAURI__: { core: { invoke: async () => replies.shift() } } };
  const { readUsage } = await import("../../dist/usage.js");
  const popover = new El("div");
  await readUsage(new El("button"), popover, null);
  assert.equal(popover.childNodes.length, 2);
  await readUsage(new El("button"), popover, null);
  assert.equal(popover.childNodes.length, 1);
});

test("a Claude-only read leaves the other agents' tiles alone", async () => {
  const replies = [[fresh(10, nowS()), failed("codex", "x")], [fresh(11, nowS())]];
  globalThis.window = { __TAURI__: { core: { invoke: async () => replies.shift() } } };
  const { readUsage } = await import("../../dist/usage.js");
  const popover = new El("div");
  await readUsage(new El("button"), popover, null);
  await readUsage(new El("button"), popover, ["claude"]);
  assert.match(textOf(popover), /x/);
});

test("with overlapping popover reads only the newest clears Reading", async () => {
  const first = deferred();
  const second = deferred();
  const calls = [first.promise, second.promise];
  globalThis.window = { __TAURI__: { core: { invoke: () => calls.shift() } } };
  const { readUsage } = await import("../../dist/usage.js");
  const popover = new El("div");
  const a = readUsage(new El("button"), popover, null);
  const b = readUsage(new El("button"), popover, null);
  first.resolve([fresh(20, nowS())]);
  await a;
  assert.match(textOf(popover), /Reading…/);
  second.resolve([fresh(30, nowS())]);
  await b;
  assert.doesNotMatch(textOf(popover), /Reading…/);
});
