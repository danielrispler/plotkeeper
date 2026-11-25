// Live sessions on the Map view (spec danielrispler/plotkeeper#306, ticket #311; spec #549): since #562 one count badge per part that marked
// sessions' latest turns touch (`sessionBadges`, `map-live.ts`), whose press opens one kept list of those sessions; a quiet "not placed" list
// beside the map for the marked sessions no part holds; and, while a session is spotlit, its footprint as corner brackets (#559), its chip on
// its latest turn's parts with its line on the home part, and the other badges dimmed. The pure rules are `map-live.ts`'s; this file draws
// them from the sessions the window already has, and `map_place` (the server, `studio-system-map`) says which part owns each path.

import { element, failure, invoke, newest, pressable, text } from "./dom.js";
import { currentMap, onCamera, onDraw, registerLayer, revealParts, syncSide } from "./map.js";
import { logStop } from "./stop-rules.js";
import { logo, logoKey } from "./logos.js";
import { sidebarOrder } from "./projects.js";
import { type Badge, type BadgeRow, footprint, footprintNote, spotlightView, latestParts, notPlacedLine, placeGroups, type Placement, sessionBadges, spotlit, turnLight } from "./map-live.js";
import { stateLabels, type Session, type Subagent, type Turn } from "./types.js";

// What the drawing needs of the window, given by `main.ts`: the sessions, a folder's project, the jump to a session or a Worker's log,
// whether a PR brief plays (no spotlight then), a session's name and second line as the session list shows them, and what follows a new
// placement (the story, whose feed redraw also refreshes the sheet's "This session here").
export type AgentDeps = {
  sessions: () => Session[];
  projectOf: (cwd: string) => string | null;
  select: (session: string, agent: string | null) => void;
  briefOpen: () => boolean;
  name: (session: Session) => string;
  line: (session: Session) => string | null;
  placed: () => void;
};
let deps: AgentDeps;
let hidden = false;
// The last answer of `map_place` for the paths asked, by path, so a refresh that changes none asks nothing.
let asked = "";
let placed: Placement = {};
// The last placement, for the story's turn rules (`turnParts`): a path asked for no part is not in it yet.
export const currentPlacement = (): Placement => placed;
const latest = newest();

// The session whose row the pointer is on (a session list or History row, a badge list's or the not placed list's), which is spotlit
// (#559, #562), and that row: a hover ends when the row leaves the page (a session moving to History, a redraw, the panel closing) or its
// list closes (`endHoverIn`), which `pointerleave` never reports.
let hovered: string | null = null;
let hoveredRow: Element | null = null;
const rowGone = () => hoveredRow !== null && !hoveredRow.isConnected;
// A list closed: a hover on one of its rows ends.
const endHoverIn = (list: Element) => {
  if (hoveredRow && list.contains(hoveredRow)) hoverAgents(null);
};
export function hoverAgents(session: string | null, row: Element | null = null) {
  hoveredRow = row;
  if (session === hovered) return;
  hovered = session;
  paint();
}

// The session open in the left panel (#385), spotlit while no row is hovered.
let opened: string | null = null;
export function openAgents(session: string | null) {
  if (session === opened && !rowGone()) return;
  opened = session;
  paint();
}

// Hides every badge, chip and the not placed list, or shows them again: for a view of a past commit, where nothing is live.
export function hideAgents(hide: boolean) {
  hidden = hide;
  paint();
}

// The shown project's sessions, whose turns are placed: none on a past commit's map.
const shownSessions = (project: string) => (hidden ? [] : deps.sessions().filter((session) => !session.shell && deps.projectOf(session.cwd) === project));

// The added hover text of a session's row (#559): "Touched no part in the last 7 days" once the placement says so; null otherwise, and for
// a session the map does not show.
export function rowNote(id: string): string | null {
  const view = currentMap();
  const session = view ? shownSessions(view.project).find((one) => one.id === id) : undefined;
  return session ? footprintNote(session, placed, Date.now()) : null;
}

type View = NonNullable<ReturnType<typeof currentMap>>;

