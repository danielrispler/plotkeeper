// Focus mode and the needs-you toast (danielrispler/Falinks#573). Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { El, fakeDocument } from "./dom.mjs";
import { questionLine, showClearToast, showToast, stackToasts, toastFor, toastShows } from "../../dist/toast.js";
import { showCopyToast, showNote, watchNotes } from "../../dist/notes.js";
import { studio } from "../../dist/state.js";

const ui = new URL("../", import.meta.url);
const read = (name) => fs.readFileSync(new URL(name, ui), "utf8");

// The stand-in page: elements by id, as index.html has them, and a modal dialog open or not.
const page = new Map();
let modal = false;
// The note's fade falls due only when a test runs the timers.
const timers = new Map();
let nextTimer = 1;
globalThis.window = { setTimeout: (run) => (timers.set(nextTimer, run), nextTimer++), clearTimeout: (id) => timers.delete(id) };
const runTimers = () => {
  for (const [id, run] of [...timers]) {
    timers.delete(id);
    run();
  }
};
// The document's listeners, by event name.
const heard = {};
globalThis.document = fakeDocument({
  addEventListener: (name, run) => (heard[name] = run),
  getElementById: (id) => page.get(id) ?? page.set(id, new El("div")).get(id),
  querySelector: (selector) => (selector === "dialog:modal" && modal ? new El("dialog") : null),
  activeElement: null,
  body: { dataset: { layout: "focus" } },
});

const session = (id, state = "running", questions = []) => ({
  id, state, questions, name: null, folder_name: id, identity: { name: id }, last_step: null,
});
const asking = (id, text) =>
  session(id, "needs_you", [{ tool_use_id: "t", agent: null, answer: "open", questions: [{ question: text, options: [] }] }]);

test("the toast shows in focus mode, where the list is folded, never beside the list or its strip (#386)", () => {
  assert.equal(toastShows("focus"), true);
  assert.equal(toastShows("full"), false);
  assert.equal(toastShows("strip"), false);
});

test("a needs-you in focus mode shows one toast with the session and its question's first line", () => {
  const listed = [session("a"), asking("b", "Which branch?\nIt matters for the merge.")];
  const toast = toastFor(listed, "a", new Set());
  assert.equal(toast.session.id, "b");
  assert.equal(toast.count, 1);
  assert.equal(questionLine(toast.session), "Which branch?");
});

test("several blocks share one toast, with a count", () => {
  const toast = toastFor([asking("b", "One?"), asking("c", "Two?")], "a", new Set());
  assert.equal(toast.session.id, "b");
  assert.equal(toast.count, 2);
});

test("a block in the focused session shows no toast", () => {
  assert.equal(toastFor([asking("a", "Mine?")], "a", new Set()), null);
});

test("a dismissed block stays dismissed; the toast goes once Daniel acts", () => {
  assert.equal(toastFor([asking("b", "One?")], "a", new Set(["b"])), null);
  assert.equal(toastFor([session("b", "your_turn")], "a", new Set()), null);
});

// The rule reversed (#457; #452's "Toasts", decided with Daniel): toasts kept out of the keyboard to keep the terminal's focus, and the
// terminal was archived (#332, #415). Now Undo, each × and each main action are buttons Tab reaches; a pointer press still keeps the
// keyboard's focus where it was, and a real Tab, Enter and press are checked in the snap fixture `toastKeys`.
test("toasts take keyboard focus: each main action is a button beside its ×, and none is out of the tab order (#457)", () => {
  studio.selection = null;
  studio.listed = [asking("b", "Which branch?")];
  showToast();
  const needs = page.get("toast");
  assert.equal(needs.hidden, false);
  showNote({ session: session("c"), count: "", line: "It finished" });
  const note = page.get("note-toast");
  let undone = 0;
  showClearToast(2, () => undone++);
  const clear = page.get("clear-toast");
  showCopyToast("manual", "In PR #1, do it.");
  const copy = page.get("copy-toast");
  for (const [toast, names] of [[needs, ["main", "×"]], [note, ["main", "×"]], [copy, ["Copy", "×"]], [clear, ["Undo", "×"]]]) {
    const buttons = toast.find("button");
    assert.deepEqual(buttons.map((b) => (b.className.includes("toast-main") ? "main" : b.getAttribute("aria-label") === "Dismiss" ? "×" : b.text)), names);
    for (const button of buttons) {
      assert.notEqual(button.tabIndex, -1, `<button> ${button.text} is out of the tab order`);
      // A button beside the ×, never around it: each is the toast's own child.
      assert.ok(toast.childNodes.includes(button), `<button> ${button.text} is nested`);
    }
  }
  // A pointer press keeps focus where it was.
  assert.match(read("toast.ts"), /addEventListener\("pointerdown", \(event\) => event\.preventDefault\(\)\)/);
});

