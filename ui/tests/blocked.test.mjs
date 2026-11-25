// A blocked command's card (danielrispler/plotkeeper#170, #171): its text and buttons by status. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { blockedView, docksInDock, outputCall, stateInDock } from "../../dist/blocked.js";

const card = (status = "blocked", agent = null) => ({
  call: "toolu_1",
  command: 'claude -p "summarise" > /tmp/out.txt',
  cwd: "/work/repo",
  reason: "Nested agent runs are blocked.",
  agent,
  at: 1,
  status,
});

test("a blocked command shows in full with its folder and rule, and Allow and Decline, also after its hook has returned", () => {
  assert.deepEqual(blockedView(card(), null, true), {
    state: "Blocked",
    command: 'claude -p "summarise" > /tmp/out.txt',
    folder: "/work/repo",
    reason: "Nested agent runs are blocked.",
    agent: null,
    buttons: ["Allow", "Decline"],
    detail: null,
    tail: null,
    risk: { state: "reading", level: null, line: null, text: "Reading the command…" },
  });
});

test("a subagent's block names the subagent", () => {
  assert.equal(blockedView(card("blocked", "a-7aaaa"), "Reviewer", true).agent, "Reviewer");
});

test("with no request file there is nothing to answer", () => {
  assert.deepEqual(blockedView(card(), null, false).buttons, []);
});

test("a card docks exactly when it shows a button, for each status, with and without a standing request", () => {
  const statuses = ["blocked", "allowed", "allowed_not_run", "running", "finished", "running_in_background", "blocked_again", "declined"];
  const docked = (answerable) => statuses.filter((status) => docksInDock(card(status), answerable));
  assert.deepEqual(docked(true), ["blocked", "allowed_not_run", "blocked_again"]);
  // Ask again needs no request file; Allow and Decline do.
  assert.deepEqual(docked(false), ["allowed_not_run"]);
  for (const status of statuses) for (const answerable of [true, false]) {
    assert.equal(docksInDock(card(status), answerable), (blockedView(card(status), null, answerable)?.buttons.length ?? 0) > 0, `${status} ${answerable}`);
  }
});

test("a docked card shows its state when it is Blocked again or Allowed, not run, and a plain Blocked card keeps the dock's look", () => {
  assert.deepEqual(["blocked", "blocked_again", "allowed_not_run"].map(stateInDock), [false, true, true]);
});

test("an allowed command waits for the agent's retry with no buttons, and a declined one is closed", () => {
  const allowed = blockedView(card("allowed"), null, true);
  assert.equal(allowed.state, "Allowed");
  assert.deepEqual(allowed.buttons, []);
  assert.equal(blockedView(card("declined"), null, true), null);
});

test("the card has no edit: it offers only Allow and Decline", () => {
  const all = ["blocked", "allowed"].flatMap((s) => blockedView(card(s), null, true).buttons);
  assert.deepEqual(all.filter((b) => !["Allow", "Decline"].includes(b)), []);
});

test("a running command shows the time since its retry started", () => {
  const view = blockedView({ ...card("running"), run: "toolu_2", run_at: 10_000 }, null, false, 75_000);
  assert.equal(view.state, "Running");
  assert.equal(view.detail, "1m");
  assert.deepEqual(view.buttons, []);
});

test("a finished command shows its exit code and the tail of its output, as a failed check does", () => {
  const done = { ...card("finished"), run: "toolu_2", exit_code: 2 };
  const view = blockedView(done, null, false, 0, { exit_code: 2, tail: "line one\nboom" });
  assert.equal(view.state, "Finished");
  assert.equal(view.detail, "exit code 2");
  assert.equal(view.tail, "line one\nboom");
  assert.equal(blockedView(done, null, false, 0).tail, null, "not read yet");
  assert.equal(blockedView(done, null, false, 0, null).tail, "No output was recorded.");
  assert.equal(blockedView(done, null, false, 0, { exit_code: 0, tail: "" }).tail, "No output was recorded.");
  assert.equal(blockedView({ ...done, exit_code: 0 }, null, false, 0, null).detail, "exit code 0");
  assert.equal(blockedView({ ...done, exit_code: null }, null, false, 0, null).detail, "No exit code");
});