// The spotlit session of the map shown: the hovered row's, else the open one's; none while a brief plays or a past commit shows.
function spotOf(view: View, sessions: Session[]): Session | null {
  if (rowGone()) [hovered, hoveredRow] = [null, null];
  const id = spotlit({ hovered, open: opened, brief: deps.briefOpen(), past: view.scrub !== null, shown: sessions.map((session) => session.id) });
  return sessions.find((one) => one.id === id) ?? null;
}

// The parts the spotlit session's latest turn touched, which the left panel keeps clear of itself when it opens (#385).
export function spotlitParts(): string[] {
  const view = currentMap();
  if (!view) return [];
  const spot = spotOf(view, shownSessions(view.project));
  return spot ? latestParts(spot, placed, Date.now()) : [];
}

const logMapAction = () => {
  const view = currentMap();
  if (view) logStop("map-action", view.project);
};

// A session's state as its row in a list says it, an unseen one's included.
const stateWord = (session: Session) => `${stateLabels[session.state]}${session.unseen && session.state !== "needs_you" && session.state !== "running" ? ", not seen" : ""}`;

// The Needs-you pill, in the session list's colours.
const needsYouPill = () => {
  const pill = text("span", "Needs you", "glyph");
  pill.dataset.glyph = "pill";
  return pill;
};

// A session's row in a badge's list or the not placed list: its agent's logo, its full name, its state and its line; hovering it spotlights
// the session, pressing it opens its story. A badge's list follows it with the session's Workers on the part, each opening its log.
function sessionRows(session: Session, workers: Subagent[], close: () => void): HTMLElement[] {
  const line = deps.line(session);
  const row = pressable(text("button", "", "session-pick"), null, () => {
    logMapAction();
    close();
    deps.select(session.id, null);
  });
  const state = text("span", stateWord(session), `session-pick-state${session.state === "needs_you" ? " needs-you" : ""}`);
  row.append(logo(logoKey(session)), text("span", deps.name(session), "session-pick-name"), state, ...(line ? [text("span", line, "session-pick-line")] : []));
  row.title = [deps.name(session), stateWord(session), ...(line ? [line] : []), "Open its story"].join("\n");
  row.dataset.session = session.id;
  row.addEventListener("pointerenter", () => hoverAgents(session.id, row));
  row.addEventListener("pointerleave", () => hoverAgents(null));
  return [row, ...workers.map((worker) => {
    const asks = session.asker?.agent === worker.id || session.asker?.agent === worker.name;
    const button = pressable(text("button", "", "session-pick worker"), null, () => {
      logMapAction();
      close();
      deps.select(session.id, worker.id);
    });
    button.append(text("span", worker.name, "session-pick-name"), ...(asks ? [text("span", "Needs you", "session-pick-state needs-you")] : []));
    button.title = `${worker.name}${asks ? " needs you" : ""}\nRead this worker's log`;
    return button;
  })];
}

// Fills `box` with rows built by `build`, only when what they show (`key`) changed: a row rebuilt under the pointer would end its own hover.
function fillRows(box: HTMLElement, key: string, build: () => HTMLElement[]) {
  if (box.dataset.rows === key) return;
  box.dataset.rows = key;
  box.replaceChildren(...build());
}
const rowsKey = (rows: BadgeRow[]) =>
  JSON.stringify(rows.map(({ session, workers }) => [session.id, session.state, session.unseen, deps.name(session), deps.line(session), session.asker?.agent ?? null, workers.map((worker) => worker.id)]));

// The badge's list (#562): one element, kept outside the transformed map world, a native popover in the top layer. It is placed by its
// badge's on-screen box when it opens and closed by any camera move, so it never floats away from its badge.
let listPart: string | null = null;
let listProject: string | null = null;
let listWasOpen = false;
const badgeList = () => element("map-badge-list");
const listOpen = () => listPart !== null && badgeList().matches(":popover-open");
const badgeOn = (part: string) => currentMap()?.slots.canvas.querySelector<HTMLElement>(`.plate[data-id="${CSS.escape(part)}"] .session-badge`) ?? null;

