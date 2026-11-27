// The live link's rules (spec danielrispler/plotkeeper#549, ticket #559): pure functions over the sessions the page already holds, which
// the map's drawing (`map-agents.ts`) and the story read. Placement is per path: `map_place` is asked one group per distinct path, and its
// answer (`Placement`, a path to the part that owns it) is folded back here per session, Worker and turn.

import { nextNeedsYou } from "./keys.js";
import type { Session, Subagent, Turn, TurnFile } from "./types.js";

// The page's 7-day cut, mirroring core's `SINCE_MS` (`core/src/turn_files.rs`): core asks for a turn's files only within it, but keeps
// them after, so the page applies the cut itself.
const CUT_MS = 7 * 24 * 60 * 60 * 1000;

// Whether a turn counts: its end, or its start if it never ended, is within the cut of `now`, whatever files it still holds.
const inCut = (turn: Pick<Turn, "started_at" | "ended_at">, now: number) => (turn.ended_at ?? turn.started_at) + CUT_MS >= now;

const editing = ["Edit", "Write", "MultiEdit", "NotebookEdit"];

// The file of a Running session's edit in flight (Edit, Write, MultiEdit, NotebookEdit), from its folder; null otherwise, so the last
// edit of a session that stopped never counts.
function inFlight(session: Pick<Session, "state" | "last_step" | "cwd">): string | null {
  const step = session.last_step;
  const path = session.state === "running" && step && editing.includes(step.tool) ? step.detail : null;
  if (!path) return null;
  return path.startsWith(`${session.cwd}/`) ? path.slice(session.cwd.length + 1) : path;
}

// A session's turns within the cut, with their index in `session.turns`.
const recentTurns = (session: Pick<Session, "turns">, now: number) =>
  session.turns.flatMap((turn, index) => (inCut(turn, now) ? [{ turn, index }] : []));

// The paths `map_place` is asked about for `sessions` (the shown project's): every file of every turn within the cut, Workers' included,
// and each Running session's in-flight edit; one group per distinct path, keyed by the path.
export function placeGroups(sessions: Session[], now: number): Record<string, string[]> {
  const paths = new Set<string>();
  for (const session of sessions) {
    for (const { turn } of recentTurns(session, now)) for (const file of turn.files?.files ?? []) paths.add(file.path);
    const flight = inFlight(session);
    if (flight) paths.add(flight);
  }
  return Object.fromEntries([...paths].map((path) => [path, [path]]));
}

// Where `map_place` put each path: the part that owns it, or none for an unowned or ignored file.
export type Placement = Record<string, string[]>;

// A session's footprint: the parts its turns within the cut changed, Workers' files included; `firm` the latest turn's, `faint` the rest.
export type Footprint = { firm: string[]; faint: string[] };
export function footprint(session: Session, placed: Placement, now: number): Footprint {
  const latest = session.turns.length - 1;
  const firm = new Set<string>();
  const all = new Set<string>();
  for (const { turn, index } of recentTurns(session, now)) {
    for (const part of turnParts(turn, placed)) (index === latest ? firm : all).add(part);
  }
  const flight = inFlight(session);
  if (flight) for (const part of placed[flight] ?? []) firm.add(part);
  return { firm: [...firm], faint: [...all].filter((part) => !firm.has(part)) };
}

// The parts `paths` touch, each once, in the order first met.
const partsOf = (paths: string[], placed: Placement): string[] => [...new Set(paths.flatMap((path) => placed[path] ?? []))];

// The parts a turn changed, Workers' files included; none while its files are not diffed.
const turnParts = (turn: Pick<Turn, "files">, placed: Placement): string[] => partsOf((turn.files?.files ?? []).map((file) => file.path), placed);

// Whether a session is marked, wants Daniel's attention on the map: Running or Needs you, or Your turn or Idle while unseen. Never an
// Ended session; `inProject`, asked of its project folder, keeps another project's out. (A Helper is an agent of a session,
// never a session.)
export function marked(session: Session, inProject: (project: string) => boolean): boolean {
  if (!inProject(session.project)) return false;
  if (session.state === "running" || session.state === "needs_you") return true;
  return (session.state === "your_turn" || session.state === "idle") && session.unseen === true;
}