test("a finished background run has no tail and reads no output; a foreground run's is read under the agent that ran it", () => {
  const bg = { ...card("finished"), run: "toolu_2", task: "bash-7", exit_code: 0 };
  assert.equal(outputCall(bg), null);
  assert.equal(blockedView(bg, null, false, 0, { exit_code: null, tail: "" }).tail, null);
  const parent = { ...card("finished", "a-7aaaa"), run: "toolu_2", run_by: null, exit_code: 0 };
  assert.deepEqual(outputCall(parent), { agent: null, call: "toolu_2" });
  assert.deepEqual(outputCall({ ...parent, run_by: "a-7aaaa" }), { agent: "a-7aaaa", call: "toolu_2" });
  assert.equal(outputCall({ ...parent, status: "running" }), null);
});

test("a command in the background reads so until its wake-up", () => {
  const view = blockedView({ ...card("running_in_background"), task: "bash-7" }, null, false, 0);
  assert.equal(view.state, "Running in background");
  assert.deepEqual(view.buttons, []);
});

test("a command blocked again offers Allow and Decline once more", () => {
  const view = blockedView(card("blocked_again"), null, true);
  assert.equal(view.state, "Blocked again");
  assert.deepEqual(view.buttons, ["Allow", "Decline"]);
  assert.deepEqual(blockedView(card("blocked_again"), null, false).buttons, []);
});

import { retryLine } from "../../dist/blocked.js";

test("a command allowed but not run says so and offers Ask again, and nothing else", () => {
  const view = blockedView(card("allowed_not_run"), null, false);
  assert.equal(view.state, "Allowed, not run");
  assert.deepEqual(view.buttons, ["Ask again"]);
});

test("the session's own block is retried with a line that names the command", () => {
  assert.equal(
    retryLine(card(), null),
    'Daniel approved the blocked command: `claude -p "summarise" > /tmp/out.txt`. Run it again.',
  );
});

test("a subagent's block tells the session to cd into its folder in its own call, then run the exact command", () => {
  assert.equal(
    retryLine(card("blocked", "a-7aaaa"), "Reviewer"),
    'Daniel approved the command subagent `Reviewer` was blocked on. Run `cd /work/repo` as its own Bash call, then run exactly: `claude -p "summarise" > /tmp/out.txt`. Or pass it to the subagent if it still runs.',
  );
});

import { blockedRequestFor, sendBlockedAnswer, startWatching } from "../../dist/answer.js";

// The shell as `invoke` and `listen` see it: `answers` is what `answer_blocked_command` returns, or an error to throw.
function shell(answers) {
  const handlers = {};
  globalThis.document = { addEventListener() {}, visibilityState: "hidden" };
  globalThis.setInterval = () => 0;
  globalThis.window = {
    __TAURI__: {
      core: {
        invoke: async (command) => {
          if (command !== "answer_blocked_command") return null;
          const next = answers.shift();
          if (next instanceof Error) throw next;
          return next;
        },
      },
      event: { listen: async (name, handler) => void (handlers[name] = (payload) => handler({ payload })) },
    },
  };
  startWatching(() => {});
  return handlers;
}

test("the answer says whether the hook took it or it was late, which decides whether the retry line is typed", async () => {
  const handlers = shell(["hook", "late"]);
  const request = (nonce) => ({ nonce, tool_use_id: `toolu_${nonce}`, waiting: true });
  handlers.blocked([request("n1"), request("n2")]);
  assert.equal(await sendBlockedAnswer("s1", request("n1"), "allow"), "hook");
  assert.equal(await sendBlockedAnswer("s1", request("n2"), "allow"), "late");
});

test("a failed answer puts the request back so the card keeps its buttons", async () => {
  const handlers = shell([new Error("no such request")]);
  const original = { nonce: "n1", tool_use_id: "toolu_n1", waiting: true };
  handlers.blocked([original]);
  const origWarn = console.warn;
  console.warn = () => {};
  const took = await sendBlockedAnswer("s1", original, "allow");
  console.warn = origWarn;
  assert.equal(took, null);
  assert.deepEqual(blockedRequestFor("toolu_n1"), original);
});

// The risk note (danielrispler/plotkeeper#298).
import { riskLevels } from "../../dist/blocked.js";

const arrived = (level, line = "Deletes the remote branch feature-x on GitHub") => ({ call: "toolu_1", state: "arrived", level, line, at: 5 });

