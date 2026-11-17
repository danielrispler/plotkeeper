// The picker (danielrispler/Falinks#467): where a new session starts. It lists the configured repository's main checkout
// and its tasks, read live from git each time it opens, with `agent-*` worktrees hidden and counted
// on the main checkout. Choosing one starts the chosen agent there in a new terminal, or, from New
// terminal, a plain terminal (`agents.ts`).
// Creating a task by its branch name and removing one (danielrispler/Falinks#468) live in Workspaces (danielrispler/Falinks#576), at the list's foot.
// The quick start, first: an issue, a spec or a prompt names a new task, and its session starts on it;
// left empty, a session starts in the main checkout with no prompt.

import { AGENT_NAMES, agentChoice, kept, type Kind, showsAuto } from "./agents.js";
import { element, fullTitle, invoke, pressable, text } from "./dom.js";
import type { Entry } from "./keys.js";
import { setProjects } from "./projects.js";
import { startAgent } from "./panes.js";
import { shortPath } from "./paths.js";
import { studio } from "./state.js";
import { branchBlocked, branchOptions, refreshAfterSwitch, shownProject, workspaceRows } from "./workspaces.js";
import type { Binding } from "./settings.js";
import type { Branch, Workspace, Workspaces } from "./types.js";

// Git's error, or any other, at the picker's foot.
function failed(error: unknown) {
  element("picker").querySelector(".picker-error")?.remove();
  element("picker").append(text("pre", String(error), "picker-error"));
}

// The agent New session starts, kept across runs; Claude Code until Daniel picks another.
export function lastAgent(): string {
  try {
    return localStorage.getItem("agent") ?? "claude";
  } catch {
    return "claude";
  }
}
function keepAgent(picked: string) {
  try {
    localStorage.setItem("agent", kept(picked, lastAgent()));
  } catch {
    // Not kept: the next picker opens on Claude Code.
  }
}

// Starts what `start` says in `cwd`, with `prompt` as its first prompt; by default the last agent.
type Start = (cwd: string, prompt: string) => Promise<void>;
export const startLast: Start = (cwd, prompt) => startAgent(lastAgent(), cwd, prompt);

