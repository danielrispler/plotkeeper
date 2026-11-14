// Workspaces (danielrispler/Falinks#576; danielrispler/Falinks#565, decision 6): the home of tasks, a
// folded section at the bottom of the list. One row per workspace, the main checkout and each task, with
// its git tokens (the same as the Git line's, #26) and how many sessions work in it; the sessions
// themselves stay in the list. The Git line in the top bar shows the tokens of the selected
// session's workspace. Clicking a workspace offers a new session there and, for a task, Remove, with the
// loss check of a session's Delete. Creating a task is here too, not in the new-session picker.

import { element, invoke, patch, pressable, text } from "./dom.js";
import { landedMark } from "./list.js";
import { startClaude } from "./panes.js";
import { newTask } from "./picker.js";
import { studio } from "./state.js";
import { removeWorktree } from "./story.js";
import { throttle } from "./throttle.js";
import type { GitState, Workspace, Workspaces, Session } from "./types.js";

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

export type Token = { text: string; title: string; bad?: boolean };

// A workspace's git tokens, left to right, each hidden when it says nothing (#26): its branch or
// "detached", the upstream distance "↑2 ↓1" or "not pushed", "main ↓5" when behind origin's default
// branch (not on it, where the upstream distance is the same), "●3" uncommitted files, and "⚠" with
// an operation in progress. The hover title says each in words. The one function for the Git line
// and for Workspaces rows.
export function gitTokens(state: GitState | null): Token[] {
  if (!state) return [];
  const tokens: Token[] = [
    state.branch ? { text: state.branch, title: `On branch ${state.branch}` } : { text: "detached", title: "Detached HEAD" },
  ];
  if (state.upstream) {
    const [ahead, behind] = state.upstream;
    if (ahead || behind) {
      tokens.push({ text: `↑${ahead} ↓${behind}`, title: `${plural(ahead, "commit")} not pushed, ${plural(behind, "commit")} not pulled` });
    }
  } else if (state.branch) {
    tokens.push({ text: "not pushed", title: "The branch has no upstream: never pushed" });
  }
  if (state.main_behind && !state.on_default) {
    const behind = `${plural(state.main_behind, "commit")} behind origin's default branch`;
    tokens.push({
      text: `main ↓${state.main_behind}`,
      title: state.would_conflict ? `${behind}; rebasing onto it would conflict` : behind,
      bad: state.would_conflict,
    });
  }
  if (state.changed) tokens.push({ text: `●${state.changed}`, title: plural(state.changed, "uncommitted file") });
  if (state.operation) {
    const title = state.operation === "conflicts" ? "Files have unresolved conflicts" : `A ${state.operation} is in progress`;
    tokens.push({ text: `⚠ ${state.operation}`, title });
  }
  return tokens;
}

// Every workspace, the main checkout first.
const allWorkspaces = ({ main, tasks }: Workspaces): Workspace[] => [main, ...tasks];

// The workspace a session in `cwd` works in: the deepest one its folder is in, so a session in a task
// is not counted for the main checkout around it too.
const workspaceOf = (all: Workspace[], cwd: string): Workspace | undefined =>
  all
    .filter(({ path }) => cwd === path || cwd.startsWith(`${path}/`))
    .sort((a, b) => b.path.length - a.path.length)[0];

export type WorkspaceRow = { workspace: Workspace; name: string; facts: string; actions: string[]; landed: boolean };