test("a card with buttons has the note's place from the start, which reads that the note is on its way, with no note sent yet", () => {
  const view = blockedView(card(), null, true);
  assert.deepEqual(view.risk, { state: "reading", level: null, line: null, text: "Reading the command…" });
  assert.deepEqual(blockedView(card(), null, true, 0, undefined, { call: "toolu_1", state: "reading" }).risk, view.risk);
});

test("a note shows at each of the six levels as 'Risk: Level · line', in plain text", () => {
  const levels = { none: "None", low: "Low", medium: "Medium", high: "High", very_high: "Very high", not_sure: "Not sure" };
  assert.deepEqual(riskLevels, levels);
  for (const [level, label] of Object.entries(levels)) {
    const risk = blockedView(card(), null, true, 0, undefined, arrived(level)).risk;
    assert.deepEqual(risk, { state: "arrived", level: label, line: "Deletes the remote branch feature-x on GitHub", text: `Risk: ${label} · Deletes the remote branch feature-x on GitHub` }, level);
  }
});

test("a failed note reads that no risk note came, which is not a level", () => {
  const risk = blockedView(card(), null, true, 0, undefined, { call: "toolu_1", state: "failed" }).risk;
  assert.deepEqual(risk, { state: "failed", level: null, line: null, text: "No risk note: the judge did not answer" });
});

test("a card's buttons and state do not depend on its note", () => {
  const notes = [undefined, { call: "toolu_1", state: "reading" }, { call: "toolu_1", state: "failed" }, ...Object.keys(riskLevels).map((level) => arrived(level))];
  for (const status of ["blocked", "allowed", "allowed_not_run", "running", "finished", "blocked_again"]) {
    for (const answerable of [true, false]) {
      const bare = blockedView(card(status), null, answerable);
      for (const note of notes) {
        const view = blockedView(card(status), null, answerable, 0, undefined, note);
        assert.deepEqual({ ...view, risk: null }, { ...bare, risk: null }, `${status} ${answerable} ${note?.state}`);
        assert.equal(docksInDock(card(status), answerable), view.buttons.length > 0);
      }
    }
  }
});

test("a card for which no note is coming, in an ended session, has no place unless a note arrived or failed", () => {
  const ended = (status, note) => blockedView(card(status), null, false, 0, undefined, note, false).risk;
  assert.equal(ended("allowed_not_run"), null);
  assert.equal(ended("blocked_again"), null);
  assert.equal(ended("blocked"), null);
  assert.equal(ended("allowed_not_run", arrived("high")).level, "High");
  assert.equal(ended("allowed_not_run", { call: "toolu_1", state: "failed" }).state, "failed");
  assert.equal(blockedView(card("allowed_not_run"), null, false).risk.state, "reading");
});

test("a card with no button has the place only once there is news of a note, and keeps it", () => {
  assert.equal(blockedView(card("allowed"), null, true).risk, null);
  assert.equal(blockedView(card("blocked"), null, false).risk, null);
  assert.equal(blockedView(card("allowed"), null, true, 0, undefined, arrived("high")).risk.level, "High");
  assert.equal(blockedView(card("finished"), null, false, 0, null, { call: "toolu_1", state: "failed" }).risk.state, "failed");
});

import { fakeDocument } from "./dom.mjs";
import { riskElement } from "../../dist/blocked.js";

test("the note's place is a polite live region whose full text is its hover, so a cut note is still readable", () => {
  globalThis.document = fakeDocument();
  const long = Array.from({ length: 16 }, (_, i) => `word${i}`).join(" ");
  const risk = blockedView(card(), null, true, 0, undefined, arrived("very_high", long)).risk;
  const place = riskElement(risk);
  assert.equal(place.className, "risk-note");
  assert.equal(place.getAttribute("aria-live"), "polite");
  assert.equal(place.title, `Risk: Very high · ${long}`);
  assert.equal(place.text, `Risk: Very high · ${long}`);
  assert.deepEqual(place.find("strong").map((s) => s.text), ["Very high"]);
  // Waiting and failed read as plain text, with no level in bold.
  const waiting = riskElement(blockedView(card(), null, true).risk);
  assert.equal(waiting.text, "Reading the command…");
  assert.deepEqual(waiting.find("strong"), []);
});

