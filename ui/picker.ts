// The picker (#467): where a new session starts. It lists the configured repository's main checkout
// and its tasks, read live from git each time it opens, with `agent-*` worktrees hidden and counted
// on the main checkout. Choosing one starts `claude` there in a new terminal.
// Creating a task by its branch name and removing one (#468) live in Places (#576), at the list's foot.
// The quick start, first: an issue, a spec or a prompt names a new task, and its session starts on it;
// left empty, a session starts in the main checkout with no prompt.

import { element, invoke, pressable, text } from "./dom.js";
import type { Entry } from "./keys.js";
import { startClaude } from "./panes.js";
import { shortPath } from "./paths.js";
import { placeRows } from "./places.js";
import type { Binding } from "./settings.js";
import type { Place, Places } from "./types.js";

// Git's error, or any other, at the picker's foot.
function failed(error: unknown) {
  element("picker").querySelector(".picker-error")?.remove();
  element("picker").append(text("pre", String(error), "picker-error"));
}

// A field whose Enter or button makes a task with `create`, then starts a session there, given
// `prompt` of the field's text as its first prompt. Only a `prompt` field passes an empty text on.
// Errors go to `report`: the picker's foot, or Places' (#576).
export function newTask(
  placeholder: string,
  label: string,
  create: (value: string) => Promise<Place>,
  prompt: boolean,
  report: (error: unknown) => void = failed,
): HTMLElement {
  const form = document.createElement("form");
  form.className = "new-task";
  const field = document.createElement("input");
  field.placeholder = placeholder;
  field.setAttribute("aria-label", placeholder);
  field.spellcheck = false;
  const button = text("button", label, "btn");
  form.append(field, button);
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    const value = field.value.trim();
    if (!value && !prompt) return;
    button.textContent = "Creating…";
    form.inert = true;
    try {
      const task = await create(value);
      element("picker").hidden = true;
      await startClaude(task.path, null, prompt ? value : "");
    } catch (e) {
      report(e);
    } finally {
      button.textContent = label;
      form.inert = false;
    }
  });
  return form;
}

// The quick start: a task named after an issue, a spec or a prompt, whose session starts on it, or,
// empty, the main checkout.
const quickStart = (main: Place) =>
  newTask("Issue #, spec path or prompt; empty for main", "Start", async (request) => request ? invoke<Place>("create_named_task", { request }) : main, true);

// A place's row: its name, then its folder.
function placeRow(name: string, place: Place): HTMLElement {
  const button = pressable(document.createElement("button"), false, () => {
    element("picker").hidden = true;
    startClaude(place.path).catch((e) => console.warn(e));
  });
  button.classList.add("row");
  const first = text("span", "", "l1");
  first.append(text("span", name, "name"));
  const second = Object.assign(text("span", shortPath(place.path), "l2"), { title: place.path }); // #571
  button.append(first, second);
  return button;
}

function picked(places: Places): HTMLElement[] {
  const { hidden } = places;
  const count = hidden ? [text("p", `${hidden} agent worktree${hidden === 1 ? "" : "s"} hidden`, "hidden-count")] : [];
  const [main, ...tasks] = placeRows(places, [], []).map(({ name, place }) => placeRow(name, place));
  return [
    text("h1", "New session in its own task"),
    quickStart(places.main),
    text("h1", "Start a session in"),
    main,
    ...count,
    ...tasks,
  ];
}

// Closes the picker when it shows `label`, and says so; else names it `label`, to open it.
function closed(label: string): boolean {
  const picker = element("picker");
  const shows = !picker.hidden && picker.ariaLabel === label;
  if (shows) picker.hidden = true;
  picker.ariaLabel = label;
  return shows;
}

// The palette (#575), in the picker's box: every action with its keys, filtered by what Daniel types;
// Enter or a click runs one, from where the focus was. Opening it again closes it.
// shortcut: Tab moves between the rows, not ↑ and ↓.
export function showPalette(entries: Entry[], run: (binding: Binding) => void) {
  if (closed("Palette")) return;
  const picker = element("picker");
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
  const draw = () =>
    rows.replaceChildren(
      ...shown().map((entry) => {
        const button = pressable(document.createElement("button"), false, () => choose(entry));
        button.classList.add("row");
        const line = text("span", "", "l1");
        line.append(text("span", entry.title, "name"), text("span", entry.keys.join("  "), "row-state"));
        button.append(line, ...(entry.note ? [text("span", entry.note, "l2")] : []));
        return button;
      }),
    );
  field.addEventListener("input", draw);
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    const first = shown()[0];
    if (first) choose(first);
  });
  draw();
  picker.replaceChildren(form, rows);
  picker.hidden = false;
  field.focus();
}

// Opens the picker, or closes it when open.
export async function showPicker() {
  const picker = element("picker");
  if (closed("New session")) return;
  // It opens once its rows are read, so no placeholder flashes first in another box (#570).
  try {
    picker.replaceChildren(...picked(await invoke<Places>("places")));
  } catch (e) {
    picker.replaceChildren(text("p", `No worktrees: ${e}`, "empty"));
  }
  picker.hidden = false;
  picker.querySelector("input")?.focus();
}
