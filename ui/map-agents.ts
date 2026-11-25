// Live agents on the Map view (spec danielrispler/plotkeeper#306, ticket #311): a chip on every part a running or needs-you session's
// or Worker's current turn touched, and a quiet "not placed" list beside the map for the ones no part holds. The pure rules come first;
// the drawing below them reads the sessions the window already has, and `map_place` (the server, `studio-system-map`) says which parts a
// list of paths touches.

import { failure, invoke, newest, pressable, text } from "./dom.js";
import { currentMap, onDraw, syncSide } from "./map.js";
import { logStop } from "./stop-rules.js";
import type { Session } from "./types.js";

/** One agent on the map: a session's own, or a Worker's. `key` is the session's id, and `session/worker` for a Worker. */
export type Live = { key: string; session: string; agent: string | null; label: string; needsYou: boolean; paths: string[] };

const editing = ["Edit", "Write", "MultiEdit", "NotebookEdit"];

// The file an Edit or Write in flight works on, from the session's folder: null for any other step.
const inFlight = (step: Session["last_step"], cwd: string): string | null => {
  const path = step && editing.includes(step.tool) ? step.detail : null;
  if (!path) return null;
  return path.startsWith(`${cwd}/`) ? path.slice(cwd.length + 1) : path;
};

/**
 * The agents on the map: sessions in `Running` or `Needs you` of a project (`inProject` says whether a folder is one's), and their
 * running Workers. A Helper, a finished Worker, a plain shell and a session in any other state are never on it. A session's own
 * paths are its current turn's changed files that no running Worker's call or worktree names, with an Edit or Write in flight; a
 * Worker's are the ones its own name or id is on.
 */
export function liveAgents(sessions: Session[], inProject: (cwd: string) => boolean): Live[] {
  const live: Live[] = [];
  for (const session of sessions) {
    if ((session.state !== "running" && session.state !== "needs_you") || session.shell || !inProject(session.cwd)) continue;
    const workers = session.agents.filter((agent) => agent.kind === "worker" && agent.status === "running");
    const named = (name: string | null) => workers.find((worker) => worker.id === name || worker.name === name);
    const files = session.capabilities.turn_trees ? (session.turns.at(-1)?.files?.files ?? []) : [];
    const flight = inFlight(session.last_step, session.cwd);
    const own = files.filter((file) => !named(file.agent)).map((file) => file.path);
    const askedByWorker = session.asker?.agent != null && named(session.asker.agent) !== undefined;
    live.push({
      key: session.id,
      session: session.id,
      agent: null,
      label: session.name ?? session.identity.name,
      needsYou: session.state === "needs_you" && !askedByWorker,
      paths: [...new Set([...own, ...(flight ? [flight] : [])])],
    });
    for (const worker of workers) {
      const paths = files.filter((file) => file.agent === worker.id || file.agent === worker.name).map((file) => file.path);
      live.push({ key: `${session.id}/${worker.id}`, session: session.id, agent: worker.id, label: worker.name, needsYou: session.asker?.agent === worker.id, paths });
    }
  }
  return live;
}

/** Where each agent goes: on every part `placed` (its key to part ids) gives it, else in the not placed list. */
export function arrange(live: Live[], placed: Record<string, string[]>): { byPart: Record<string, Live[]>; notPlaced: Live[] } {
  const byPart: Record<string, Live[]> = {};
  const notPlaced: Live[] = [];
  for (const agent of live) {
    const parts = placed[agent.key] ?? [];
    if (!parts.length) notPlaced.push(agent);
    for (const part of parts) {
    const here = byPart[part] ?? [];
    here.push(agent);
    byPart[part] = here;
  }
  }
  return { byPart, notPlaced };
}

/**
 * The not placed list's closed line (#472): how many agents no part holds, and how many of them need you, so a needs-you agent stays loud.
 */
export function notPlacedLine(notPlaced: Live[]): { count: string; needsYou: string | null } {
  const asking = notPlaced.filter((agent) => agent.needsYou).length;
  return { count: `${notPlaced.length} not placed`, needsYou: asking ? `${asking} ${asking === 1 ? "needs" : "need"} you` : null };
}

/**
 * The chips a hovered list row lights (#382): every chip of the `hovered` session, its Workers' too; and those of the session `open` in the left
 * panel (#385), which stay lit while it is open. None without either.
 */
export function litAgents(live: Live[], hovered: string | null, open: string | null = null): string[] {
  return live.filter((agent) => agent.session === hovered || agent.session === open).map((agent) => agent.key);
}