// What decides the spotlit session: the hovered session (a session list or History row; `null` once its row left the page), the session
// open in the left panel, whether a PR brief plays or a past commit shows, and the ids of the sessions the map shows (the shown project's).
export type Spotlight = { hovered: string | null; open: string | null; brief: boolean; past: boolean; shown: string[] };

// The spotlit session: the hovered one, else the open one, each only while the map shows it; none while a brief plays or a past commit shows.
export function spotlit(given: Spotlight): string | null {
  if (given.brief || given.past) return null;
  return [given.hovered, given.open].find((id) => id !== null && given.shown.includes(id)) ?? null;
}

export const NO_FOOTPRINT = "Touched no part in the last 7 days";

// A hovered row's added hover text when its session touched no part within the cut; null when it did, or while `map_place` has not yet
// answered for one of its paths, so a pending answer never reads as an empty footprint.
export function footprintNote(session: Session, placed: Placement, now: number): string | null {
  const paths = Object.keys(placeGroups([session], now));
  if (paths.some((path) => !(path in placed))) return null;
  const { firm, faint } = footprint(session, placed, now);
  return firm.length || faint.length ? null : NO_FOOTPRINT;
}

// "This session here" on a part's sheet (#561): the open session's turns within the cut that changed `part`, newest first, each with its
// index in `session.turns` and its number (the index from 1); `older` says whether the session has turns older than the cut, and `pending`
// whether `map_place` has yet to answer for a file of one of those turns, so a pending answer never reads as "no turn changed this part".
export type HereRow = { turn: Turn; index: number; number: number };
export function sessionHere(session: Pick<Session, "turns">, part: string, placed: Placement, now: number): { rows: HereRow[]; older: boolean; pending: boolean } {
  const recent = recentTurns(session, now);
  const rows = recent
    .filter(({ turn }) => turnParts(turn, placed).includes(part))
    .map(({ turn, index }) => ({ turn, index, number: index + 1 }))
    .reverse();
  const pending = recent.some(({ turn }) => (turn.files?.files ?? []).some((file) => !(file.path in placed)));
  return { rows, older: session.turns.some((turn) => !inCut(turn, now)), pending };
}

// Turn lighting (ticket #560): a story's turn, hovered or focused, lights the parts it changed; unfolding it or "Show on map" reveals them.

// The parts a turn lights and reveals: the parts it changed (its Workers' files included) while it is within the cut; none for an older
// turn, a turn with no files, or while a PR brief plays, which keeps the map.
export function turnLight(turn: Pick<Turn, "files" | "started_at" | "ended_at">, placed: Placement, now: number, brief: boolean): string[] {
  return brief || !inCut(turn, now) ? [] : turnParts(turn, placed);
}

export const OLD_TURN = "Files older than 7 days aren't shown on the map";

// Whether a turn says `OLD_TURN`: it is past the cut, and its agent records turn files (an agent that never does blames no rule).
export function oldTurn(turn: Pick<Turn, "started_at" | "ended_at">, trees: boolean, now: number): boolean {
  return trees && !inCut(turn, now);
}

// Session badges (ticket #562): one count badge per part that marked sessions' latest turns touch.

// A badge's session: the session, and its running Workers whose files of the latest turn are on the part.
export type BadgeRow = { session: Session; workers: Subagent[] };
// A part's badge: its sessions, Needs you first, then in the order given; `accent` when any of them needs you.
export type Badge = { rows: BadgeRow[]; accent: boolean };

// The running Workers of a session, and whether a file is theirs.
const runningWorkers = (session: Session) => session.agents.filter((agent) => agent.kind === "worker" && agent.status === "running");
const workerFile = (worker: Subagent, file: TurnFile) => file.agent === worker.id || file.agent === worker.name;

// The paths of a session's latest turn that count now, while within the cut: its own files and its running Workers' (a finished Worker's no
// longer count, though the footprint keeps them).
function latestPaths(session: Session, now: number): string[] {
  const latest = session.turns.at(-1);
  const workers = runningWorkers(session);
  const files = latest && inCut(latest, now) ? (latest.files?.files ?? []) : [];
  return files.filter((file) => file.agent === null || workers.some((worker) => workerFile(worker, file))).map((file) => file.path);
}

