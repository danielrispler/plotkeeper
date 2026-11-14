// The learning section asks its question before it shows the main idea (danielrispler/plotkeeper#10). Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";

// A just-enough DOM: elements that hold children and text, and run their click listeners.
class Node {
  constructor(tag) {
    this.tag = tag;
    this.children = [];
    this.listeners = {};
    this.classList = { add() {} };
    this.className = "";
  }
  append(...items) {
    this.children.push(...items);
  }
  prepend(...items) {
    this.children.unshift(...items);
  }
  setAttribute() {}
  addEventListener(name, run) {
    this.listeners[name] = run;
  }
  set textContent(value) {
    this.children = [value];
  }
  get text() {
    return this.children.map((c) => (typeof c === "string" ? c : c.text)).join("");
  }
  find(tag, out = []) {
    if (this.tag === tag) out.push(this);
    for (const c of this.children) if (typeof c !== "string") c.find(tag, out);
    return out;
  }
}

// The fake document is set after the import: times.ts starts a timer when a document exists at load.
const { learningElement } = await import("../../dist/story.js");
globalThis.document = { createElement: (tag) => new Node(tag), getElementById: () => Object.assign(new Node("div"), { replaceChildren() {} }) };

const learning = {
  idea: "A gate frozen before scoring fixes the pairs.",
  question: {
    text: "What does the frozen gate fix?",
    key: 1,
    options: [
      { text: "The scores", why: "Scores come later." },
      { text: "The pairs", why: "Right, the set of pairs." },
    ],
  },
};

test("the question and options come first; the main idea and reasons wait for a pick", () => {
  const before = learningElement("t1", learning);
  assert.match(before.text, /What does the frozen gate fix\?/);
  assert.match(before.text, /The scores/);
  assert.doesNotMatch(before.text, /frozen before scoring/);
  assert.doesNotMatch(before.text, /Scores come later/);
  // The folded label says a question, not the idea.
  assert.equal(before.find("summary")[0].text, "Learning: one question");

  before.find("button")[0].listeners.click();
  const after = learningElement("t1", learning);
  assert.match(after.text, /A gate frozen before scoring fixes the pairs\./);
  assert.match(after.text, /Scores come later\./);
  assert.match(after.text, /The right answer\. Right, the set of pairs\./);
  // A redraw keeps the pick.
  assert.match(learningElement("t1", learning).text, /frozen before scoring/);
  // Another turn is untouched.
  assert.doesNotMatch(learningElement("t2", learning).text, /frozen before scoring/);
});
