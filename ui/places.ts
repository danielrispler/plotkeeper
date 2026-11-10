// Places (#576; #565, decision 6): the home of tasks, a folded section at the bottom of the list. One
// row per place, the main checkout and each task, with its branch, where it stands against its base
// (#559's counts), its changed files and how many sessions work in it; the sessions themselves stay
// in the list. Clicking a place offers a new session there and, for a task, Remove, with the loss
// check of a session's Delete. Creating a task is here too, not in the new-session picker.

import { element, invoke, patch, pressable, text } from "./dom.js";
import { startClaude } from "./panes.js";
import { newTask } from "./picker.js";
import { studio } from "./state.js";
import { removeWorktree } from "./story.js";
import type { Counts, Place, Places, Session } from "./types.js";

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

// A place's facts, such as "2 ahead · 1 behind · 3 changed · 2 sessions": each count that is not zero.
export function factsText(counts: Counts | null, sessions: number): string {
  const [ahead, behind] = counts?.ahead_behind ?? [0, 0];
  return [
    ahead && `${ahead} ahead`,
    behind && `${behind} behind`,
    counts?.changed && `${counts.changed} changed`,
    sessions && plural(sessions, "session"),
  ]
    .filter(Boolean)
    .join(" · ");
}

export type PlaceRow = { place: Place; name: string; facts: string; actions: string[] };

// The rows: the main checkout, then the tasks, each with its facts. A session counts for the deepest
// place its folder is in, so one in a task is not counted for the main checkout around it too.
export function placeRows({ main, tasks }: Places, counts: (Counts | null)[], sessions: Pick<Session, "cwd">[]): PlaceRow[] {
  const all = [main, ...tasks];
  const working = all.map(() => 0);
  for (const { cwd } of sessions) {
    let deepest = -1;
    all.forEach(({ path }, i) => {
      if ((cwd === path || cwd.startsWith(`${path}/`)) && (deepest < 0 || path.length > all[deepest].path.length)) deepest = i;
    });
    if (deepest >= 0) working[deepest]++;
  }
  const folder = (place: Place) => place.path.split("/").pop() ?? place.path;
  return all.map((place, i) => ({
    place,
    name: i === 0 ? `Main checkout${main.branch ? ` · ${main.branch}` : ""}` : (place.branch ?? `${folder(place)} (detached)`),
    facts: factsText(counts[i] ?? null, working[i]),
    actions: i === 0 ? ["New session here"] : ["New session here", "Remove"],
  }));
}

// The last places read, their counts, and the places whose actions show.
let read: { places: Places; counts: (Counts | null)[] } | null = null;
let reading = false;
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

function draw() {
  if (!read) return;
  const rows = placeRows(read.places, read.counts, studio.listed);
  element("places-title").textContent = `Places · ${rows.length}`;
  // Patched, so the new task's field keeps its text and focus as the list updates.
  patch(element("places-rows"), [...rows.map(placeRow), creating()]);
}

// Reads the places from git, and, while the section is open, their counts. At most one read at a
// time and, unless `now`, one per 10 s, as the list redraws on every hook event.
export async function readPlaces(now = false) {
  if (reading || (!now && Date.now() - readAt < 10_000)) return;
  reading = true;
  readAt = Date.now();
  try {
    const places = await invoke<Places>("places");
    const all = [places.main, ...places.tasks];
    const open = (element("places") as HTMLDetailsElement).open;
    const count = (place: Place, i: number) => invoke<Counts>("place_counts", { path: place.path, main: i === 0 }).catch(() => null);
    read = { places, counts: open ? await Promise.all(all.map(count)) : all.map(() => null) };
    draw();
  } catch (e) {
    failed(e);
  } finally {
    reading = false;
  }
}

// The list was drawn: the session counts follow at once, git's facts at most every 10 s.
export function showPlaces() {
  draw();
  readPlaces();
}