// The rows: the main checkout, then the tasks, each with its tokens and session count.
export function workspaceRows(found: Workspaces, states: (GitState | null)[], sessions: (Pick<Session, "cwd"> & { landed?: boolean })[]): WorkspaceRow[] {
  const all = allWorkspaces(found);
  const working = all.map(() => 0);
  // A task is landed when the core says so for its sessions (#53): the mark is the list's, on the task's row.
  const landed = all.map(() => false);
  for (const { cwd, landed: shipped } of sessions) {
    const workspace = workspaceOf(all, cwd);
    if (!workspace) continue;
    working[all.indexOf(workspace)]++;
    if (shipped) landed[all.indexOf(workspace)] = true;
  }
  const folder = (workspace: Workspace) => workspace.path.split("/").pop() ?? workspace.path;
  return all.map((workspace, i) => ({
    workspace,
    name: i === 0 ? "Main checkout" : (workspace.branch ?? `${folder(workspace)} (detached)`),
    facts: [...gitTokens(states[i] ?? null).map((token) => token.text), working[i] && plural(working[i], "session")]
      .filter(Boolean)
      .join("  "),
    actions: i === 0 ? ["New session here"] : ["New session here", "Remove"],
    landed: i > 0 && landed[i],
  }));
}

// The last workspaces read, each workspace's git state by path, and the workspaces whose actions show.
let read: Workspaces | null = null;
const states = new Map<string, GitState | null>();
let reading = false;
// A forced read asked for while one runs: one more read follows it.
let again = false;
let readAt = 0;
const opened = new Set<string>();

// Git's error, or any other, at the section's foot, until the next one.
function failed(error: unknown) {
  element("workspaces").querySelector(".picker-error")?.remove();
  element("workspaces").append(text("pre", String(error), "picker-error"));
}

// A workspace's row: its name, then its facts; a click shows or hides its actions under it.
function workspaceRow({ workspace, name, facts, actions, landed }: WorkspaceRow): HTMLElement {
  const row = document.createElement("div");
  row.dataset.key = workspace.path;
  const button = pressable(document.createElement("button"), false, () => {
    if (!opened.delete(workspace.path)) opened.add(workspace.path);
    draw();
  });
  button.classList.add("row");
  button.title = workspace.path;
  button.setAttribute("aria-expanded", String(opened.has(workspace.path)));
  const line = text("span", "", "l1");
  line.append(text("span", name, "name"));
  if (landed) line.append(landedMark());
  // The facts go under the name, so a long branch name is not cut for them.
  button.append(line, ...(facts ? [text("span", facts, "l2")] : []));
  row.append(button);
  if (!opened.has(workspace.path)) return row;
  const act = (label: string) =>
    pressable(text("button", label, "small"), false, () => {
      opened.delete(workspace.path);
      const done = label === "Remove" ? removeWorktree(workspace.path).then(() => readWorkspaces(true)) : startClaude(workspace.path);
      done.catch(failed);
    });
  const buttons = text("div", "", "workspace-actions");
  buttons.append(...actions.map(act));
  row.append(buttons);
  return row;
}

// A new task, named by its branch; a session starts in it.
function creating(): HTMLElement {
  const form = newTask("New task: branch name", "Create", (name) => invoke<Workspace>("create_task", { name }).finally(() => readWorkspaces(true)), false, failed);
  form.dataset.key = "new-task";
  return form;
}

// The Git line: the workspace of the selected session, or the main checkout's with none, and its tokens.
function drawLine() {
  const line = element("git-line");
  line.hidden = !read;
  if (!read) return;
  const workspace = shownWorkspace(read);
  const repo = read.main.path.split("/").pop();
  const name = workspace === read.main ? repo : `${repo} › ${workspace.path.split("/").pop()}`;
  const tokens = gitTokens(states.get(workspace.path) ?? null);
  line.title = [...tokens.map((token) => token.title), fetchNote(fetched, Date.now())].join("\n");
  line.replaceChildren(text("span", name ?? "", "git-workspace"), ...tokens.map((token) => Object.assign(text("span", token.text, token.bad ? "git-bad" : ""), { title: token.title })));
}

const shownWorkspace = (workspaces: Workspaces) => workspaceOf(allWorkspaces(workspaces), studio.selection?.session.cwd ?? workspaces.main.path) ?? workspaces.main;

function draw() {
  if (!read) return;
  drawLine();
  const all = allWorkspaces(read);
  const rows = workspaceRows(read, all.map(({ path }) => states.get(path) ?? null), studio.listed);
  element("workspaces-title").textContent = `Workspaces · ${rows.length}`;
  // Patched, so the new task's field keeps its text and focus as the list updates.
  patch(element("workspaces-rows"), [...rows.map(workspaceRow), creating()]);
}