/** What the drawing needs of the window, given by `main.ts`: the sessions, a folder's project, and the jump to a session. */
export type AgentDeps = { sessions: () => Session[]; projectOf: (cwd: string) => string | null; select: (session: string, agent: string | null) => void };
let deps: AgentDeps;
let hidden = false;
// The last answer of `map_place` for the paths asked, so a refresh that changes none asks nothing.
let asked = "";
let placed: Record<string, string[]> = {};
const latest = newest();

// The session whose list row the pointer is on (#382), whose chips are lit.
let hovered: string | null = null;
export function hoverAgents(session: string | null) {
  if (session === hovered) return;
  hovered = session;
  paint();
}

// The session open in the left panel (#385), whose chips stay lit while it is open.
let opened: string | null = null;
export function openAgents(session: string | null) {
  if (session === opened) return;
  opened = session;
  paint();
}

/** Hides every chip and the not placed list, or shows them again: for a view of a past commit, where nothing is live. */
export function hideAgents(hide: boolean) {
  hidden = hide;
  paint();
}

const chip = (agent: Live, select: AgentDeps["select"], lit: string[]) => {
  const button = pressable(text("button", agent.needsYou ? `${agent.label} needs you` : agent.label, `agent-chip${agent.needsYou ? " needs-you" : ""}${lit.includes(agent.key) ? " lit" : ""}`), null, () => {
    const view = currentMap();
    if (view) logStop("map-action", view.project);
    select(agent.session, agent.agent);
  });
  // The chip's press opens its session, not the plate's sheet under it (#385).
  button.addEventListener("click", (event) => event.stopPropagation());
  button.title = `${agent.label}${agent.needsYou ? " needs you" : ""}\nGo to this ${agent.agent === null ? "session" : "worker"}`;
  return button;
};

// The not placed list: a closed <details> whose summary is the line, made at the first paint and kept.
let kept: { details: HTMLDetailsElement; summary: HTMLElement; chips: HTMLElement } | undefined;
const notPlacedList = () => {
  if (kept) return kept;
  const details = document.createElement("details");
  const summary = text("summary", "", "side-title");
  summary.title = "The agents whose turn touched no part yet: click to show or hide them";
  const chips = text("div", "", "not-placed-chips");
  details.append(summary, chips);
  kept = { details, summary, chips };
  return kept;
};

// Puts the chips on the plates and the not placed list in its slot, from what the last refresh found.
function paint() {
  const view = currentMap();
  if (!view) return;
  const live = hidden ? [] : liveAgents(deps.sessions(), (cwd) => deps.projectOf(cwd) === view.project);
  const { byPart, notPlaced } = arrange(live, placed);
  const lit = litAgents(live, hovered, opened);
  for (const plate of view.slots.canvas.querySelectorAll<HTMLElement>(".plate[data-id]")) {
    plate.querySelector(".plate-chips")?.replaceChildren(...(byPart[plate.dataset.id ?? ""] ?? []).map((agent) => chip(agent, deps.select, lit)));
  }
  // One line that a click opens (#472), so an agent joining or leaving changes its count and not its height. The same element is kept
  // from paint to paint, so an opened list stays open, and its line keeps the focus, while agents come and go.
  const slot = view.slots.notPlaced;
  if (notPlaced.length) {
    const list = notPlacedList();
    const line = notPlacedLine(notPlaced);
    list.summary.replaceChildren(text("span", line.count), ...(line.needsYou ? [text("span", ` · ${line.needsYou}`, "not-placed-asking")] : []));
    list.chips.replaceChildren(...notPlaced.map((agent) => chip(agent, deps.select, lit)));
    // Back in its slot after it emptied or the map was cleared, the list comes back closed, so it never drops the sheet without a click.
    if (list.details.parentElement !== slot) {
      list.details.open = false;
      slot.replaceChildren(list.details);
    }
  } else slot.replaceChildren();
  syncSide(view.slots);
}

/**
 * Reads which parts the live agents' paths touch (when they changed) and draws them. Called when the sessions change and when the map draws.
 */
export const refreshAgents = () => read().catch(failure("map_place"));
async function read() {
  const view = currentMap();
  if (!view) return;
  const live = hidden ? [] : liveAgents(deps.sessions(), (cwd) => deps.projectOf(cwd) === view.project);
  const groups = Object.fromEntries(live.filter((agent) => agent.paths.length).map((agent) => [agent.key, agent.paths]));
  const key = JSON.stringify([view.project, view.state.commit, groups]);
  if (key !== asked) {
    const found = await latest(invoke("map_place", { project: view.project, groups }));
    if (!found || currentMap() !== view) return;
    asked = key;
    placed = found;
  }
  paint();
}

/** Wires the drawing: a map that draws gets its chips, and `list.ts` calls `refreshAgents` as the sessions change. */
export function startMapAgents(given: AgentDeps) {
  deps = given;
  onDraw(() => void refreshAgents());
}
