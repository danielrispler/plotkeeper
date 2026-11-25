// The live link's rules (spec danielrispler/plotkeeper#549, ticket #559): pure functions over the sessions the page already holds, which
// the map's drawing (`map-agents.ts`) and the story read. Placement is per path: `map_place` is asked one group per distinct path, and its
// answer (`Placement`, a path to the part that owns it) is folded back here per session, Worker and turn.

import type { Session, Turn } from "./types.js";

// The page's 7-day cut, mirroring core's `SINCE_MS` (`core/src/turn_files.rs`): core asks for a turn's files only within it, but keeps
// them after, so the page applies the cut itself.
export const CUT_MS = 7 * 24 * 60 * 60 * 1000;

// Whether a turn counts: its end, or its start if it never ended, is within the cut of `now`, whatever files it still holds.
export const inCut = (turn: Pick<Turn, "started_at" | "ended_at">, now: number) => (turn.ended_at ?? turn.started_at) + CUT_MS >= now;

const editing = ["Edit", "Write", "MultiEdit", "NotebookEdit"];

// The file of a Running session's edit in flight (Edit, Write, MultiEdit, NotebookEdit), from its folder; null otherwise, so the last
// edit of a session that stopped never counts.
export function inFlight(session: Pick<Session, "state" | "last_step" | "cwd">): string | null {
  const step = session.last_step;
  const path = session.state === "running" && step && editing.includes(step.tool) ? step.detail : null;
  if (!path) return null;
  return path.startsWith(`${session.cwd}/`) ? path.slice(session.cwd.length + 1) : path;
}

// A session's turns within the cut, with their index in `session.turns`.
export const recentTurns = (session: Pick<Session, "turns">, now: number) =>
  session.turns.flatMap((turn, index) => (inCut(turn, now) ? [{ turn, index }] : []));

// The paths `map_place` is asked about for `sessions` (the shown project's): every file of every turn within the cut, Workers' included,
// and each Running session's in-flight edit, plus `extra` (the live chips' own paths); one group per distinct path, keyed by the path.
export function placeGroups(sessions: Session[], now: number, extra: string[] = []): Record<string, string[]> {
  const paths = new Set<string>();
  for (const session of sessions) {
    for (const { turn } of recentTurns(session, now)) for (const file of turn.files?.files ?? []) paths.add(file.path);
    const flight = inFlight(session);
    if (flight) paths.add(flight);
  }
  for (const path of extra) paths.add(path);
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
export const partsOf = (paths: string[], placed: Placement): string[] => [...new Set(paths.flatMap((path) => placed[path] ?? []))];

// The parts a turn changed, Workers' files included; none while its files are not diffed.
export const turnParts = (turn: Pick<Turn, "files">, placed: Placement): string[] => partsOf((turn.files?.files ?? []).map((file) => file.path), placed);

// Whether a session is marked, wants Daniel's attention on the map: Running or Needs you, or Your turn or Idle while unseen. Never a plain
// shell or an Ended session; `inProject` keeps another project's out. (A Helper is an agent of a session, never a session.)
export function marked(session: Session, inProject: (cwd: string) => boolean): boolean {
  if (session.shell || !inProject(session.cwd)) return false;
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
