// The palette (danielrispler/Falinks#575): every action with its keys, filtered by what Daniel types.

import { element, fullTitle, pressable, text } from "./dom.js";
import type { Entry } from "./keys.js";
import type { Binding } from "./settings.js";

// The palette (danielrispler/Falinks#575), a native popover (#458): every action with its keys, filtered by what Daniel types. The
// filter is a combobox over a listbox: ↑ and ↓ move the active option while the focus stays in the field (#24), announced through
// `aria-activedescendant`. Enter or a click runs one, from where the focus was. Opening it again, Esc, a click outside it and its ×
// close it, and a close returns the focus to where it was before it opened, as no button opened it for the browser to go back to; a click
// outside on something that takes the focus, such as a text field, keeps the focus there.
export function showPalette(entries: Entry[], run: (binding: Binding) => void) {
  const picker = element("palette");
  if (picker.matches(":popover-open")) {
    picker.hidePopover();
    return;
  }
  const back = document.activeElement as HTMLElement | null;
  const choose = (entry: Entry) => {
    picker.hidePopover();
    run(entry.binding);
  };
  const close = pressable(text("button", "×", "palette-close"), null, () => picker.hidePopover());
  close.setAttribute("aria-label", "Close");
  close.title = "Close the palette (esc)";
  const form = document.createElement("form");
  form.className = "new-task";
  const field = Object.assign(document.createElement("input"), { name: "action", autocomplete: "off", placeholder: "Run an action…", spellcheck: false });
  field.setAttribute("aria-label", "Run an action");
  const rows = document.createElement("div");
  rows.id = "palette-rows";
  rows.setAttribute("role", "listbox");
  rows.setAttribute("aria-label", "Actions");
  Object.entries({ role: "combobox", "aria-controls": rows.id, "aria-expanded": "true", "aria-autocomplete": "list" }).forEach(([name, value]) =>
    field.setAttribute(name, value),
  );
  form.append(field, close);
  const shown = () => entries.filter((entry) => entry.title.toLowerCase().includes(field.value.trim().toLowerCase()));
  let highlighted = 0;
  const draw = () => {
    rows.replaceChildren(
      ...shown().map((entry, index) => {
        const button = pressable(document.createElement("button"), null, () => choose(entry));
        button.classList.add("row");
        button.id = `palette-option-${index}`;
        button.tabIndex = -1;
        button.setAttribute("role", "option");
        button.setAttribute("aria-selected", String(index === highlighted));
        button.title = "Run this action (↩)";
        const line = text("span", "", "l1");
        line.append(fullTitle(text("span", entry.title, "name"), entry.title), text("span", entry.keys.join("  "), "row-state"));
        button.append(line, ...(entry.note ? [fullTitle(text("span", entry.note, "l2"), entry.note)] : []));
        return button;
      }),
    );
    const active = rows.children[highlighted];
    if (active) field.setAttribute("aria-activedescendant", active.id);
    else field.removeAttribute("aria-activedescendant");
  };
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
  // Esc is the browser's: it closes the popover, and the window's Esc, which peels the map's layers, does not see it.
  // Each handler is replaced at the next opening; neither returns a value, as `false` from an `on` handler cancels the event.
  picker.onkeydown = (event) => {
    if (event.key === "Escape") event.stopPropagation();
  };
  // Before every close, so the focus is back before a chosen action moves it. Only from inside the palette, or from nowhere after a click
  // outside on something that takes no focus: a click outside that focused a control leaves it there (#452's review).
  picker.onbeforetoggle = (event) => {
    if ((event as ToggleEvent).newState !== "closed") return;
    const now = document.activeElement;
    if (!now || now === document.body || picker.contains(now)) back?.focus();
  };
  picker.showPopover();
  field.focus();
}