function closeBadgeList() {
  if (badgeList().matches(":popover-open")) badgeList().hidePopover();
}

// The list closed, by a press outside, Esc or any of the cases above: its badge says so, and a focus inside it goes back to the badge.
function listClosed() {
  endHoverIn(badgeList());
  const part = listPart;
  listPart = null;
  listProject = null;
  if (part === null) return;
  const badge = badgeOn(part);
  badge?.setAttribute("aria-expanded", "false");
  if (badge && (document.activeElement === document.body || badgeList().contains(document.activeElement))) badge.focus({ preventScroll: true });
}

function openList(part: string, badge: HTMLElement, rows: Badge["rows"], byKeyboard: boolean) {
  const list = badgeList();
  // Showing the list again at once folds the close's queued `toggle` into this opening's, so the list it replaces is closed here.
  if (listOpen()) {
    closeBadgeList();
    listClosed();
  }
  listPart = part;
  listProject = currentMap()?.project ?? null;
  list.dataset.rows = "";
  fillRows(list, rowsKey(rows), () => rows.flatMap((row) => sessionRows(row.session, row.workers, closeBadgeList)));
  list.showPopover({ source: badge });
  badge.setAttribute("aria-expanded", "true");
  // Below the badge, its end edge at the badge's end; over it when there is no room below; always inside the window.
  const box = badge.getBoundingClientRect();
  const [w, h, gap] = [list.offsetWidth, list.offsetHeight, 8];
  const below = box.bottom + gap + h <= window.innerHeight - gap;
  list.style.left = `${Math.max(gap, Math.min(box.right - w, window.innerWidth - w - gap))}px`;
  list.style.top = `${Math.max(gap, below ? box.bottom + gap : box.top - gap - h)}px`;
  if (byKeyboard) list.querySelector<HTMLElement>("button")?.focus();
}

// A part's badge: the count of its sessions, accented when one needs you, dimmed while another session is spotlit. Its press opens its list
// (a badge of one too, never the story directly) and stops there, so the plate's sheet does not open; hovering it spotlights nothing.
function badgeButton(part: string, badge: Badge, dim: boolean): HTMLElement {
  const count = badge.rows.length;
  const asking = badge.rows.filter((row) => row.session.state === "needs_you").length;
  const button = pressable(text("button", String(count), `session-badge${badge.accent ? " needs-you" : ""}${dim ? " dim" : ""}`), null, (event) => {
    // A pointer's press reads whether the list was open at its pointerdown, before light dismiss closed it; a keyboard press (no pointerdown,
    // `detail` 0) reads it now, so a pointer press let go off the badge, which never clicked, leaves nothing behind.
    const close = (event.detail !== 0 && listWasOpen) || (listOpen() && listPart === part);
    listWasOpen = false;
    logMapAction();
    if (close) return closeBadgeList();
    openList(part, button, badge.rows, event.detail === 0);
  });
  button.addEventListener("pointerdown", () => (listWasOpen = listOpen() && listPart === part));
  button.addEventListener("click", (event) => event.stopPropagation());
  button.dataset.part = part;
  button.setAttribute("aria-label", `${count} ${count === 1 ? "session" : "sessions"} on this part${asking ? `, ${asking} ${asking === 1 ? "needs" : "need"} you` : ""}`);
  button.setAttribute("aria-expanded", String(listOpen() && listPart === part));
  button.title = [...badge.rows.map((row) => `${deps.name(row.session)} · ${stateWord(row.session)}`), "Show these sessions"].join("\n");
  return button;
}

// The spotlit session's chip on a part of its latest turn: its logo, full name and Needs-you pill, and on its home part its line.
function spotChip(session: Session, home: boolean): HTMLElement {
  const line = home ? deps.line(session) : null;
  const chip = text("span", "", `spot-chip${session.state === "needs_you" ? " needs-you" : ""}`);
  chip.append(logo(logoKey(session)), text("span", deps.name(session), "spot-chip-name"), ...(session.state === "needs_you" ? [needsYouPill()] : []), ...(line ? [text("span", line, "spot-chip-line")] : []));
  chip.title = [deps.name(session), stateWord(session), ...(line ? [line] : [])].join("\n");
  return chip;
}

