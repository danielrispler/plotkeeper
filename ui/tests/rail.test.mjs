// The rail opens on "now", as a feed of turns (danielrispler/Falinks#574). Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { blockKind, cardTime, feedTurns, foldedLine, headlineOf, nowOf, oneAtATime, turnAt } from "../../dist/story.js";

test("summaries on open are written one at a time, in the order asked, newest first", async () => {
  const started = [];
  const done = [];
  const ask = oneAtATime((id) => {
    started.push(id);
    return new Promise((resolve) => done.push(resolve));
  });
  for (const id of ["newest", "older", "oldest"]) ask(id);
  assert.deepEqual(started, ["newest"]);
  done[0]();
  await new Promise((resolve) => setTimeout(resolve));
  assert.deepEqual(started, ["newest", "older"]);
  done[1]();
  await new Promise((resolve) => setTimeout(resolve));
  assert.deepEqual(started, ["newest", "older", "oldest"]);
});

// #548: a run that fails is caught, and the queue goes on to the next id.
test("a summary whose write throws leaves no unhandled rejection and the next one still runs", async () => {
  const unhandled = [];
  const note = (reason) => unhandled.push(String(reason));
  process.on("unhandledRejection", note);
  const started = [];
  const ask = oneAtATime((id) => {
    started.push(id);
    return id === "bad" ? Promise.reject(new Error("no writer")) : Promise.resolve();
  });
  ask("bad");
  ask("next");
  await new Promise((resolve) => setTimeout(resolve, 5));
  process.off("unhandledRejection", note);
  assert.deepEqual(started, ["bad", "next"]);
  assert.deepEqual(unhandled, []);
});

const html = fs.readFileSync(new URL("../index.html", import.meta.url), "utf8");
const rail = html.slice(html.indexOf('<aside class="rail"'), html.indexOf("</aside>"));
// A turn, a question and a summary as the core sends them, less the fields the feed never reads.
const turn = (started_at, prompt) => /** @type {import("../../dist/types.js").Turn} */ ({ started_at, ended_at: null, prompt, id: null, message: null });
/** @param {Partial<import("../../dist/types.js").Question>} fields */
const question = (fields) => /** @type {import("../../dist/types.js").Question} */ (fields);

test("the rail is the dock, the intention and the feed; the newest card is inside its turn", () => {
  const order = ["dock", "now-asked", "intention", "feed"].map((id) => rail.indexOf(`id="${id}"`));
  assert.ok(order.every((at) => at > 0));
  assert.deepEqual([...order].sort((a, b) => a - b), order);
  assert.ok(!rail.includes('id="now-card"'));
});

test("an event id's time places it in a turn, as a design line's citation of an older card does", () => {
  const turns = [turn(10, "a"), turn(20, "b")];
  assert.equal(cardTime("477b6b89-5461@1793715833094"), 1793715833094);
  assert.equal(turnAt(turns, 5), null);
  assert.equal(turnAt(turns, 10), 0);
  assert.equal(turnAt(turns, 25), 1);
});

test("turns render newest first, each with its subagents and questions", () => {
  const turns = [turn(10, "Fix the parser"), turn(20, "worker w1 returned"), turn(30, "Open the PR")];
  const agents = [/** @type {import("../../dist/types.js").Subagent} */ ({ id: "w1", started_at: 11, turn: 0 })];
  const questions = [question({ tool_use_id: "q1", answer: "not_answered", turn: 1 })];
  const feed = feedTurns(turns, { agents, questions });
  assert.deepEqual(feed.map((b) => b.turn.prompt), ["Open the PR", "worker w1 returned", "Fix the parser"]);
  assert.deepEqual(feed[1].questions.map((q) => q.tool_use_id), ["q1"]);
  assert.deepEqual(feed[2].agents.map((a) => a.id), ["w1"]);
});

test("what started in no turn, such as a subagent's question, goes in one last block", () => {
  const feed = feedTurns([], { agents: [], questions: [question({ tool_use_id: "q1", answer: "not_answered", turn: null })] });
  assert.equal(feed.length, 1);
  assert.equal(feed[0].turn, null);
  assert.deepEqual(feed[0].questions.map((q) => q.tool_use_id), ["q1"]);
  assert.deepEqual(feedTurns([], { agents: [], questions: [] }), []);
});