test("the toasts are manual popovers in one region named Notifications, and none is a status (#457)", () => {
  // A named <section> is a region.
  const region = read("index.html").match(/<section id="toasts"[^>]*>([\s\S]*?)<\/section>/);
  assert.ok(region, "a region holds the toasts");
  assert.match(region[0], /aria-label="Notifications"/);
  for (const id of ["toast", "note-toast", "copy-toast", "clear-toast"]) {
    const toast = region[1].match(new RegExp(`<div id="${id}"[^>]*>`))?.[0];
    assert.ok(toast, `#${id} is in the region`);
    assert.match(toast, /popover="manual"/);
    assert.doesNotMatch(toast, /role="status"/);
  }
});

test("no toast shows while a modal dialog is open, and it shows once the dialog closes (#457)", () => {
  studio.selection = null;
  studio.listed = [asking("b", "Which branch?")];
  modal = true;
  showToast();
  assert.equal(page.get("toast").matches(":popover-open"), false);
  modal = false;
  showToast();
  assert.equal(page.get("toast").matches(":popover-open"), true);
});

// A note that came while a modal dialog was open waits for it, however long it stays open, and fades only once it has shown
// (#452's outside review).
test("a note held back by a modal dialog does not fade before it shows", () => {
  const note = page.get("note-toast");
  watchNotes();
  modal = true;
  showNote({ session: session("d"), count: "", line: "Done meanwhile" });
  runTimers();
  assert.equal(note.matches(":popover-open"), false, "no note over the dialog");
  // The dialog closes: its toggle reaches the document, where the toasts' own listener stacks them.
  modal = false;
  heard.toggle?.({ target: null });
  stackToasts();
  assert.equal(note.matches(":popover-open"), true, "the note shows once the dialog closes");
  runTimers();
  assert.equal(note.matches(":popover-open"), false, "then it fades");
});

// Copy for the agent (#493): the fourth toast. A copied prompt shows its first 3 lines and closes after 6 s; a prompt the clipboard
// refused shows whole, to copy by hand, and stays until its ×.
test("the copy toast shows a copied prompt's first 3 lines and closes after 6 s", () => {
  const copy = page.get("copy-toast");
  runTimers();
  showCopyToast("copied", "In PR #12, the brief warns this could break.\nCould break: Fog\nA push clears it.\n- fog: a second push");
  assert.equal(copy.matches(":popover-open"), true);
  assert.match(copy.text, /Prompt copied, paste it into your agent/);
  assert.match(copy.text, /In PR #12, the brief warns this could break\.\s*Could break: Fog\s*A push clears it\./);
  assert.doesNotMatch(copy.text, /a second push/, "only the first 3 lines");
  runTimers();
  assert.equal(copy.matches(":popover-open"), false, "it closes on its own");
});

test("with no clipboard the copy toast holds the whole prompt to copy by hand, and stays", () => {
  const copy = page.get("copy-toast");
  showCopyToast("manual", "In PR #12, do it.\nline 2\nline 3\nline 4");
  assert.match(copy.text, /Copy this prompt and paste it into your agent/);
  const area = copy.find("textarea")[0];
  assert.equal(area?.value, "In PR #12, do it.\nline 2\nline 3\nline 4");
  const button = copy.find("button").find((b) => b.text === "Copy");
  assert.equal(button.hidden, false);
  runTimers();
  assert.equal(copy.matches(":popover-open"), true, "a prompt to copy by hand does not fade");
});
