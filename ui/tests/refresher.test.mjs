// Staleness bands and the refresher (spec danielrispler/plotkeeper#378, ticket #392): the refresher is drawn by code from the map file, never a
// writer call, and a band's words carry no count. Pure logic, run on the `tsc` output.
import { test } from "node:test";
import assert from "node:assert/strict";
import { tauri } from "./dom.mjs";

const calls = [];
tauri(calls);
const { refresherOf, staleWords } = await import("../../dist/refresher.js");

const map = JSON.stringify({
  version: 1,
  parts: [
    { id: "hook", name: "Hook", caption: "Records every agent event", purpose: "Appends one line per event.", holds: [], tier: "core", paths: ["hook/"] },
    { id: "server", name: "Server", caption: "Reads the log, acts", purpose: "Feeds the core and serves the page.", holds: [], tier: "core", paths: ["server/"] },
    { id: "window", name: "Window", caption: "What you see", purpose: "The page.", holds: [], tier: "core", paths: ["ui/"] },
  ],
  actors: [{ id: "agents", name: "Agents" }],
  links: [{ id: "agents-hook", from: "agents", to: "hook" }, { id: "hook-server", from: "hook", to: "server" }, { id: "server-window", from: "server", to: "window" }],
  ignored: [],
  laws: [
    { id: "log-append", on: "server", backing: "check", text: "The log is only appended." },
    { id: "notice-focus", on: "server", backing: "trust", text: "Notices wait for focus." },
    { id: "hook-held", on: "hook", backing: "trust", text: "A held question blocks the agent." },
  ],
  flows: [
    { id: "turn", name: "Turn writing", hops: [{ link: "agents-hook", text: "An agent's hook fires." }, { link: "hook-server", text: "The hook's line reaches the server." }, { link: "server-window", text: "The server tells the page." }] },
    { id: "other", name: "Hook only", hops: [{ link: "agents-hook", text: "Only the hook." }] },
  ],
});

// Each flow carries its id, which "Explore this" builds its micro-world from (#399).
// Its links are the sheet's "Talks to" (spec #473), so the refresher lists none of its own.
test("the refresher adds to the part's sheet what the map says around it: the flows through it, hop by hop", () => {
  assert.deepEqual(refresherOf(map, "server"), {
    flows: [{ id: "turn", name: "Turn writing", hops: ["The hook's line reaches the server.", "The server tells the page."] }],
  });
  // A map with no flows has none.
  assert.deepEqual(refresherOf(JSON.stringify({ ...JSON.parse(map), flows: undefined }), "hook").flows, []);
});

test("the refresher is drawn with no call to the shell, and a part the map lacks has none", () => {
  calls.length = 0;
  refresherOf(map, "window");
  assert.equal(refresherOf(map, "gone"), null);
  // #548: a file that is JSON but no object does not read.
  for (const text of ["null", "[]", "3", '"parts"']) assert.equal(refresherOf(text, "window"), null, text);
  assert.equal(refresherOf("not json", "server"), null);
  assert.deepEqual(calls, []);
});

test("a band's words say how long, in weeks", () => {
  assert.match(staleWords(1), /3 weeks/);
  assert.match(staleWords(2), /6 weeks/);
  assert.match(staleWords(3), /12 weeks/);
});
