// The projects the list groups by (#97, #326, #381): a repository named by its main checkout, a task's worktree path
// (`<main>/.claude/worktrees/<task>`) in its main checkout's project, any other folder its own. The server keeps them as the
// saved project list (`server/src/projects.rs`), so a project keeps its header after its sessions go; the Map view lists the same.

import { studio } from "./state.js";
import type { SavedProject, Session, SessionState } from "./types.js";

export const liveStates: SessionState[] = ["needs_you", "running", "your_turn", "idle"];
export type Rollup = "needs_you" | "running" | null;
export type StateGroup<S> = { state: SessionState; sessions: S[] };
export type ProjectGroup<S> = {
  // The repository's main checkout.
  key: string;
  name: string;
  rollup: Rollup;
  states: StateGroup<S>[];
  // Its folder was moved or deleted.
  gone: boolean;
};

export const lastSegment = (path: string) => path.split("/").filter(Boolean).pop() ?? "";

// The main checkout a task's worktree path names, else the folder itself.
export const repositoryOf = (cwd: string) => /^(.*)\/\.claude\/worktrees\/[^/]+/.exec(cwd)?.[1] || cwd;

// A project's name: its folder's, else "~".
export const projectName = (path: string) => lastSegment(path) || "~";

// The name of the project a session belongs to.
export const sessionProjectName = (session: Pick<Session, "cwd">): string => projectName(repositoryOf(session.cwd));

const urgency = (sessions: Pick<Session, "state">[]): Rollup =>
  sessions.some(({ state }) => state === "needs_you") ? "needs_you" : sessions.some(({ state }) => state === "running") ? "running" : null;

// The sidebar's groups: one per repository, in the order its first live session is listed, then each saved project with no
// live session, as a header only, in the saved order. Inside a group the sessions are by state, and the header's rollup is
// its most urgent state. Ended sessions are in History.
export function projectGroups<S extends Pick<Session, "cwd" | "state">>(sessions: S[], saved: SavedProject[] = []): ProjectGroup<S>[] {
  const by = new Map<string, S[]>();
  for (const session of sessions.filter(({ state }) => liveStates.includes(state))) {
    const key = repositoryOf(session.cwd);
    by.set(key, [...(by.get(key) ?? []), session]);
  }
  const gone = new Set(saved.filter((project) => project.gone).map((project) => project.path));
  for (const { path } of saved) if (!by.has(path)) by.set(path, []);
  return [...by].map(([key, members]) => ({
    key,
    name: projectName(key),
    rollup: urgency(members),
    states: liveStates.flatMap((state) => {
      const here = members.filter((session) => session.state === state);
      return here.length ? [{ state, sessions: here }] : [];
    }),
    gone: gone.has(key),
  }));
}

// The sessions in the sidebar's order: project by project, by state, without ended ones. The session keys step over it.
export const sidebarOrder = <S extends Pick<Session, "cwd" | "state">>(sessions: S[]): S[] =>
  projectGroups(sessions).flatMap((group) => group.states.flatMap((state) => state.sessions));

// The saved projects, so the Map view reaches a project whose sessions all ended or were deleted.
export const knownProjects = (): string[] => studio.projects.map((project) => project.path);

// Whether `path` is a saved project whose folder was moved or deleted.
export const projectGone = (path: string): boolean => studio.projects.some((project) => project.path === path && project.gone);

// The project a folder belongs to, and the one the selected session belongs to.
export const projectOfCwd = (cwd: string): string => repositoryOf(cwd);
export const selectedProject = (): string | null => {
  const cwd = studio.selection?.session.cwd;
  return cwd ? repositoryOf(cwd) : null;
};
