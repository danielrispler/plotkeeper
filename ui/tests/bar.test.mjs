// The top bar's placing (danielrispler/plotkeeper#127): the session header, the needs-you and unseen chips and the git line sit left of the
// spacer, the rest right of it, after every placing. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";

// A stand-in bar: elements that move between the bar and the menu, as `insertBefore` and `append` move them in a browser.
class Node {
  hidden = false;
  children = [];
  parent = null;
  constructor(id, offsetWidth = 0) {
    this.id = id;
    this.offsetWidth = offsetWidth;
  }
  detach() {
    if (this.parent) this.parent.children.splice(this.parent.children.indexOf(this), 1);
  }
  insertBefore(node, before) {
    node.detach();
    this.children.splice(this.children.indexOf(before), 0, node);
    node.parent = this;
  }
  append(node) {
    node.detach();
    this.children.push(node);
    node.parent = this;
  }
  querySelector(selector) {
    return this.children.find((child) => child.id === selector);
  }
  hidePopover() {}
}

const widths = { "session-header": 260, "needs-you": 90, unseen: 80, "git-line": 400, usage: 120, "new-build": 130, "new-session": 28 };
const top = new Node(".top");
for (const id of ["session-header", "needs-you", "unseen", "git-line", ".sp", "usage", "new-build", "new-session", "bar-more", "settings-gear", "bar-menu"])
  top.append(new Node(id, widths[id] ?? 28));
const nodes = Object.fromEntries(top.children.map((node) => [node.id, node]));
nodes["new-build"].hidden = true;
// No session is unseen, so its chip is hidden and stays on its side.
nodes.unseen.hidden = true;

globalThis.document = { querySelector: () => top, getElementById: (id) => nodes[id] };
globalThis.getComputedStyle = () => ({ columnGap: "12", paddingLeft: "78", paddingRight: "12" });
const { placeBar } = await import("../../dist/bar.js");

const inBar = () => top.children.map((node) => node.id).filter((id) => !nodes[id].hidden && id !== "bar-menu");

test("the header, the chips and the git line stay left of the spacer once the bar is placed", () => {
  top.clientWidth = 1728;
  placeBar();
  assert.deepEqual(inBar(), ["session-header", "needs-you", "git-line", ".sp", "usage", "new-session", "settings-gear"]);
});

test("an item back from the menu returns to its side of the spacer", () => {
  top.clientWidth = 1020;
  placeBar();
  assert.deepEqual(inBar(), ["session-header", "needs-you", "git-line", ".sp", "new-session", "bar-more", "settings-gear"]);
  top.clientWidth = 1728;
  placeBar();
  assert.deepEqual(inBar(), ["session-header", "needs-you", "git-line", ".sp", "usage", "new-session", "settings-gear"]);
});

test("the gear is the bar's last item and takes its room: it never goes into the menu, the usage does instead (#194)", () => {
  // Room for everything but the gear: 898 px of items and four gaps of 12 fit in 946, the gear's 28 and a gap more do not.
  top.clientWidth = 946 + 78 + 12 + 24; // plus the bar's padding and the spacer's two gaps
  placeBar();
  assert.deepEqual(inBar(), ["session-header", "needs-you", "git-line", ".sp", "new-session", "bar-more", "settings-gear"]);
  assert.ok(nodes["usage"].parent === nodes["bar-menu"], "the usage is in the menu");
  assert.ok(nodes["settings-gear"].parent === top, "the gear is in the bar");
  top.clientWidth = 1728;
  placeBar();
});
