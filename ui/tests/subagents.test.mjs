// Finished subagents leave the list; the rail keeps their history (#535). Run: `npm test` in
// apps/studio.
import { test } from "node:test";
import assert from "node:assert/strict";
import { agentLine, childrenOf, doneCount, hidden } from "../../dist/list.js";
import { subagentHistory } from "../../dist/story.js";

test("a subagent row has a second line only while it runs or needs you (#572)", () => {
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
