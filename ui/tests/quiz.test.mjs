// A turn's Quiz is one open question; the reasons wait for a pick (danielrispler/plotkeeper#118). Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { El } from "./dom.mjs";

// The fake document is set after the import: times.ts starts a timer when a document exists at load.
const { quizElement } = await import("../../dist/story.js");
globalThis.document = { createElement: (tag) => new El(tag), getElementById: () => Object.assign(new El("div"), { replaceChildren() {} }) };

const quiz = {
  text: "What does the frozen gate fix?",
  key: 1,
  options: [
    { text: "The scores", why: "Scores come later." },
    { text: "The pairs", why: "Right, the set of pairs." },
  ],
};

test("the question and options show open, with no fold or label; the reasons wait for a pick", () => {
  const before = quizElement("t1", quiz);
  assert.match(before.text, /What does the frozen gate fix\?/);
  assert.match(before.text, /The scores/);
  assert.doesNotMatch(before.text, /Scores come later/);
  assert.doesNotMatch(before.text, /Learning/);
  assert.equal(before.find("summary").length, 0);

  before.find("button")[0].listeners.click();
  const after = quizElement("t1", quiz);
  assert.match(after.text, /Scores come later\./);
  assert.match(after.text, /The right answer\. Right, the set of pairs\./);
  // A redraw keeps the pick.
  assert.match(quizElement("t1", quiz).text, /Scores come later/);
  // Another turn is untouched.
  assert.doesNotMatch(quizElement("t2", quiz).text, /Scores come later/);
});
