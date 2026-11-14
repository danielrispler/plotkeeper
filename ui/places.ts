// Places (danielrispler/Falinks#576; danielrispler/Falinks#565, decision 6): the home of tasks, a folded section at the bottom of the list. One
// row per place, the main checkout and each task, with its git tokens (branch, upstream distance,
// uncommitted files; the same as the Git line's, #29) and how many sessions work in it; the sessions themselves stay
// in the list. The Git line in the top bar shows the tokens of the selected session's place. Clicking a place offers a new session there and, for a task, Remove, with the loss
// check of a session's Delete. Creating a task is here too, not in the new-session picker.

import { element, invoke, patch, pressable, text } from "./dom.js";
import { startClaude } from "./panes.js";
import { newTask } from "./picker.js";
import { studio } from "./state.js";
import { removeWorktree } from "./story.js";
import { throttle } from "./throttle.js";
import type { GitState, Place, Places, Session } from "./types.js";

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

export type Token = { text: string; title: string };

// A place's git tokens, left to right, each hidden when it says nothing (#26): its branch or
// "detached", the upstream distance "↑2 ↓1" or "not pushed", "main ↓5" when behind origin's default branch (not on it, where the upstream distance is the same), and "●3" uncommitted files. The hover
// title says each in words. The one function for the Git line and for Places rows.
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
    tokens.push({ text: `main ↓${state.main_behind}`, title: `${plural(state.main_behind, "commit")} behind origin's default branch` });
  }
  if (state.changed) tokens.push({ text: `●${state.changed}`, title: plural(state.changed, "uncommitted file") });
  if (state.operation) {
    const title = state.operation === "conflicts" ? "Files have unresolved conflicts" : `A ${state.operation} is in progress`;
    tokens.push({ text: `⚠ ${state.operation}`, title });
  }
  return tokens;
}

// The place a session in `cwd` works in: the deepest one its folder is in, so a session in a task
// is not counted for the main checkout around it too.
const placeOf = (all: Place[], cwd: string): Place | undefined =>
  all
    .filter(({ path }) => cwd === path || cwd.startsWith(`${path}/`))
    .sort((a, b) => b.path.length - a.path.length)[0];

export type PlaceRow = { place: Place; name: string; facts: string; actions: string[] };

// The rows: the main checkout, then the tasks, each with its tokens and session count.
export function placeRows({ main, tasks }: Places, states: (GitState | null)[], sessions: Pick<Session, "cwd">[]): PlaceRow[] {
  const all = [main, ...tasks];
  const working = all.map(() => 0);
  for (const { cwd } of sessions) {
    const place = placeOf(all, cwd);
    if (place) working[all.indexOf(place)]++;
  }
  const folder = (place: Place) => place.path.split("/").pop() ?? place.path;
  return all.map((place, i) => ({
    place,
    name: i === 0 ? "Main checkout" : (place.branch ?? `${folder(place)} (detached)`),
    facts: [...gitTokens(states[i] ?? null).map((token) => token.text), working[i] && plural(working[i], "session")]
      .filter(Boolean)
      .join("  "),
    actions: i === 0 ? ["New session here"] : ["New session here", "Remove"],
  }));
}

// The last places read, each place's git state by path, and the places whose actions show.
let read: Places | null = null;
const states = new Map<string, GitState | null>();
let reading = false;
// A forced read asked for while one runs: one more read follows it.
let again = false;
let readAt = 0;
const opened = new Set<string>();

// Git's error, or any other, at the section's foot, until the next one.
function failed(error: unknown) {
  element("places").querySelector(".picker-error")?.remove();
  element("places").append(text("pre", String(error), "picker-error"));
}

