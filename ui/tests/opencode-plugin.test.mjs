// plotkeeper's OpenCode plugin (`plugin-opencode/plotkeeper.js`), loaded as OpenCode loads it, with a fake OpenCode
// context and a fake `plotkeeper-hook` that appends each line it gets to a file. These tests run no OpenCode. Run:
// `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { chmodSync, existsSync, mkdtempSync, readFileSync, realpathSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const source = join(import.meta.dirname, "../../plugin-opencode/plotkeeper.js");

/** The plugin rendered as install renders it, its hook a script that appends each line to `lines.jsonl`. */
async function loadPlugin() {
  const bin = realpathSync(mkdtempSync(join(tmpdir(), "opencode-plugin-test-")));
  const out = join(bin, "lines.jsonl");
  writeFileSync(join(bin, "plotkeeper-hook"), `#!/bin/sh\nline=$(cat)\nprintf '%s\\n' "$line" >> "${out}"\n`);
  chmodSync(join(bin, "plotkeeper-hook"), 0o755);
  const rendered = join(bin, "plotkeeper.js");
  writeFileSync(rendered, readFileSync(source, "utf8").replaceAll("__PLOTKEEPER_BIN__", bin));
  const plugin = (await import(pathToFileURL(rendered).href)).default;
  const lines = () => (existsSync(out) ? readFileSync(out, "utf8").trim().split("\n").filter(Boolean).map((l) => JSON.parse(l)) : []);
  return { plugin, lines };
}

/** A fake OpenCode context: sessions by parent, the hooks the plugin adds, and an event stream fed by `emit`. */
function fakeContext(parents) {
  const hooks = {};
  const queue = [];
  let wake = () => {};
  const ctx = {
    location: { directory: "/Users/me/work/hello" },
    session: {
      get: async ({ sessionID }) => ({ id: sessionID, parentID: parents[sessionID] }),
      hook: (name, fn) => (hooks[`session.${name}`] = fn),
    },
    tool: { hook: (name, fn) => (hooks[`tool.${name}`] = fn) },
    event: {
      subscribe: ({ types }) => ({
        async *[Symbol.asyncIterator]() {
          for (;;) {
            while (queue.length) {
              const event = queue.shift();
              if (event === null) return;
              if (types.includes(event.type)) yield event;
            }
            await new Promise((resolve) => (wake = () => resolve(undefined)));
          }
        },
      }),
    },
  };
  const emit = (type, data) => {
    queue.push({ type, data });
    wake();
  };
  const end = () => {
    queue.push(null);
    wake();
  };
  return { ctx, hooks, emit, end };
}

/** Waits until `lines` holds `count` lines, then a little longer, so a line too many shows. */
async function settled(lines, count) {
  const until = Date.now() + 10_000;
  while (lines().length < count && Date.now() < until) await new Promise((r) => setTimeout(r, 25));
  await new Promise((r) => setTimeout(r, 300));
  return lines();
}

const P = "ses_parent";
const result = (inboxID, source, metadata) => ({
  sessionID: P,
  inboxID,
  item: { type: "synthetic", payload: { text: "<background-result>", metadata: { source, ...metadata } }, delivery: "steer" },
});

// #779: a wake-up waits for its turn's first step and carries the subagent results the turn took by then, by
// `inbox.delivered`; a result delivered later in a turn goes on a line of its own; a shell job's result names no one.
test("a wake-up carries the subagent results its turn took, and a later one its own line", async () => {
  const { plugin, lines } = await loadPlugin();
  const { ctx, hooks, emit, end } = fakeContext({ ses_a: P, ses_b: P });
  const stop = plugin.setup(ctx);
  await hooks["session.prompt"]({ sessionID: P, messageID: "msg_0", prompt: { text: "Spawn two." } });
  emit("session.execution.started", { sessionID: P });
  emit("session.execution.succeeded", { sessionID: P });
  // B's and a shell job's results wake the session; A's joins its turn after its first step.
  emit("session.inbox.enqueued", result("in_b", "subagent", { childID: "ses_b", state: "error" }));
  emit("session.inbox.enqueued", result("in_sh", "shell", {}));
  emit("session.execution.started", { sessionID: P });
  emit("session.inbox.delivered", { sessionID: P, inboxID: "in_b" });
  emit("session.inbox.delivered", { sessionID: P, inboxID: "in_sh" });
  emit("session.step.started", { sessionID: P, model: { id: "big-pickle" } });
  emit("session.inbox.enqueued", result("in_a", "subagent", { childID: "ses_a", state: "completed" }));
  emit("session.inbox.delivered", { sessionID: P, inboxID: "in_a" });
  emit("session.execution.succeeded", { sessionID: P });
  // A shell job alone wakes it: the wake-up names no one, and goes at the turn's end when no step came.
  emit("session.inbox.enqueued", result("in_sh2", "shell", {}));
  emit("session.execution.started", { sessionID: P });
  emit("session.inbox.delivered", { sessionID: P, inboxID: "in_sh2" });
  emit("session.execution.succeeded", { sessionID: P });
  end();

  const got = await settled(lines, 7);
  stop();
  const shape = got
    .map((l) => ({ name: l.hook_event_name, sent: l.sent_ms, reported: l.reported }))
    .sort((x, y) => x.sent - y.sent)
    .map(({ name, reported }) => (reported === undefined ? name : `${name} ${JSON.stringify(reported)}`));
  const bError = '[{"subagent_session_id":"ses_b","state":"error"}]';
  const aCompleted = '[{"subagent_session_id":"ses_a","state":"completed"}]';
  assert.deepEqual(new Set(shape), new Set([
    "session.prompt",
    "session.execution.succeeded",
    `session.execution.started ${bError}`,
    `session.inbox.delivered ${aCompleted}`,
    "session.execution.started []",
  ]));
  assert.equal(got.length, 7, JSON.stringify(shape));
});