const summary = /** @type {import("../../dist/types.js").TurnSummary} */ ({ summary: { summary: "Fixed the lint.\nThe test reads the token.\nNot checked: the strip.", quiz: null } });

test("a stored summary is a Headline and reported lines; with none, the final response's first line", () => {
  assert.deepEqual(headlineOf(summary, "Fixed it, mostly."), {
    headline: "Fixed the lint.",
    body: [
      { kind: "reported", text: "The test reads the token." },
      { kind: "reported", text: "Not checked: the strip." },
    ],
  });
  assert.deepEqual(headlineOf(null, "Fixed it, mostly."), { headline: "Fixed it, mostly.", body: [] });
  assert.deepEqual(headlineOf(null, null), { headline: null, body: [] });
});

test("a summary with a Headline and typed lines gives them as they are, and a stripped Headline gives the response's first line", () => {
  const typed = (headline) => /** @type {import("../../dist/types.js").TurnSummary} */ ({
    summary: { summary: "x", headline, lines: [{ kind: "open", text: "Nobody looked." }, { kind: "ask", text: "Merge it?" }], quiz: null },
  });
  assert.deepEqual(headlineOf(typed("Agent says: fixed the overlay."), "Done."), {
    headline: "Agent says: fixed the overlay.",
    body: [
      { kind: "open", text: "Nobody looked." },
      { kind: "ask", text: "Merge it?" },
    ],
  });
  assert.equal(headlineOf(typed(""), "Done.").headline, "Done.");
});

test("what now holds in each state", () => {
  const turn = { prompt: "Fix it", message: "Done.", intention: "Reading the lint" };
  // Running: what was asked and the intention in its box; no Headline yet.
  assert.deepEqual(nowOf("running", turn, null), { asked: "Fix it", intention: "box", headline: null, body: [], soFar: false, fullResponse: false });
  // Needs you: the intention is small, as why it asks, and the turn's facts so far.
  assert.deepEqual(nowOf("needs_you", turn, null), { asked: null, intention: "why", headline: null, body: [], soFar: true, fullResponse: false });
  // Your turn: what was asked, the Headline and Body lines, and the full response; no intention box.
  const done = nowOf("your_turn", turn, summary);
  assert.equal(done.asked, "Fix it");
  assert.equal(done.intention, null);
  assert.equal(done.headline, "Fixed the lint.");
  assert.equal(done.body.length, 2);
  assert.ok(done.fullResponse);
  // Idle and ended read like your turn; with no summary, the response's first line is the Headline.
  for (const state of /** @type {const} */ (["idle", "ended"])) assert.equal(nowOf(state, turn, null).headline, "Done.");
});

test("the block a session needs Daniel for is named by its kind", () => {
  assert.equal(blockKind([], null), null);
  assert.equal(blockKind([{ answer: "open" }], null), "question");
  assert.equal(blockKind([{ answer: "not_answered" }], null), null);
  assert.equal(blockKind([], { tool_name: "ExitPlanMode" }), "plan");
  assert.equal(blockKind([], { tool_name: "Bash" }), "permission");
});

test("a folded turn's line holds the prompt and Headline, and a mark for a failed check", () => {
  const run = (status) => ({ suite: "check", status, retries: 0, runs: [] });
  const line = (checks, headline = "Fixed the parser") => foldedLine({ prompt: "  Fix the parser\nand the lexer", checks }, headline);
  assert.deepEqual(line([]), { prompt: "Fix the parser", headline: "Fixed the parser", failed: false });
  assert.deepEqual(line([run("failed")]), { prompt: "Fix the parser", headline: "Fixed the parser", failed: true });
  // A failure the agent fixed ends as passed, with its retry: no mark.
  assert.equal(line([{ ...run("passed"), retries: 1 }]).failed, false);
  assert.deepEqual(line([run("ran")], null), { prompt: "Fix the parser", headline: null, failed: false });
  assert.equal(foldedLine({ prompt: "", checks: [] }, null).prompt, "No prompt");
});
