// The part sheet's folds (spec #633, ticket #637): under the catch-up card, the part's purpose clamped to two lines with More/Less, a row
// of chips that each open their fold, and the sections, each a `<details>` that opens on its own (no shared `name`: Laws and Talks to read
// side by side) with a short value beside its title. Which folds are open is kept in this browser's page store, as the sheet's width is;
// a browser that keeps nothing starts with all folded and keeps them for as long as the page lives. `map-sheet.ts` fills each fold.

import { hint, pageStore, pressable, text } from "./dom.js";
import { explainLine, SHEET_FOLDS } from "./explanations.js";
import { type FoldKey, foldTitles } from "./map-model.js";

const kept = pageStore("map-sheet-folds");
// What this page keeps when the browser keeps nothing.
let remembered = new Set<string>();
const keptOpen = (): Set<string> => {
  const found = kept.get();
  if (found === null) return remembered;
  try {
    const keys: unknown = JSON.parse(found);
    return new Set(Array.isArray(keys) ? keys.filter((key): key is string => typeof key === "string") : []);
  } catch {
    return remembered;
  }
};
function keep(key: FoldKey, open: boolean) {
  const now = new Set(keptOpen());
  if (open) now.add(key);
  else now.delete(key);
  remembered = now;
  kept.set(JSON.stringify([...now]));
}

/** A fold as drawn: the `<details>`, its summary (the heading) and the value beside its title, and its body. */
export type Fold = { node: HTMLDetailsElement; summary: HTMLElement; value: HTMLElement; body: HTMLElement };

/**
 * The fold `key`: its title and `glance` in the summary, then, inside, its explanation line while `explain` is on (the summary is
 * described by it) and `content`. Open as last left, or always when `forced` (CI on main while a run breaks this part's law); `bad` says
 * so in the value's tone.
 */
export function fold(key: FoldKey, glance: string, explain: boolean, content: (HTMLElement | string)[], { forced = false, bad = false } = {}): Fold {
  const node = text("details", "", `sheet-fold sheet-fold-${key}`);
  node.dataset.fold = key;
  const summary = text("summary", "", "sheet-fold-summary");
  const value = text("span", glance, `sheet-fold-value${bad ? " bad" : ""}`);
  summary.append(text("span", foldTitles[key], "sheet-fold-title"), value);
  const body = text("div", "", "sheet-fold-body");
  if (explain) {
    const line = explainLine(SHEET_FOLDS[key]);
    summary.setAttribute("aria-describedby", line.id);
    body.append(line);
  }
  body.append(...content);
  node.append(summary, body);
  let drawn = forced || keptOpen().has(key);
  node.open = drawn;
  // Only Daniel's own opening and closing is kept: the toggle the drawn state fires is not.
  node.addEventListener("toggle", () => {
    if (node.open === drawn) return;
    drawn = node.open;
    keep(key, node.open);
  });
  return { node, summary, value, body };
}

/** A chip under the purpose: `label`, and a press that opens `target` and brings it into view. */
export function chip(label: string, target: Fold, bad = false): HTMLElement {
  const button = pressable(text("button", label, `btn quiet sheet-chip${bad ? " bad" : ""}`), null);
  hint(button, `Open ${foldTitles[target.node.dataset.fold as FoldKey]}`);
  button.addEventListener("click", () => {
    target.node.open = true;
    const still = matchMedia("(prefers-reduced-motion: reduce)").matches;
    target.node.scrollIntoView({ block: "nearest", behavior: still ? "auto" : "smooth" });
    target.summary.focus({ preventScroll: true });
  });
  return button;
}

/**
 * The purpose, clamped to two lines while `open` is false, and its More or Less, which flips it in place (so the button keeps the focus)
 * and tells `flipped`, so the sheet's next draw keeps it. The More hides while the purpose fits its two lines (`fitPurpose`, once the
 * sheet is in the page).
 */
export function purposeNodes(purpose: string, open: boolean, flipped: (open: boolean) => void): HTMLElement[] {
  const words = text("p", purpose, "sheet-purpose");
  words.id = "sheet-purpose";
  const more = pressable(text("button", "", "btn quiet sheet-more"), null);
  more.setAttribute("aria-controls", words.id);
  const show = (whole: boolean) => {
    words.className = whole ? "sheet-purpose" : "sheet-purpose clamped";
    more.textContent = whole ? "Less" : "More";
    more.setAttribute("aria-expanded", String(whole));
    hint(more, whole ? "Show the purpose in two lines" : "Show the whole purpose");
  };
  show(open);
  more.addEventListener("click", () => {
    open = !open;
    show(open);
    flipped(open);
  });
  return [words, more];
}

/** Hides the purpose's More while its two lines hold it all; drawn again at every draw and resize of the sheet. */
export function fitPurpose(slot: HTMLElement) {
  const words = slot.querySelector<HTMLElement>(".sheet-purpose.clamped");
  const more = slot.querySelector<HTMLElement>(".sheet-more");
  if (words && more) more.hidden = words.scrollHeight <= words.clientHeight;
}