// The not placed list: a closed <details> whose summary is the line, made at the first paint and kept.
let kept: { details: HTMLDetailsElement; summary: HTMLElement; rows: HTMLElement } | undefined;
const notPlacedList = () => {
  if (kept) return kept;
  const details = document.createElement("details");
  const summary = text("summary", "", "side-title");
  summary.title = "The sessions whose latest turn touched no part: click to show or hide them";
  const rows = text("div", "", "not-placed-rows");
  details.append(summary, rows);
  details.addEventListener("toggle", () => details.open || endHoverIn(rows));
  kept = { details, summary, rows };
  return kept;
};

// Corner brackets around the spotlit session's plates (#559): firm on its latest turn's parts, faint on the rest within the cut.
function paintFootprint(view: View, spot: Session | null) {
  const { firm, faint } = spot ? footprint(spot, placed, Date.now()) : { firm: [], faint: [] };
  for (const plate of view.slots.canvas.querySelectorAll<HTMLElement>(".plate[data-id]")) {
    const id = plate.dataset.id ?? "";
    plate.classList.toggle("foot-firm", firm.includes(id));
    plate.classList.toggle("foot-faint", faint.includes(id));
  }
  paintTurn(view);
}

// Turn lighting (#560): the story's turn under the pointer or the focus lights its parts with `turn-lit`; nothing dims and the camera
// stays. Unfolding a past turn, or "Show on map" on the newest, pans its parts clear of the panel and the sheet (`revealParts`).
type LitTurn = Pick<Turn, "files" | "started_at" | "ended_at">;
let litTurn: LitTurn | null = null;

// The parts `turn` lights and reveals now: none past the cut, without files, or while a brief plays (unless `briefing` says otherwise).
export const turnLit = (turn: LitTurn, briefing = deps?.briefOpen() ?? false) => turnLight(turn, placed, Date.now(), briefing);

// `turn` is hovered or focused (`on`), or no longer is; a feed redraw lights none (`lightTurn(null)`).
export function lightTurn(turn: LitTurn | null, on = true) {
  const next = on ? turn : litTurn === turn ? null : litTurn;
  if (next === litTurn) return;
  litTurn = next;
  const view = currentMap();
  if (view) paintTurn(view);
}

function paintTurn(view: View) {
  const parts = litTurn ? turnLit(litTurn) : [];
  for (const plate of view.slots.canvas.querySelectorAll<HTMLElement>(".plate[data-id]")) plate.classList.toggle("turn-lit", parts.includes(plate.dataset.id ?? ""));
}

// Pans `turn`'s parts clear of the panel and the sheet, by `revealParts`'s limits; nothing while a brief plays.
export function revealTurn(turn: LitTurn) {
  const parts = turnLit(turn);
  if (parts.length) revealParts(parts);
}

