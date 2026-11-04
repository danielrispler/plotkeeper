// The picker (#467): where a new session starts. It lists the configured repository's main checkout
// and its tasks, read live from git each time it opens, with `agent-*` worktrees hidden and counted
// on the main checkout. Choosing one starts `claude` there in a new terminal.
// Tasks (#468): a name field creates a task and starts a session in it; each task's "Remove" removes
// its worktree, never forced, and first lists what that would lose. Git's errors show as they are.

import { element, invoke, pressable, text } from "./dom.js";
import { startClaude } from "./panes.js";
import type { Losses, Place, Places } from "./types.js";

// Git's error, or any other, at the picker's foot.
function failed(error: unknown) {
  element("picker").querySelector(".picker-error")?.remove();
  element("picker").append(text("pre", String(error), "picker-error"));
}

// The "new task" field: Enter or "Create" makes the task, then starts a session there.
function newTask(): HTMLElement {
  const form = document.createElement("form");
  form.className = "new-task";
  const name = document.createElement("input");
  name.placeholder = "New task: branch name";
  name.setAttribute("aria-label", "New task name");
  name.spellcheck = false;
  const create = text("button", "Create");
  form.append(name, create);
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    if (!name.value.trim()) return;
    create.textContent = "Creating…";
    form.inert = true;
    try {
      const task = await invoke<Place>("create_task", { name: name.value.trim() });
      element("picker").hidden = true;
      await startClaude(task.path);
    } catch (e) {
      failed(e);
    } finally {
      create.textContent = "Create";
      form.inert = false;
    }
  });
  return form;
}

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

// Removes the task's worktree, or, unless confirmed, asks first when that would lose work.
async function remove(row: HTMLElement, task: Place, name: string, confirmed: boolean) {
  try {
    const losses = await invoke<Losses | null>("remove_task", { path: task.path, confirmed });
    if (losses) return confirmRemoval(row, task, name, losses);
    row.remove();
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
  button.className = "row";
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
    text("h1", "Start a session in"),
    placeRow(`Main checkout${main.branch ? ` · ${main.branch}` : ""}`, main),
    ...count,
    ...tasks.map((task) => taskRow(task.branch ?? `${folder(task)} (detached)`, task)),
    newTask(),
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
  } catch (e) {
    picker.replaceChildren(text("p", `No worktrees: ${e}`, "empty"));
  }
}
