// The projects the list groups by (#97, #326): the page has no project registry, so a project is a repository some session
// ran in, named by its main checkout: a task's worktree path (`<main>/.claude/worktrees/<task>`) is its main checkout's
// project, any other folder is its own. A pure function of the listed sessions; the Map view lists the same repositories.

import { studio } from "./state.js";
import type { Session, SessionState } from "./types.js";

export const liveStates: SessionState[] = ["needs_you", "running", "your_turn", "idle"];
export type Rollup = "needs_you" | "running" | null;
export type StateGroup<S> = { state: SessionState; sessions: S[] };
export type ProjectGroup<S> = {
  // The repository's main checkout.
  key: string;
  name: string;
  rollup: Rollup;
  states: StateGroup<S>[];
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

// The sidebar's groups: one per repository, in the order its first live session is listed. Inside a group the sessions are by
// state, and the header's rollup is its most urgent state. Ended sessions are in History.
export function projectGroups<S extends Pick<Session, "cwd" | "state">>(sessions: S[]): ProjectGroup<S>[] {
  const by = new Map<string, S[]>();
  for (const session of sessions.filter(({ state }) => liveStates.includes(state))) {
    const key = repositoryOf(session.cwd);
    by.set(key, [...(by.get(key) ?? []), session]);
  }
  return [...by].map(([key, members]) => ({
    key,
    name: projectName(key),
    rollup: urgency(members),
    states: liveStates.flatMap((state) => {
      const here = members.filter((session) => session.state === state);
      return here.length ? [{ state, sessions: here }] : [];
    }),
  }));
}

// The sessions in the sidebar's order: project by project, by state, without ended ones. The session keys step over it.
export const sidebarOrder = <S extends Pick<Session, "cwd" | "state">>(sessions: S[]): S[] =>
  projectGroups(sessions).flatMap((group) => group.states.flatMap((state) => state.sessions));

// Every repository a listed session ran in, ended ones too, so the Map view reaches a project whose sessions all ended.
export const knownProjects = (): string[] => [...new Set(studio.listed.map((session) => repositoryOf(session.cwd)))];

// The project a folder belongs to, and the one the selected session belongs to.
export const projectOfCwd = (cwd: string): string => repositoryOf(cwd);
export const selectedProject = (): string | null => {
  const cwd = studio.selection?.session.cwd;
  return cwd ? repositoryOf(cwd) : null;
};
