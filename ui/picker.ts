// The picker (#467): where a new session starts. It lists the configured repository's main checkout
// and its tasks, read live from git each time it opens, with `agent-*` worktrees hidden and counted
// on the main checkout. Choosing one starts `claude` there in a new terminal.
// Tasks (#468): a name field creates a task and starts a session in it; each task's "Remove" removes
// its worktree, never forced, and first lists what that would lose. Git's errors show as they are.
// The quick start, first: an issue, a spec or a prompt names a new task, and its session starts on it;
// left empty, a session starts in the main checkout with no prompt.

import { element, invoke, pressable, text } from "./dom.js";
import { startClaude } from "./panes.js";
import type { Losses, Place, Places } from "./types.js";

// Git's error, or any other, at the picker's foot.
function failed(error: unknown) {
  element("picker").querySelector(".picker-error")?.remove();
  element("picker").append(text("pre", String(error), "picker-error"));
}

// A field whose Enter or button makes a task with `create`, then starts a session there, given
// `prompt` of the field's text as its first prompt. Only a `prompt` field passes an empty text on.
function newTask(placeholder: string, label: string, create: (value: string) => Promise<Place>, prompt: boolean): HTMLElement {
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
      failed(e);
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
// A task with the branch name Daniel types.
const namedTask = () => newTask("New task: branch name", "Create", (name) => invoke<Place>("create_task", { name }), false);

// What removing a task would lose, and the choice to remove it anyway.
function confirmRemoval(row: HTMLElement, task: Place, name: string, losses: Losses) {
  const box = document.createElement("div");
  box.className = "losses";
  box.append(text("p", `${name} has work that is not on origin:`));
  const listed = (title: string, lines: string[]) => {
    if (!lines.length) return;
    const list = document.createElement("ul");
    list.append(...lines.map((line) => text("li", line, "mono")));
    box.append(text("p", title, "losses-title"), list);
  };
  listed("Uncommitted changes", losses.uncommitted);
  listed("Commits not on origin (they stay on the branch)", losses.unpushed);
  const anyway = pressable(text("button", "Remove anyway"), false, () => remove(row, task, name, true));
  const cancel = pressable(text("button", "Cancel"), false, () => box.remove());
  const buttons = document.createElement("div");
  buttons.append(anyway, cancel);
  box.append(buttons);
  row.querySelector(".losses")?.remove();
  row.append(box);
}

// Removes the task's worktree, or, unless confirmed, asks first when that would lose work, then its
// branch when the main checkout holds it, as "Delete" does.
async function remove(row: HTMLElement, task: Place, name: string, confirmed: boolean) {
  try {
    const losses = await invoke<Losses | null>("remove_task", { path: task.path, confirmed });
    if (losses) return confirmRemoval(row, task, name, losses);
    row.remove();
    if (task.branch) await invoke("delete_merged_branch", { branch: task.branch }).catch((e) => failed(`Branch ${task.branch} kept: ${e}`));
  } catch (e) {
    failed(e);
  }
}

// A task's row, with its "Remove" button.
function taskRow(name: string, task: Place): HTMLElement {
  const row = document.createElement("div");
  row.className = "task";
  const removing = pressable(text("button", "Remove", "remove"), false, () => remove(row, task, name, false));
  removing.setAttribute("aria-label", `Remove ${name}`);
  row.append(placeRow(name, task), removing);
  return row;
}

// A place's row: its name, then its folder.
function placeRow(name: string, place: Place): HTMLElement {
  const button = pressable(document.createElement("button"), false, () => {
    element("picker").hidden = true;
    startClaude(place.path).catch((e) => console.warn(e));
  });
  button.classList.add("row");
  const first = text("span", "", "l1");
  first.append(text("span", name, "name"));
  const second = text("span", place.path, "l2");
  button.append(first, second);
  return button;
}

function picked({ main, tasks, hidden }: Places): HTMLElement[] {
  const folder = (place: Place) => place.path.split("/").pop() ?? place.path;
  const count = hidden ? [text("p", `${hidden} agent worktree${hidden === 1 ? "" : "s"} hidden`, "hidden-count")] : [];
  return [
    text("h1", "New session in its own task"),
    quickStart(main),
    text("h1", "Start a session in"),
    placeRow(`Main checkout${main.branch ? ` · ${main.branch}` : ""}`, main),
    ...count,
    ...tasks.map((task) => taskRow(task.branch ?? `${folder(task)} (detached)`, task)),
    namedTask(),
  ];
}

// Opens the picker, or closes it when open.
export async function showPicker() {
  const picker = element("picker");
  picker.hidden = !picker.hidden;
  if (picker.hidden) return;
  picker.replaceChildren(text("p", "Reading worktrees…", "empty"));
  try {
    picker.replaceChildren(...picked(await invoke<Places>("places")));
    picker.querySelector("input")?.focus();
  } catch (e) {
    picker.replaceChildren(text("p", `No worktrees: ${e}`, "empty"));
  }
}
