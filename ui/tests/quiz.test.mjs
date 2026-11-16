// A turn's Quiz is one open question; one click answers it in place (danielrispler/plotkeeper#118, #120). Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { El } from "./dom.mjs";

// The fake document is set after the import: times.ts starts a timer when a document exists at load.
const { quizAllowed, quizRegion, quizStage, quizView, seedQuiz, quizAnswered } = await import("../../dist/quiz.js");
globalThis.document = { createElement: (tag) => new El(tag), getElementById: () => Object.assign(new El("div"), { replaceChildren() {} }) };

const quiz = {
  text: "What does the frozen gate fix?",
  key: 1,
  options: [
    { text: "The scores", why: "Scores come later." },
    { text: "The pairs", why: "Right, the set of pairs." },
  ],
};
const held = [{ kind: "why", text: "Because the pairs set must not move." }, { kind: "reported", text: "It passes." }];

const make = (id, record = () => {}, explain = async () => "A longer reason.") =>
  quizRegion(id, quiz, held, { record, explain, line: (line) => Object.assign(new El("li"), { childNodes: [{ nodeType: 3, text: line.text }] }) });
const settle = () => new Promise((resolve) => setTimeout(resolve));

test("the Quiz's state before a pick, after a pick and after Show the rest anyway", () => {
  assert.equal(quizStage(null, false), "open");
  assert.equal(quizStage(1, false), "picked");
  assert.equal(quizStage(null, true), "skipped");
  // The first answer counts.
  assert.equal(quizStage(0, true), "picked");
  assert.deepEqual(quizView("open"), { reasons: false, rest: false, chips: false, skip: true });
  assert.deepEqual(quizView("picked"), { reasons: true, rest: true, chips: true, skip: false });
  assert.deepEqual(quizView("skipped"), quizView("picked"));
});

test("no open Quiz beside a decision: not with an ask line, not in Needs you", () => {
  assert.equal(quizAllowed([{ kind: "why", text: "x" }], false), true);
  assert.equal(quizAllowed([{ kind: "ask", text: "Which one?" }], false), false);
  assert.equal(quizAllowed([], true), false);
});

test("the question and options show open; the reasons and the held lines wait for an answer", () => {
  const region = make("a1");
  assert.match(region.text, /What does the frozen gate fix\?/);
  assert.match(region.text, /Show the rest anyway/);
  assert.doesNotMatch(region.text, /Scores come later|pairs set must not move|I get why/);
});

test("a pick reveals the reasons, the held lines and the chips, changing only the Quiz's region", () => {
  const taps = [];
  const evidence = Object.assign(new El("details"), { open: true });
  const rail = new El("ul", {}, []);
  const region = make("a2", (tap) => taps.push(tap));
  const before = region.childNodes.length;
  rail.append(evidence, region);
  const siblings = [...rail.childNodes];

  region.find("button")[1].listeners.click();

  assert.deepEqual(taps, [{ kind: "pick", option: 1 }]);
  assert.deepEqual([...rail.childNodes], siblings);
  assert.equal(evidence.open, true);
  assert.ok(before > 0);
  assert.match(region.text, /Scores come later\./);
  assert.match(region.text, /The right answer\. Right, the set of pairs\./);
  assert.match(region.text, /pairs set must not move/);
  assert.match(region.text, /I get why.*Not yet — explain more.*Key wrong/);
  assert.doesNotMatch(region.text, /Show the rest anyway/);
  assert.equal(quizAnswered("a2"), true);
});

test("Show the rest anyway reveals the same without a pick, and its chips are optional", () => {
  const taps = [];
  const region = make("a3", (tap) => taps.push(tap));
  const button = region.find("button").find((b) => /Show the rest/.test(b.text));
  button.listeners.click();
  assert.deepEqual(taps, [{ kind: "skip" }]);
  assert.match(region.text, /Scores come later\./);
  assert.match(region.text, /pairs set must not move/);
  // Nothing waits for a chip.
  assert.deepEqual(taps.length, 1);
  region.find("button").find((b) => /I get why/.test(b.text)).listeners.click();
  assert.deepEqual(taps.at(-1), { kind: "understood" });
});

test("a revealed Quiz stays revealed when the rail draws it again, and a stored pick reads back", () => {
  make("a4").find("button")[0].listeners.click();
  assert.match(make("a4").text, /Scores come later/);
  // Another turn is untouched.
  assert.doesNotMatch(make("a5").text, /Scores come later/);
  // After a restart the store's taps seed it.
  seedQuiz("a6", [{ kind: "skip", at: 1 }, { kind: "understood", at: 2 }]);
  assert.match(make("a6").text, /Scores come later/);
  assert.equal(make("a6").find("button").find((b) => /I get why/.test(b.text)).disabled, true);
  seedQuiz("a7", [{ kind: "pick", option: 0, at: 1 }]);
  assert.equal(make("a7").find("button")[0].attrs.get("aria-pressed"), "true");
});

test("Not yet — explain more asks the writer once and shows its text under the chips", async () => {
  let asked = 0;
  const region = make("a8", () => {}, async () => (asked++, "A longer reason."));
  region.find("button")[0].listeners.click();
  const chip = () => region.find("button").find((b) => /explain more/.test(b.text));
  chip().listeners.click();
  assert.match(region.text, /Writing…/);
  await settle();
  assert.match(region.text, /A longer reason\./);
  chip().listeners.click();
  assert.equal(asked, 1);
});