// The parts a session's latest turn touches now: the parts of `latestPaths`, and its in-flight edit's while Running.
export function latestParts(session: Session, placed: Placement, now: number): string[] {
  const flight = inFlight(session);
  return partsOf([...latestPaths(session, now), ...(flight ? [flight] : [])], placed);
}

// The badges of the parts, from the sessions in the session list's order: each marked session (`marked`) counted once on every part of its
// latest turn, with its running Workers whose files are there; the marked sessions whose latest turn touches no part are `notPlaced` (the map says only which of them need you, `notPlacedLine`).
export function sessionBadges(sessions: Session[], placed: Placement, now: number, inProject: (project: string) => boolean): { byPart: Record<string, Badge>; notPlaced: Session[] } {
  const byPart: Record<string, Badge> = {};
  const notPlaced: Session[] = [];
  const first = [...sessions.filter((one) => one.state === "needs_you"), ...sessions.filter((one) => one.state !== "needs_you")];
  for (const session of first.filter((one) => marked(one, inProject))) {
    const parts = latestParts(session, placed, now);
    if (!parts.length) notPlaced.push(session);
    const files = session.turns.at(-1)?.files?.files ?? [];
    const workers = runningWorkers(session);
    for (const part of parts) {
      const badge = byPart[part] ?? { rows: [], accent: false };
      byPart[part] = badge;
      const on = (worker: Subagent) => files.some((file) => workerFile(worker, file) && (placed[file.path] ?? []).includes(part));
      badge.rows.push({ session, workers: workers.filter(on) });
      badge.accent ||= session.state === "needs_you";
    }
  }
  return { byPart, notPlaced };
}

// The home part of a session, where its line sits while it is spotlit: while Running, the part that owns the in-flight edit's file; else the
// part holding most of its latest turn's counted files (`latestPaths`, so always one of the chips' parts); a tie goes to the map's reading
// `order`. Null with no part.
export function homePart(session: Session, placed: Placement, now: number, order: string[]): string | null {
  const flight = inFlight(session);
  const owner = flight ? placed[flight]?.[0] : undefined;
  if (owner !== undefined) return owner;
  const count = new Map<string, number>();
  for (const path of latestPaths(session, now)) for (const part of placed[path] ?? []) count.set(part, (count.get(part) ?? 0) + 1);
  const rank = (part: string) => (order.includes(part) ? order.indexOf(part) : order.length);
  return [...count].sort(([a, x], [b, y]) => y - x || rank(a) - rank(b))[0]?.[0] ?? null;
}

// The spotlight's view while a session is spotlit: its chip on the parts of its latest turn (`chips`, replacing their badges), its line on `home`, and
// the other badges `dimmed`, but a badge with the Needs-you accent.
export type SpotlightView = { chips: string[]; home: string | null; dimmed: string[] };
export function spotlightView(byPart: Record<string, Badge>, spot: Session | null, placed: Placement, now: number, order: string[]): SpotlightView {
  if (!spot) return { chips: [], home: null, dimmed: [] };
  const chips = latestParts(spot, placed, now);
  const dimmed = Object.entries(byPart).flatMap(([part, badge]) => (chips.includes(part) || badge.accent ? [] : [part]));
  return { chips, home: chips.length ? homePart(spot, placed, now, order) : null, dimmed };
}

// The sessions among those no part holds that need Daniel: the only ones the map says anything about (#694), since the session list already
// shows the rest.
export const askingOf = <T extends Pick<Session, "state">>(notPlaced: T[]): T[] => notPlaced.filter((session) => session.state === "needs_you");

// The one line over the map for the unplaced sessions that need Daniel (#694), "2 need you · not on the map"; null when none does.
export function notPlacedLine(notPlaced: Pick<Session, "state">[]): string | null {
  const asking = askingOf(notPlaced).length;
  return asking ? `${asking} ${asking === 1 ? "needs" : "need"} you \u00B7 not on the map` : null;
}

// The session the line's press opens: the next unplaced one that needs Daniel after the session open in the left panel, in the list's order,
// wrapping; the first with none open (or an open one the line does not count). Null with none.
export const notPlacedTarget = (notPlaced: Pick<Session, "id" | "state">[], open: string | null): string | null => nextNeedsYou(askingOf(notPlaced), open);
