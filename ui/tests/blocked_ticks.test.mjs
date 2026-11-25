// A blocked command's Running card ticks its elapsed time (danielrispler/plotkeeper#181). Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { fakeDocument } from "./dom.mjs";

// The page's one second timer, captured; the clock is ours. Both are set before the import: times.ts starts its timer at load.
let timer = null;
let clock = 100_000;
globalThis.setInterval = /** @type {any} */ (
  (run, ms) => {
    timer = { run, ms };
    return 0;
  }
);
globalThis.document = fakeDocument({ addEventListener() {}, visibilityState: "hidden" });
globalThis.window = /** @type {any} */ ({ __TAURI__: { core: { invoke: async () => null }, event: { listen: async () => {} } } });
Date.now = () => clock;
const { blockedElement } = await import("../../dist/blocked.js");

/** @type {import("../../dist/types.js").BlockedCommand} */
const running = { call: "toolu_1", command: "sleep 90", cwd: "/work/repo", reason: "r", agent: null, at: 1, status: "running", run: "toolu_2", run_at: 100_000 };
// The card as the stand-in DOM holds it.
/** @param {any} card */
const secondLine = (card) => card.find("p")[5];

test("a Running card's elapsed time counts up each second while it shows", () => {
  const card = blockedElement("s1", running, null, () => {});
  const line = secondLine(card);
  line.isConnected = true;
  assert.equal(line.text, "0s");
  assert.ok(timer.ms <= 1000);
  clock += 5000;
  timer.run();
  assert.equal(line.text, "5s");
  clock += 60_000;
  timer.run();
  assert.equal(line.text, "1m");
});

test("the tick lets go of a card that left the page, and leaves a card that is not Running alone", () => {
  const card = blockedElement("s1", { ...running, run_at: clock }, null, () => {});
  const line = secondLine(card);
  line.isConnected = false;
  clock += 1000;
  timer.run();
  line.isConnected = true;
  clock += 7000;
  timer.run();
  assert.equal(line.text, "0s");
  const done = blockedElement("s1", { ...running, status: "finished", run: "toolu_9", exit_code: 0 }, null, () => {});
  const exit = secondLine(done);
  exit.isConnected = true;
  timer.run();
  assert.equal(exit.text, "exit code 0");
});
