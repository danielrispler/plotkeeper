// The projects the list groups by (#97, #326, #381): each session's project folder, which the core sets once from the folder it started
// in (spec #659: git's top level, else that folder, a task's worktree's main checkout), so a `cd` never moves a session to another
// project. The page never derives a project from a session's folder. The server keeps them as the saved project list
// (`server/src/projects.rs`), so a project keeps its header after its sessions go; the Map view lists the same.

import { studio } from "./state.js";
import type { SavedProject, Session, SessionState } from "./types.js";

const liveStates: SessionState[] = ["needs_you", "running", "your_turn", "idle"];
type Rollup = "needs_you" | "running" | null;
type StateGroup<S> = { state: SessionState; sessions: S[] };
export type ProjectGroup<S> = {
  // The repository's main checkout.
  key: string;
  name: string;
  rollup: Rollup;
  states: StateGroup<S>[];
  // Its folder was moved or deleted.
  gone: boolean;
  // It has a live session, also one Clear landed hid from the list.
  live: boolean;
};

const lastSegment = (path: string) => path.split("/").filter(Boolean).pop() ?? "";

// A project's name: its folder's, else "~".
const projectName = (path: string) => lastSegment(path) || "~";

// The name of the project a session belongs to.
export const sessionProjectName = (session: Pick<Session, "project">): string => projectName(session.project);

const urgency = (sessions: Pick<Session, "state">[]): Rollup =>
  sessions.some(({ state }) => state === "needs_you") ? "needs_you" : sessions.some(({ state }) => state === "running") ? "running" : null;

// The sidebar's groups: one per repository, in the order its first live session is listed, then each saved project with no
// live session, as a header only, in the saved order, but for one that is gone or temporary (spec #659), which the server still lists so a
// gone one comes back with its folder. Inside a group the sessions are by state, and the header's rollup is its most urgent state. Ended
// sessions are in History. The sessions in `hidden` (Clear landed's, #382) are left out of the groups, but still make their project live.
export function projectGroups<S extends Pick<Session, "project" | "state" | "id">>(sessions: S[], saved: SavedProject[] = [], hidden: ReadonlySet<string> = new Set()): ProjectGroup<S>[] {
  const by = new Map<string, S[]>();
  const live = new Set<string>();
  for (const session of sessions.filter(({ state }) => liveStates.includes(state))) {
    const key = session.project;
    live.add(key);
    if (!hidden.has(session.id)) by.set(key, [...(by.get(key) ?? []), session]);
  }
  const gone = new Set(saved.filter((project) => project.gone).map((project) => project.path));
  for (const project of saved) if (!by.has(project.path) && (live.has(project.path) || !(project.gone || project.temporary))) by.set(project.path, []);
  return [...by].map(([key, members]) => ({
    key,
    name: projectName(key),
    rollup: urgency(members),
    states: liveStates.flatMap((state) => {
      const here = members.filter((session) => session.state === state);
      return here.length ? [{ state, sessions: here }] : [];
    }),
    gone: gone.has(key),
    live: live.has(key),
  }));
}

// The sessions in the sidebar's order: project by project, by state, without ended ones. The session keys step over it.
export const sidebarOrder = <S extends Pick<Session, "project" | "state" | "id">>(sessions: S[]): S[] =>
  projectGroups(sessions).flatMap((group) => group.states.flatMap((state) => state.sessions));

// The saved projects, so the Map view reaches a project whose sessions all ended or were deleted.
export const knownProjects = (): string[] => studio.projects.map((project) => project.path);

// Whether `path` is a saved project whose folder was moved or deleted.
export const projectGone = (path: string): boolean => studio.projects.some((project) => project.path === path && project.gone);

// The project the selected session belongs to.
export const selectedProject = (): string | null => studio.selection?.session.project || null;
