// Pull main's flow (#164): a pull's error survives the read that follows it, even when a read was already in flight. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { El, fakeDocument } from "./dom.mjs";

// The document is set after the import: times.ts starts a timer when one exists at load.
const { readWorkspaces, pullAll } = await import("../../dist/workspaces.js");
const els = new Map();
globalThis.document = fakeDocument({
  getElementById: (id) => els.get(id) ?? (els.set(id, new El("div")), els.get(id)),
  visibilityState: "hidden",
  hasFocus: () => false,
});
const calls = [];
let gate = null;
globalThis.window = {
  __TAURI__: {
    core: {
      invoke: async (command, args) => {
        calls.push(command);
        if (command === "projects") return ["/r"];
        if (command === "workspaces") {
          if (gate) await gate;
          return { main: { path: "/r", branch: "main" }, tasks: [], hidden: 0 };
        }
        if (command === "pull_main") throw new Error("fatal: Not possible to fast-forward");
        if (command === "workspace_git_state") return { branch: "main", upstream: [0, 2], changed: 0, main_behind: 2, on_default: true, would_conflict: false, operation: null };
        return null;
      },
    },
  },
};
const textOf = (node) => (node instanceof El ? node.childNodes.map(textOf).join("") : (node.text ?? ""));

test("a pull's error shows after the read that follows it, though a read was in flight, and a later read keeps it", async () => {
  await readWorkspaces(true);
  await new Promise((r) => setTimeout(r, 10));
  let release;
  gate = new Promise((r) => (release = r));
  const flying = readWorkspaces(true);
  const pulling = pullAll();
  await new Promise((r) => setTimeout(r, 10));
  const before = calls.filter((c) => c === "workspaces").length;
  gate = null;
  release();
  await Promise.all([flying, pulling]);
  assert.ok(calls.filter((c) => c === "workspaces").length > before, "a read ran after the pull");
  assert.match(textOf(els.get("groups")), /fast-forward/);
  await readWorkspaces(true);
  assert.match(textOf(els.get("groups")), /fast-forward/);
});
