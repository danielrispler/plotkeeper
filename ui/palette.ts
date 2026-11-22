// The palette (danielrispler/Falinks#575): every action with its keys, filtered by what Daniel types.

import { element, fullTitle, pressable, text } from "./dom.js";
import type { Entry } from "./keys.js";
import type { Binding } from "./settings.js";

// The palette (danielrispler/Falinks#575), in its own box: every action with its keys, filtered by what Daniel types;
// ↑ and ↓ move the highlighted row while the focus stays in the field (#24); Enter or a click runs one,
// from where the focus was. Opening it again closes it.
export function showPalette(entries: Entry[], run: (binding: Binding) => void) {
  const picker = element("palette");
  if (!picker.hidden) {
    picker.hidden = true;
    return;
  }
  const back = document.activeElement as HTMLElement | null;
  const choose = (entry: Entry) => {
    picker.hidden = true;
    back?.focus();
    run(entry.binding);
  };
  const form = document.createElement("form");
  form.className = "new-task";
  const field = Object.assign(document.createElement("input"), { placeholder: "Run an action", spellcheck: false });
  field.setAttribute("aria-label", "Run an action");
  form.append(field);
  const rows = document.createElement("div");
  const shown = () => entries.filter((entry) => entry.title.toLowerCase().includes(field.value.trim().toLowerCase()));
  let highlighted = 0;
  const draw = () =>
    rows.replaceChildren(
      ...shown().map((entry, index) => {
        const button = pressable(document.createElement("button"), index === highlighted, () => choose(entry));
        button.classList.add("row");
        button.title = "Run this action (↩)";
        const line = text("span", "", "l1");
        line.append(fullTitle(text("span", entry.title, "name"), entry.title), text("span", entry.keys.join("  "), "row-state"));
        button.append(line, ...(entry.note ? [fullTitle(text("span", entry.note, "l2"), entry.note)] : []));
        return button;
      }),
    );
  field.addEventListener("input", () => {
    highlighted = 0;
    draw();
  });
  field.addEventListener("keydown", (event) => {
    const by = { ArrowDown: 1, ArrowUp: -1 }[event.key];
    if (!by || event.metaKey || event.ctrlKey || event.altKey || event.shiftKey) return;
    event.preventDefault();
    highlighted = Math.max(0, Math.min(shown().length - 1, highlighted + by));
    draw();
    rows.children[highlighted]?.scrollIntoView({ block: "nearest" });
  });
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    const entry = shown()[highlighted];
    if (entry) choose(entry);
  });
  draw();
  picker.replaceChildren(form, rows);
  picker.hidden = false;
  field.focus();
}