// Puts the badges or the spotlit session's chips on the plates, the not placed list in its slot and the footprint, from what the last refresh
// found; keeps the open badge list while its part still has a badge, and the keyboard on a part's badge. Also called when a brief opens or closes.
export function paint() {
  const view = currentMap();
  if (!view) return;
  const sessions = shownSessions(view.project);
  const now = Date.now();
  const { byPart, notPlaced } = sessionBadges(sidebarOrder(sessions), placed, now, (cwd) => deps.projectOf(cwd) === view.project);
  const spot = spotOf(view, sessions);
  const plates = [...view.slots.canvas.querySelectorAll<HTMLElement>(".plate[data-id]")];
  // The plates are drawn in the map's reading order.
  const shown = spotlightView(byPart, spot, placed, now, plates.flatMap((plate) => plate.dataset.id ?? []));
  // The keyboard on a badge stays on its part's new one; a part's button that held it for a badge still to come (`map.ts`) gives it back.
  const active = document.activeElement as HTMLElement | null;
  const focused = active?.matches(".session-badge") ? active.dataset.part : active?.matches(".plate-open[data-wants-badge]") ? active.closest<HTMLElement>(".plate")?.dataset.id : undefined;
  for (const plate of plates) {
    const id = plate.dataset.id ?? "";
    const slot = plate.querySelector(".plate-chips");
    if (spot && shown.chips.includes(id)) slot?.replaceChildren(spotChip(spot, id === shown.home));
    else slot?.replaceChildren(...(byPart[id] ? [badgeButton(id, byPart[id], shown.dimmed.includes(id))] : []));
  }
  // A focused badge a spotlit chip replaced (its session's story opened from its list) leaves the keyboard on its part's button, as a redraw does.
  if (focused) (badgeOn(focused) ?? view.slots.canvas.querySelector<HTMLElement>(`.plate[data-id="${CSS.escape(focused)}"] .plate-open`))?.focus({ preventScroll: true });
  if (listPart !== null) {
    const rows = byPart[listPart]?.rows;
    if (!rows || listProject !== view.project || deps.briefOpen()) closeBadgeList();
    else fillRows(badgeList(), rowsKey(rows), () => rows.flatMap((row) => sessionRows(row.session, row.workers, closeBadgeList)));
  }
  // One line that a click opens (#472), so a session joining or leaving changes its count and not its height. The same element is kept
  // from paint to paint, so an opened list stays open, and its line keeps the focus, while sessions come and go.
  const slot = view.slots.notPlaced;
  if (notPlaced.length) {
    const list = notPlacedList();
    const line = notPlacedLine(notPlaced);
    list.summary.replaceChildren(text("span", line.count), ...(line.needsYou ? [text("span", ` · ${line.needsYou}`, "not-placed-asking")] : []));
    fillRows(list.rows, rowsKey(notPlaced.map((session) => ({ session, workers: [] }))), () => notPlaced.flatMap((session) => sessionRows(session, [], () => {})));
    // Back in its slot after it emptied or the map was cleared, the list comes back closed, so it never drops the sheet without a click.
    if (list.details.parentElement !== slot) {
      list.details.open = false;
      slot.replaceChildren(list.details);
    }
  } else slot.replaceChildren();
  syncSide(view.slots);
  paintFootprint(view, spot);
  // The row hovered may leave the page in the same update that painted (the list and History are drawn after the sessions are read):
  // looked at again once that update is done.
  if (hoveredRow) queueMicrotask(() => rowGone() && paint());
}

// Reads which parts the shown project's paths touch (when they changed) and draws the badges and the footprint. Called when the sessions change and when the map draws.
export const refreshAgents = () => read().catch(failure("map_place"));
async function read() {
  const view = currentMap();
  if (!view) return;
  // One group per path (#559): every file of the shown project's turns within the cut, and each Running session's in-flight edit.
  const groups = placeGroups(shownSessions(view.project), Date.now());
  const key = JSON.stringify([view.project, view.state.commit, Object.keys(groups).sort()]);
  const fresh = key !== asked;
  if (fresh) {
    const found = await latest(invoke("map_place", { project: view.project, groups }));
    if (!found || currentMap() !== view) return;
    asked = key;
    placed = found;
  }
  paint();
  // A new answer reaches what else reads the placement: the story's "Show on map" and the sheet's "This session here".
  if (fresh) deps.placed();
}

// Wires the drawing: a map that draws gets its badges, and `list.ts` calls `refreshAgents` as the sessions change. The badge list closes
// on any pan, zoom or drag of the map, and is Esc's layer of its own before the sheet.
export function startMapAgents(given: AgentDeps) {
  deps = given;
  onDraw(() => void refreshAgents());
  onCamera(closeBadgeList);
  badgeList().addEventListener("toggle", (event) => {
    if ((event as ToggleEvent).newState === "closed") listClosed();
  });
  registerLayer("badges", { open: listOpen, close: closeBadgeList });
}