// A field whose Enter or button makes a task with `create`, then starts a session there with
// `start`, given `prompt` of the field's text as its first prompt. Only a `prompt` field passes an
// empty text on. Errors go to `report`: the picker's foot, or Workspaces' (danielrispler/Falinks#576).
export function newTask(
  placeholder: string,
  label: string,
  create: (value: string) => Promise<Workspace>,
  prompt: boolean,
  report: (error: unknown) => void = failed,
  start: Start = startLast,
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
      await start(task.path, prompt ? value : "");
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
// empty, the main checkout. It creates in `project`, the one the picker opened on, never the one
// selected by the time Daniel submits.
export const quickTask = (project: string, main: Workspace, request: string, create: typeof invoke = invoke): Promise<Workspace> =>
  request ? create<Workspace>("create_named_task", { project, request }) : Promise.resolve(main);
const quickStart = (project: string, main: Workspace, start: Start) =>
  newTask("Issue #, spec path or prompt; empty for main", "Start", (request) => quickTask(project, main, request), true, failed, start);

// The Auto mode choice (danielrispler/Falinks#541): off each time the picker opens; the session starts in Claude Code's
// auto mode when on. Bypass mode is never offered.
function autoChoice(): [HTMLElement, () => boolean] {
  const box = Object.assign(document.createElement("input"), { type: "checkbox" });
  const label = text("label", "", "auto-mode");
  label.append(box, " Auto mode");
  return [label, () => box.checked];
}

// The agents on Daniel's `PATH`, asked once per run, as his login shell is slow to start.
let installed: Promise<string[]> | null = null;
const offeredAgents = () => (installed ??= invoke<string[]>("agents").catch(() => ["claude"]));

// The agent choice: one button per agent, the chosen one pressed; Auto mode shows with Claude Code only.
function agentRow(offered: string[], chosen: string, auto: HTMLElement): [HTMLElement, () => string] {
  const row = text("div", "", "agent-choice");
  let current = chosen;
  const draw = () => {
    auto.hidden = !showsAuto(current);
    row.replaceChildren(
      ...offered.map((agent) => {
        return pressable(text("button", AGENT_NAMES[agent] ?? agent, "btn small"), agent === current, () => {
          current = agent;
          keepAgent(agent);
          draw();
        });
      }),
    );
  };
  draw();
  return [row, () => current];
}

// A workspace's row: its name, then its folder.
export function workspaceRow(name: string, workspace: Workspace, start: Start): HTMLElement {
  const button = pressable(document.createElement("button"), false, () => {
    element("picker").hidden = true;
    start(workspace.path, "").catch((e) => console.warn(e));
  });
  button.classList.add("row");
  const first = text("span", "", "l1");
  first.append(fullTitle(text("span", name, "name"), name));
  const second = Object.assign(text("span", shortPath(workspace.path), "l2"), { title: workspace.path }); // danielrispler/Falinks#571
  button.append(first, second);
  return button;
}

function picked(project: string, workspaces: Workspaces, agents: string[], kind: Kind): HTMLElement[] {
  const { hidden } = workspaces;
  const count = hidden ? [text("p", `${hidden} agent worktree${hidden === 1 ? "" : "s"} hidden`, "hidden-count")] : [];
  const [choice, auto] = autoChoice();
  const { offered, chosen } = agentChoice(agents, lastAgent(), kind);
  const [agentButtons, agent] = agentRow(offered, chosen, choice);
  const start: Start = (cwd, prompt) => startAgent(agent(), cwd, prompt, auto());
  const [main, ...tasks] = workspaceRows(workspaces, [], []).map(({ name, workspace }) => workspaceRow(name, workspace, start));
  return [
    agentButtons,
    text("h1", "In its own task"),
    quickStart(project, workspaces.main, start),
    choice,
    text("h1", "Start in"),
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

// The palette (danielrispler/Falinks#575), in the picker's box: every action with its keys, filtered by what Daniel types;
// ↑ and ↓ move the highlighted row while the focus stays in the field (#24); Enter or a click runs one,
// from where the focus was. Opening it again closes it.
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
  let highlighted = 0;
  const draw = () =>
    rows.replaceChildren(
      ...shown().map((entry, index) => {
        const button = pressable(document.createElement("button"), index === highlighted, () => choose(entry));
        button.classList.add("row");
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

// Opens the picker, on the last agent for New session or on a plain terminal for New terminal, or
// closes it when open.
export async function showPicker(kind: Kind = "session") {
  const picker = element("picker");
  if (closed(kind === "terminal" ? "New terminal" : "New session")) return;
  // It opens once its rows are read, so no placeholder flashes first in another box (danielrispler/Falinks#570).
  try {
    const project = await shownProject();
    const [workspaces, agents] = await Promise.all([invoke<Workspaces>("workspaces", { project }), offeredAgents()]);
    picker.replaceChildren(...picked(project, workspaces, agents, kind));
  } catch (e) {
    picker.replaceChildren(text("p", `No worktrees: ${e}`, "empty"));
  }
  picker.hidden = false;
  picker.querySelector("input")?.focus();
}

// A branch's row: its label, cut to fit, with its full text in a title, and its note.
export function branchRow(label: string, note: string | null | undefined, disabled: boolean, pick: () => void): HTMLElement {
  const button = pressable(document.createElement("button"), false, pick);
  button.classList.add("row");
  button.disabled = disabled;
  const line = text("span", "", "l1");
  line.append(fullTitle(text("span", label, "name"), label), ...(note ? [text("span", note, "row-state")] : []));
  button.append(line);
  return button;
}

// The branch picker (#95), for the main checkout only: its local branches and the remote-only ones, one
// pick runs `git switch`, and git's refusal shows as it is. Disabled with its reason while a session in the
// main checkout is running or needs Daniel; a branch a task holds is greyed out as "in task X". Opening it
// again closes it.
export async function showBranches(from?: string) {
  const picker = element("picker");
  if (closed("Switch branch")) return;
  try {
    const project = from ?? (await shownProject());
    const [workspaces, branches] = await Promise.all([invoke<Workspaces>("workspaces", { project }), invoke<Branch[]>("branches", { project })]);
    const blocked = branchBlocked(workspaces, studio.listed);
    const rows = branchOptions(branches, blocked).map(({ branch, label, note, disabled }) =>
      branchRow(label, note, disabled, () => {
        invoke("switch_branch", { project, branch: branch.name, remote: branch.remote })
          .then(() => {
            picker.hidden = true;
            return refreshAfterSwitch(project);
          })
          .catch(failed);
      }),
    );
    picker.replaceChildren(...(blocked ? [text("p", `Switching is off: ${blocked}`, "empty")] : []), ...rows);
  } catch (e) {
    picker.replaceChildren(text("p", `No branches: ${e}`, "empty"));
  }
  picker.hidden = false;
}

// Adds the repository holding `folder` as a project (#98), at the end; a refusal, such as a folder git does not
// recognise or a repository already listed, shows in the picker's box as the message it is.
export async function addProject(folder: string) {
  try {
    setProjects(await invoke<string[]>("add_project", { folder }));
    element("picker").hidden = true;
  } catch (e) {
    refuse(e);
  }
}

// A refused add, remove or reorder of a project, as a message in the picker's box.
export function refuse(error: unknown) {
  const picker = element("picker");
  picker.ariaLabel = "Add project";
  picker.replaceChildren(text("p", String(error), "empty"));
  picker.hidden = false;
}

// Add project (#98): the native folder dialog, and below it the repositories where listed sessions ran, newest
// first, that are not projects yet. Opening it again closes it.
export async function showAddProject() {
  const picker = element("picker");
  if (closed("Add project")) return;
  const choose = pressable(text("button", "Choose folder…", "name"), false, () => {
    invoke<string | null>("choose_folder")
      .then((folder) => (folder ? addProject(folder) : undefined))
      .catch(failed);
  });
  choose.classList.add("row");
  picker.replaceChildren(choose);
  picker.hidden = false;
  try {
    const cwds = [...studio.listed].sort((a, b) => b.last_event_at - a.last_event_at).map(({ cwd }) => cwd);
    const suggested = await invoke<string[]>("suggest_projects", { cwds });
    if (picker.hidden || picker.ariaLabel !== "Add project") return;
    const rows = suggested.map((path) => {
      const row = pressable(document.createElement("button"), false, () => void addProject(path));
      row.classList.add("row");
      row.append(Object.assign(text("span", shortPath(path), "name"), { title: path }));
      return row;
    });
    picker.append(...(rows.length ? [text("p", "Repositories where sessions ran", "empty"), ...rows] : []));
  } catch (e) {
    failed(e);
  }
}
