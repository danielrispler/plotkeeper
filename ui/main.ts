// The window (#463 pick, mockup C): a top bar, then the session list on the right, the terminal in
// the centre and the story rail on the left. Each region is its own module: `list.ts`, `story.ts`
// with `questions.ts` and `cards.ts`, and the terminal pane, `terminal.ts` (#466). The core
// decides what the window shows; this file wires the shell's events to the regions and lays the
// columns out by the width rule in `layout.ts`.

import { markShown, showCards } from "./cards.js";
import { element, invoke, listen } from "./dom.js";
import { layoutMode } from "./layout.js";
import { showSessions } from "./list.js";
import { show } from "./selection.js";
import { startPanes } from "./panes.js";
import { showPicker } from "./picker.js";
import type { Selection, Session, SessionCards } from "./types.js";

// The terminal's cell, measured from a probe in its font, sets the columns' layout.
function layout() {
  const probe = element("probe");
  const cell = probe.getBoundingClientRect();
  const width = cell.width / (probe.textContent ?? "").length;
  const mode = layoutMode(window.innerWidth, width);
  document.body.dataset.layout = mode;
  element("rail-toggle").hidden = mode !== "overlay";
}
window.addEventListener("resize", layout);
layout();

// ⌘1 opens the list from its strip, ⌘2 the rail as an overlay; Esc closes both and "read wide".
const toggle = (name: string) => document.body.classList.toggle(name);
window.addEventListener("keydown", (event) => {
  if (event.key === "Escape") document.body.classList.remove("wide", "list-open", "rail-open");
  if (event.key === "Escape") element("picker").hidden = true;
  if (event.metaKey && event.key === "1") toggle("list-open");
  if (event.metaKey && event.key === "2") toggle("rail-open");
  if (event.metaKey && ["1", "2"].includes(event.key)) event.preventDefault();
});
element("rail-toggle").addEventListener("click", () => toggle("rail-open"));
element("wide").addEventListener("click", () => document.body.classList.add("wide"));
// Every block in the story opens or closes, except a card's "explain more", whose first open asks the
// writer.
const blocks = () => element("story").querySelectorAll<HTMLDetailsElement>("details:not(.more)");
element("open-all").addEventListener("click", () => blocks().forEach((block) => (block.open = true)));
element("close-all").addEventListener("click", () => blocks().forEach((block) => (block.open = false)));

document.addEventListener("visibilitychange", markShown);

// A login shell in the configured repository, then a pane per session terminal (#467).
startPanes().catch((e) => (element("terminal-body").textContent = `No terminal: ${e}`));
element("new-session").addEventListener("click", showPicker);

listen<Session[]>("sessions", showSessions);
invoke<Session[]>("sessions").then(showSessions);
listen<Selection | null>("selected", show);
invoke<Selection | null>("selected").then(show);
listen<SessionCards>("cards", showCards);
// Claude Code's version changed: re-record the subagent fixture.
listen<string>("notice", (notice) => {
  element("notice").textContent = notice;
  element("notice").hidden = false;
});
invoke<SessionCards>("cards").then(showCards);