// A place's row: its name, then its facts; a click shows or hides its actions under it.
function placeRow({ place, name, facts, actions }: PlaceRow): HTMLElement {
  const row = document.createElement("div");
  row.dataset.key = place.path;
  const button = pressable(document.createElement("button"), false, () => {
    if (!opened.delete(place.path)) opened.add(place.path);
    draw();
  });
  button.classList.add("row");
  button.title = place.path;
  button.setAttribute("aria-expanded", String(opened.has(place.path)));
  const line = text("span", "", "l1");
  line.append(text("span", name, "name"));
  // The facts go under the name, so a long branch name is not cut for them.
  button.append(line, ...(facts ? [text("span", facts, "l2")] : []));
  row.append(button);
  if (!opened.has(place.path)) return row;
  const act = (label: string) =>
    pressable(text("button", label, "small"), false, () => {
      opened.delete(place.path);
      const done = label === "Remove" ? removeWorktree(place.path).then(() => readPlaces(true)) : startClaude(place.path);
      done.catch(failed);
    });
  const buttons = text("div", "", "place-actions");
  buttons.append(...actions.map(act));
  row.append(buttons);
  return row;
}

// A new task, named by its branch; a session starts in it.
function creating(): HTMLElement {
  const form = newTask("New task: branch name", "Create", (name) => invoke<Place>("create_task", { name }).finally(() => readPlaces(true)), false, failed);
  form.dataset.key = "new-task";
  return form;
}

// The Git line: the place of the selected session, or the main checkout's with none, and its tokens.
function drawLine() {
  const line = element("git-line");
  line.hidden = !read;
  if (!read) return;
  const place = shownPlace(read);
  const repo = read.main.path.split("/").pop();
  const name = place === read.main ? repo : `${repo} › ${place.path.split("/").pop()}`;
  const tokens = gitTokens(states.get(place.path) ?? null);
  line.title = tokens.map((token) => token.title).join("\n");
  line.replaceChildren(text("span", name ?? "", "git-place"), ...tokens.map((token) => Object.assign(text("span", token.text), { title: token.title })));
}

const shownPlace = ({ main, tasks }: Places) => placeOf([main, ...tasks], studio.selection?.session.cwd ?? main.path) ?? main;

function draw() {
  if (!read) return;
  drawLine();
  const all = [read.main, ...read.tasks];
  const rows = placeRows(read, all.map(({ path }) => states.get(path) ?? null), studio.listed);
  element("places-title").textContent = `Places · ${rows.length}`;
  // Patched, so the new task's field keeps its text and focus as the list updates.
  patch(element("places-rows"), [...rows.map(placeRow), creating()]);
}

// A place's state is read behind a throttle each (#26, Freshness, local state): at once on the first
// event, then once per 2 s while events keep coming, and once after the last.
const throttles = new Map<string, () => void>();
function touch(path: string) {
  let event = throttles.get(path);
  if (!event) {
    const readState = () =>
      invoke<GitState>("place_git_state", { path })
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

// The places whose state shows: every place while the section is open, else the Git line's.
function touchShown() {
  if (!read) return;
  const open = (element("places") as HTMLDetailsElement).open;
  for (const place of open ? [read.main, ...read.tasks] : [shownPlace(read)]) touch(place.path);
}

// Reads the places from git. At most one read at a time and, unless `now`, one per 10 s, as the list
// redraws on every hook event. A `now` during a read is not dropped: the read after it sees the
// change, such as a new task.
export async function readPlaces(now = false) {
  if (reading) {
    again ||= now;
    return;
  }
  if (!now && Date.now() - readAt < 10_000) return;
  reading = true;
  readAt = Date.now();
  try {
    read = await invoke<Places>("places");
    draw();
    touchShown();
  } catch (e) {
    failed(e);
  } finally {
    reading = false;
  }
  if (again) {
    again = false;
    await readPlaces(true);
  }
}

// The list was drawn, or the selection changed: the session counts follow at once, and the state of
// every place a session works in is read behind its throttle, a hook event having come.
export function showPlaces() {
  draw();
  readPlaces();
  if (!read) return;
  const all = [read.main, ...read.tasks];
  const working = new Set(studio.listed.map(({ cwd }) => placeOf(all, cwd)?.path));
  for (const place of all) if (working.has(place.path)) touch(place.path);
  touchShown();
}

// While the window is focused, every shown place is read every 5 s; started by `main.ts`, not on import.
export const watchPlaces = () => setInterval(() => document.hasFocus() && touchShown(), 5000);
