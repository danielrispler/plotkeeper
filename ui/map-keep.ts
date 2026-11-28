// The Keep it current card (#753, "Keep it current"; ticket #766): after Accept, and later from a quiet button in the map's head while a map is
// drawn, two snippets to copy so the person's agents and CI keep the map true: a line for AGENTS.md or CLAUDE.md, and a GitHub Actions job.
// plotkeeper edits no file of theirs. `map.ts` hands the head's box and button here on each draw.

import { copyButton } from "./copy-button.js";
import { hint, pressable, text } from "./dom.js";

/** The line for `AGENTS.md` or `CLAUDE.md`. */
export const agentsLine = "When you change files, update `system-map.json` and run `npx -y plotkeeper map check` until it passes.";

/** A GitHub Actions workflow whose one job runs the check; the checker prints its own problems and exits 1 on a failure. */
export const actionsStep = `name: Map check
on: [pull_request]
jobs:
  map:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
      - uses: actions/setup-node@v7
        with:
          node-version: 22
      - run: npx -y plotkeeper map check`;

// The project whose card is open: Accept's (`askKeep`) or the head button's. One project's at a time; a card is shut by its project's change.
let open: string | null = null;

/** Opens `project`'s card at its next draw: Accept's, or a closed PR's way forward, has just put the map in place. */
export function askKeep(project: string) {
  open = project;
}

/** Wires the head's button: a press opens the shown project's card, or shuts it, and draws again. */
export function startKeep(button: HTMLElement, current: () => string | null, redraw: () => void) {
  button.addEventListener("click", () => {
    const project = current();
    if (project === null) return;
    open = open === project ? null : project;
    redraw();
  });
}

const note = "To keep the map out of git, list system-map.json in .git/info/exclude (it is local to your checkout); the check then runs on your machine only.";

function snippet(heading: string, words: string, code: string, label: string): HTMLElement {
  const section = text("section", "", "map-keep-snippet");
  const head = text("div", "", "map-keep-head");
  head.append(text("h3", heading, "map-keep-title"), copyButton(`Copy ${label}`, `Copy ${label} to paste it yourself`, () => code));
  section.append(head, text("p", words, "map-keep-words"), Object.assign(text("pre", code, "map-keep-code"), { translate: false }));
  return section;
}

/** Draws the card into `box` and shows `button` while `drawn` (a map is on the page); the card is open only for the project asked. */
export function drawKeep(box: HTMLElement, button: HTMLElement, project: string | null, drawn: boolean) {
  const shown = drawn && project !== null && open === project;
  button.hidden = !drawn;
  button.setAttribute("aria-expanded", String(shown));
  box.hidden = !shown;
  if (!shown) {
    delete box.dataset.key;
    return;
  }
  // A draw with the card as it is leaves it alone, so a selection in a snippet survives.
  if (box.dataset.key === project) return;
  box.dataset.key = project;
  const close = pressable(text("button", "Close", "btn quiet small"), null, () => {
    open = null;
    drawKeep(box, button, project, drawn);
  });
  hint(close, "Close this card; the Keep it current button in the map's head opens it again");
  const head = text("div", "", "map-keep-head");
  head.append(text("h2", "Keep it current", "map-keep-heading"), close);
  box.replaceChildren(
    head,
    snippet("For your agents", "Add this line to AGENTS.md or CLAUDE.md:", agentsLine, "the agent line"),
    snippet("For CI", "Save this as a workflow under .github/workflows/, for GitHub Actions:", actionsStep, "the Actions workflow"),
    text("p", note, "map-keep-note"),
  );
}