// A workspace's state is read behind a throttle each (#26, Freshness, local state): at once on the first
// event, then once per 2 s while events keep coming, and once after the last.
const throttles = new Map<string, () => void>();
function touch(path: string) {
  let event = throttles.get(path);
  if (!event) {
    const readState = () =>
      invoke<GitState>("workspace_git_state", { path })
        .catch(() => states.get(path) ?? null)
        .then((state) => {
          states.set(path, state);
          draw();
        });
    event = throttle(readState, 2000, Date.now, (run, delay) => void setTimeout(run, delay));
    throttles.set(path, event);
  }
  event();
}

// The workspaces whose state shows: every workspace while the section is open, else the Git line's.
function touchShown() {
  if (!read) return;
  const open = (element("workspaces") as HTMLDetailsElement).open;
  for (const workspace of open ? allWorkspaces(read) : [shownWorkspace(read)]) touch(workspace.path);
}

// Reads the workspaces from git. At most one read at a time and, unless `now`, one per 10 s, as the list
// redraws on every hook event. A `now` during a read is not dropped: the read after it sees the
// change, such as a new task.
export async function readWorkspaces(now = false) {
  if (reading) {
    again ||= now;
    return;
  }
  if (!now && Date.now() - readAt < 10_000) return;
  reading = true;
  readAt = Date.now();
  try {
    read = await invoke<Workspaces>("workspaces");
    draw();
    touchShown();
  } catch (e) {
    failed(e);
  } finally {
    reading = false;
  }
  if (again) {
    again = false;
    await readWorkspaces(true);
  }
}

// The list was drawn, or the selection changed: the session counts follow at once, and the state of
// every workspace a session works in is read behind its throttle, a hook event having come.
export function showWorkspaces() {
  draw();
  readWorkspaces();
  if (!read) return;
  const all = allWorkspaces(read);
  const working = new Set(studio.listed.map(({ cwd }) => workspaceOf(all, cwd)?.path));
  for (const workspace of all) if (working.has(workspace.path)) touch(workspace.path);
  touchShown();
}

// The repository's fetch, once for all its workspaces (#33): when it last succeeded, and git's message
// if the latest failed. A failed fetch leaves the refs as they were, so the old numbers stay.
export type FetchState = { at: number | null; error: string | null };
const fetched: FetchState = { at: null, error: null };
let fetching = false;

// The Git line's hover on the fetch, from `now` in ms.
export function fetchNote({ at, error }: FetchState, now: number): string {
  const lines = ["Not fetched yet"];
  if (at !== null) {
    const minutes = Math.floor((now - at) / 60_000);
    const hours = Math.floor(minutes / 60);
    lines[0] = `Last fetch: ${minutes < 1 ? "just now" : hours < 1 ? `${plural(minutes, "minute")} ago` : `${plural(hours, "hour")} ago`}`;
  }
  if (error) lines.push(`The latest fetch failed: ${error}`);
  return lines.join("\n");
}

// Fetches now, then reads every workspace again; the palette's Fetch and the 5-minute timer. One at a time.
export async function fetchNow() {
  if (fetching) return;
  fetching = true;
  try {
    await invoke("fetch_repository");
    fetched.at = Date.now();
    fetched.error = null;
  } catch (e) {
    fetched.error = String(e);
  } finally {
    fetching = false;
  }
  if (read) for (const workspace of allWorkspaces(read)) touch(workspace.path);
  draw();
}

// The repository is fetched once as watching starts, so the first distances are not stale. Then,
// while the window is focused, every shown workspace is read every 5 s and the repository is fetched
// every 5 minutes; started by `main.ts`, not on import.
export function watchWorkspaces() {
  void fetchNow();
  setInterval(() => document.hasFocus() && touchShown(), 5000);
  setInterval(() => document.hasFocus() && void fetchNow(), 5 * 60_000);
}
