// Finished subagents leave the list; the rail keeps their history (danielrispler/Falinks#535). Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { agentLine, childrenOf, doneCount, drawn, folds, hidden } from "../../dist/list.js";
import { subagentHistory } from "../../dist/story.js";

test("a subagent row has a second line only while it runs or needs you (danielrispler/Falinks#572)", () => {
  assert.equal(agentLine("running", ["Read", null]), "Read");
  assert.equal(agentLine("needs_you", ["Explore", "2m"]), "Explore · 2m");
  assert.equal(agentLine("done", ["Explore", "2m", "Found it"]), null);
  assert.equal(agentLine("failed", ["Explore"]), null);
});

const MIN = 60_000;
const agent = (id, fields = {}) => ({
  id,
  parent: "session",
  kind: "worker",
  name: `Task ${id}`,
  asked: null,
  agent_type: null,
  status: "running",
  started_at: 0,
  ended_at: null,
  result: null,
  failed: false,
  opened: false,
  faded: false,
  ...fields,
});

test("a finished worker draws no row and counts in its parent's \"N done\"", () => {
  const session = {
    agents: [
      agent("w1", { status: "completed", ended_at: MIN, faded: true }),
      agent("w2"),
      agent("w3", { status: "killed", ended_at: MIN, failed: true }),
    ],
  };
  const children = childrenOf(session);
  assert.deepEqual(session.agents.filter((a) => !hidden(children, a)).map((a) => a.id), ["w2", "w3"]);
  assert.equal(doneCount(children, children.get("session")), 1);
});

test("the rail's history lists every subagent newest first, with its purpose, result and details", () => {
  const session = {
    agents: [
      agent("w1", { status: "completed", ended_at: 7 * MIN, faded: true, asked: "Implement #9.", agent_type: "implementer", result: "Done." }),
      agent("h1", { kind: "helper", started_at: MIN, status: "killed", ended_at: 2 * MIN, failed: true }),
      agent("h2", { kind: "helper", started_at: 2 * MIN, agent_type: "Explore" }),
    ],
  };
  const history = subagentHistory(session, new Map(), 5 * MIN);
  assert.deepEqual(history, [
    { agent: "h2", purpose: "Task h2", asked: "", result: "running", details: "Explore · 3m · helper" },
    { agent: "h1", purpose: "Task h1", asked: "", result: "killed", details: "1m · helper" },
    { agent: "w1", purpose: "Task w1", asked: "Implement #9.", result: "Done.", details: "implementer · 7m · worker" },
  ]);
});

test("a written summary takes the place of the first line", () => {
  const session = { agents: [agent("w1", { status: "completed", ended_at: MIN, result: "Done." })] };
  const [entry] = subagentHistory(session, new Map([["w1", "Added the parser; its tests pass."]]), MIN);
  assert.equal(entry.result, "Added the parser; its tests pass.");
});

// A collapsed row hides what it spawned, but not a subagent that needs you or failed unopened.
const tree = {
  asker: { agent: "h2" },
  agents: [
    agent("w1"),
    agent("h1", { kind: "helper", parent: { agent: "w1" } }),
    agent("h2", { kind: "helper", parent: { agent: "w1" } }),
    agent("h3", { kind: "helper", parent: { agent: "w1" }, status: "killed", failed: true }),
    agent("h4", { kind: "helper", parent: { agent: "w1" }, status: "killed", failed: true, opened: true }),
    agent("w2"),
  ],
};
const rows = (folded) => drawn(tree, childrenOf(tree), "session", 1, folded).map(({ agent, depth }) => `${agent.id}@${depth}`);

test("an open row draws its subagents under it", () => {
  assert.deepEqual(rows(new Set()), ["w1@1", "h1@2", "h2@2", "h3@2", "h4@2", "w2@1"]);
});

test("a collapsed row hides its subagents, except one that needs you or failed unopened", () => {
  assert.deepEqual(rows(new Set(["w1"])), ["w1@1", "h2@2", "h3@2", "w2@1"]);
  assert.deepEqual(folds(tree, childrenOf(tree), ["w1"], new Set(["w1"])), { open: false, hiding: 2 });
});

test("a row folds only when it has subagents drawn under it", () => {
  assert.deepEqual(folds(tree, childrenOf(tree), ["w1"], new Set()), { open: true, hiding: 0 });
  assert.equal(folds(tree, childrenOf(tree), ["w2"], new Set()), null);
});