test("the place keeps two lines and cuts a longer note at two, with no colour", async () => {
  const fs = await import("node:fs");
  const css = fs.readFileSync(new URL("../../ui/style.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const rule = css.match(/\.risk-note\s*\{([^}]*)\}/)?.[1] ?? "";
  assert.match(rule, /-webkit-line-clamp:\s*2/);
  assert.match(rule, /min-height:\s*calc\(2 \* /);
  assert.doesNotMatch(rule, /color|background/);
});

// A docked card drawn before its note was known gets its place when the note is (an ended session's card, drawn while the previous
// session's notes were held), and a place that stands is filled where it stands.
import { El } from "./dom.mjs";
import { fillCardRisk } from "../../dist/blocked.js";

class Card extends El {
  querySelector(selector) {
    const cls = selector.slice(1);
    return this.childNodes.find((c) => c.className.split(" ").includes(cls)) ?? null;
  }
}
const cardWithReason = () => {
  const block = new Card("article");
  const reason = new El("p");
  reason.className = "description reason";
  const after = new El("p");
  after.className = "answer-buttons";
  block.append(reason, after);
  return { block, reason, after };
};

test("a docked card with no place gets one after the rule when its note is known, and a place that stands keeps its node", () => {
  globalThis.document = fakeDocument();
  const { block, reason, after } = cardWithReason();
  block.after = undefined;
  reason.after = (item) => block.childNodes.splice(block.childNodes.indexOf(reason) + 1, 0, item);
  const risk = blockedView(card("allowed_not_run"), null, false, 0, undefined, arrived("high"), false).risk;
  fillCardRisk(block, risk);
  const place = block.querySelector(".risk-note");
  assert.deepEqual(block.childNodes, [reason, place, after]);
  assert.equal(place.getAttribute("aria-live"), "polite");
  assert.equal(place.text, "Risk: High · Deletes the remote branch feature-x on GitHub");

  fillCardRisk(block, { state: "failed", level: null, line: null, text: "No risk note: the judge did not answer" });
  assert.equal(block.querySelector(".risk-note"), place);
  assert.equal(block.childNodes.length, 3);
  assert.equal(place.text, "No risk note: the judge did not answer");
});

// "Ask again" (#550): the retry line shows only once the approval was renewed.
import { blockedElement } from "../../dist/blocked.js";

// A page with a notice line and a Studio log: `notice` is the line, `logged` the lines the page sent to the log.
function page(invoke = async () => null) {
  const notice = { textContent: "", hidden: true };
  const logged = [];
  globalThis.document = fakeDocument({ getElementById: () => notice });
  globalThis.window = { __TAURI__: { core: { invoke }, log: { warn: async (m) => void logged.push(m), error: async (m) => void logged.push(m) } } };
  return { notice, logged };
}

// Presses "Ask again" on a card allowed but not run, with `ask_again` answering as `renew` does; returns the cards whose retry
// line was shown and the notice line's text.
async function askAgain(renew) {
  const asked = [];
  const { notice } = page(async (command, args) => (asked.push([command, args]), renew()));
  const retried = [];
  const element = blockedElement("s1", card("allowed_not_run"), null, (c) => retried.push(c.call));
  await element.find("button")[0].listeners.click();
  assert.deepEqual(asked, [["ask_again", { session: "s1", command: card().command, cwd: "/work/repo" }]]);
  return { retried, notice: notice.textContent };
}

test("Ask again shows the retry line once the approval is renewed", async () => {
  assert.deepEqual(await askAgain(() => null), { retried: ["toolu_1"], notice: "" });
});

test("Ask again that could not renew the approval shows why, and no retry line", async () => {
  const failed = await askAgain(() => Promise.reject("no session s1"));
  assert.deepEqual(failed, { retried: [], notice: "ask_again: no session s1" });
});

// The late Allow's line (#550): what to tell the session, shown as it is and kept out of the Studio log.
import { retryNotice } from "../../dist/story.js";

test("a late Allow shows the line to tell the session, with no command's name before it, and logs nothing", () => {
  const { notice, logged } = page();
  retryNotice({ state: "working" }, card("allowed"), null);
  assert.equal(notice.textContent, 'Allowed. Tell the session: Daniel approved the blocked command: `claude -p "summarise" > /tmp/out.txt`. Run it again.');
  assert.equal(notice.hidden, false);
  assert.deepEqual(logged, []);
});

test("a late Allow of an ended session says to start it again, and logs nothing", () => {
  const { notice, logged } = page();
  retryNotice({ state: "ended" }, card("allowed"), null);
  assert.equal(notice.textContent, "The session has ended: start it again and ask it to run the approved command.");
  assert.deepEqual(logged, []);
});
