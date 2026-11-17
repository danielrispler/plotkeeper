// Workspaces (danielrispler/Falinks#576; danielrispler/Falinks#565, decision 6; #97): the home of tasks, the
// rows under each project's sessions in the sidebar. One row per workspace, the main checkout and each task, with
// its git tokens (the same as the Git line's, #26) and how many sessions work in it; the sessions
// themselves stay in their project's group. The Git line in the top bar shows the tokens of the selected
// session's workspace. Clicking a workspace offers a new session there and, for a task, Remove, with the
// loss check of a session's Delete. Creating a task is here too, not in the new-session picker.

import { element, hover, invoke, patch, pressable, text } from "./dom.js";
import { knownProjects, mainProject, onProjects, projects } from "./projects.js";
import { drawSidebar, glyph, projectOpen } from "./list.js";
import { newTask, showBranches, startLast } from "./picker.js";
import { studio } from "./state.js";
import { removeWorktree } from "./story.js";
import { throttle } from "./throttle.js";
import type { Branch, GitState, Workspace, Workspaces, Session, SessionState } from "./types.js";

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

export type Token = { text: string; title: string; bad?: boolean };

// A workspace's git tokens, left to right, each hidden when it says nothing (#26): its branch or
// "detached", the upstream distance "↑2 ↓1" or "not pushed", "main ↓5" when behind origin's default
// branch (not on it, where the upstream distance is the same), "●3" uncommitted files, and "⚠" with
// an operation in progress. The hover title says each in words. The one function for the Git line
// and for Workspaces rows. A task's folder, branch and name are one string, so with the task's `name` the
// branch token drops when it is that string (#160): the surrounding line says it once.
export function gitTokens(state: GitState | null, name?: string): Token[] {
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
  return state.branch !== null && state.branch === name ? tokens.slice(1) : tokens;
}

// Every workspace, the main checkout first.
const allWorkspaces = ({ main, tasks }: Workspaces): Workspace[] => [main, ...tasks];

// The workspace a session in `cwd` works in: the deepest one its folder is in, so a session in a task
// is not counted for the main checkout around it too.
const workspaceOf = (all: Workspace[], cwd: string): Workspace | undefined =>
  all
    .filter(({ path }) => cwd === path || cwd.startsWith(`${path}/`))
    .sort((a, b) => b.path.length - a.path.length)[0];

// The main checkout's Pull ↓N (#164): how far its default branch is behind origin's, and why Pull is refused now, or null.
export type Pull = { behind: number; refused: string | null };
export type WorkspaceRow = { workspace: Workspace; name: string; facts: string; actions: string[]; landed: boolean; pull: Pull | null };

// Why Pull main is refused for a main checkout now, or null: files would change under a session there, local main has
// commits origin lacks (a fast-forward is never forced), or the incoming commits would overwrite changed files. Shared by
// the button and the palette's action.
export function pullRefusal(found: Workspaces, state: GitState | null, sessions: Pick<Session, "cwd" | "state">[]): string | null {
  const blocked = branchBlocked(found, sessions);
  if (blocked) return `Not now: ${blocked}`;
  if (state?.on_default && state.upstream?.[0]) return "Local main has diverged from origin: it has commits origin lacks";
  if (state?.would_overwrite) return "Uncommitted changes would be overwritten";
  return null;
}

// Pull main's button for the main checkout: only with its default branch checked out and behind origin.
function pullOf(found: Workspaces, state: GitState | null, sessions: Pick<Session, "cwd" | "state">[]): Pull | null {
  const behind = state?.on_default ? (state.upstream?.[1] ?? 0) : 0;
  return behind ? { behind, refused: pullRefusal(found, state, sessions) } : null;
}

// The rows: the main checkout, then the tasks, each with its tokens and session count.
export function workspaceRows(found: Workspaces, states: (GitState | null)[], sessions: (Pick<Session, "cwd"> & { landed?: boolean; state?: SessionState })[]): WorkspaceRow[] {
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
  const names = all.map((workspace, i) => (i === 0 ? "Main checkout" : (workspace.branch ?? `${folder(workspace)} (detached)`)));
  return all.map((workspace, i) => ({
    workspace,
    name: names[i],
    // A task's branch shows only when it differs from its name; the main checkout keeps its branch.
    facts: [...gitTokens(states[i] ?? null, i === 0 ? undefined : names[i]).map((token) => token.text), working[i] && plural(working[i], "session")]
      .filter(Boolean)
      .join("  "),
    actions: i === 0 ? ["New session here"] : ["New session here", "Remove"],
    landed: i > 0 && landed[i],
    pull: i === 0 ? pullOf(found, states[0] ?? null, sessions as Pick<Session, "cwd" | "state">[]) : null,
  }));
}

// The sidebar's groups (#97, spec #90, Sidebar): a pure function of the projects, each project's workspaces
// and the listed sessions. A session belongs to the project with the deepest workspace holding its `cwd`;
// one group for each other folder, named after its repository's main checkout, else its folder, else "~", after the projects.
// Inside a group the sessions are by state, and the header's rollup is its most urgent state. Ended sessions are in History.
export const liveStates: SessionState[] = ["needs_you", "running", "your_turn", "idle"];
export type Rollup = "needs_you" | "running" | null;
export type StateGroup<S> = { state: SessionState; sessions: S[] };
export type ProjectGroup<S> = {
  // The project's path, or the outside folder's.
  key: string;
  name: string;
  // False for a folder outside every project.
  project: boolean;
  workspaces: Workspaces | null;
  rollup: Rollup;
  states: StateGroup<S>[];
};

export const lastSegment = (path: string) => path.split("/").filter(Boolean).pop() ?? "";

// The main checkout a task's worktree path names, else the folder itself.
const repositoryOf = (cwd: string) => /^(.*)\/\.claude\/worktrees\/[^/]+/.exec(cwd)?.[1] || cwd;

// The name of a folder outside every project: its repository's main checkout, else its folder, else "~".
export const outsideName = (cwd: string) => lastSegment(repositoryOf(cwd)) || "~";

const lone = (path: string): Workspaces => ({ main: { path, branch: null }, tasks: [], hidden: 0 });

// The project whose workspace holds `cwd`, the deepest of them, with that workspace; null outside every project.
function ownerOf(paths: string[], found: Map<string, Workspaces>, cwd: string): { project: string; workspaces: Workspaces; workspace: Workspace } | null {
  let best: { project: string; workspaces: Workspaces; workspace: Workspace } | null = null;
  for (const project of paths) {
    const workspaces = found.get(project) ?? lone(project);
    const workspace = workspaceOf(allWorkspaces(workspaces), cwd);
    if (workspace && (!best || workspace.path.length > best.workspace.path.length)) best = { project, workspaces, workspace };
  }
  return best;
}

// The name of the project a session belongs to, else its outside group's.
export function sessionProjectName(paths: string[], found: Map<string, Workspaces>, session: Pick<Session, "cwd">): string {
  const owner = ownerOf(paths, found, session.cwd);
  return owner ? lastSegment(owner.project) || owner.project : outsideName(session.cwd);
}

// A session row's workspace-and-branch token (#97): the main checkout's branch, or a task's name and branch.
// The branch is the git line's first token when the workspace's state is known, else the worktree list's.
export function workspaceToken(workspaces: Workspaces, cwd: string, state: (path: string) => GitState | null = () => null): string | null {
  const workspace = workspaceOf(allWorkspaces(workspaces), cwd);
  if (!workspace) return null;
  const branch = (state(workspace.path) ? gitTokens(state(workspace.path))[0]?.text : workspace.branch) ?? "detached";
  if (workspace === workspaces.main) return branch;
  const name = lastSegment(workspace.path);
  return branch === name || branch === "detached" ? name : `${name} · ${branch}`;
}

const urgency = (sessions: Pick<Session, "state">[]): Rollup =>
  sessions.some(({ state }) => state === "needs_you") ? "needs_you" : sessions.some(({ state }) => state === "running") ? "running" : null;

export function projectGroups<S extends Pick<Session, "cwd" | "state">>(paths: string[], found: Map<string, Workspaces>, sessions: S[]): ProjectGroup<S>[] {
  const live = sessions.filter(({ state }) => liveStates.includes(state));
  const inProject = new Map<string, S[]>(paths.map((path) => [path, []]));
  const outside = new Map<string, S[]>();
  for (const session of live) {
    const owner = ownerOf(paths, found, session.cwd);
    if (owner) inProject.get(owner.project)!.push(session);
    else {
      const key = repositoryOf(session.cwd);
      outside.set(key, [...(outside.get(key) ?? []), session]);
    }
  }
  const group = (key: string, name: string, project: boolean, members: S[]): ProjectGroup<S> => ({
    key,
    name,
    project,
    workspaces: found.get(key) ?? null,
    rollup: urgency(members),
    states: liveStates.flatMap((state) => {
      const here = members.filter((session) => session.state === state);
      return here.length ? [{ state, sessions: here }] : [];
    }),
  });
  return [
    ...paths.map((path) => group(path, lastSegment(path) || path, true, inProject.get(path)!)),
    ...[...outside].map(([key, members]) => group(key, outsideName(key), false, members)),
  ];
}

// The sessions in the sidebar's order: project by project, by state, without ended ones. The session keys step over it.
export const sidebarOrder = <S extends Pick<Session, "cwd" | "state">>(paths: string[], found: Map<string, Workspaces>, sessions: S[]): S[] =>
  projectGroups(paths, found, sessions).flatMap((group) => group.states.flatMap((state) => state.sessions));

// Why the main checkout's branch cannot be switched now (#95), or null when it can: a session working in
// the main checkout itself, not in a task's worktree, is running or needs Daniel, so its files must not change
// under it. A pure function of the listed sessions.
export function branchBlocked(found: Workspaces, sessions: Pick<Session, "cwd" | "state">[]): string | null {
  const all = allWorkspaces(found);
  const here = sessions.filter(({ cwd }) => workspaceOf(all, cwd) === found.main);
  if (here.some(({ state }) => state === "needs_you")) return "a session here needs you";
  if (here.some(({ state }) => state === "running")) return "a session here is running";
  return null;
}

export type BranchOption = { branch: Branch; label: string; note: string; disabled: boolean };

// The branch picker's rows: every branch, disabled all together by `blocked`, and one a task holds or the
// main checkout is on by its own reason (#95).
export function branchOptions(branches: Branch[], blocked: string | null): BranchOption[] {
  return branches.map((branch) => {
    const note = branch.task ? `in task ${branch.task}` : branch.current ? "current" : branch.remote ? "at origin" : "";
    return { branch, label: branch.name, note, disabled: blocked !== null || branch.task !== null || branch.current };
  });
}

// The last workspaces read of each project, each workspace's git state by path, and the workspaces whose actions show.
const read = new Map<string, Workspaces>();
const states = new Map<string, GitState | null>();
// A project's latest error, git's or any other, shown in its group until the next one.
const failures = new Map<string, string>();
let reading = false;
// A forced read asked for while one runs: one more read follows it.
let again = false;
let readAt = 0;
const opened = new Set<string>();

// Drops what `maps` hold for a project no longer in `paths`, so a removed project's git line, fetch and workspaces go.
export function keepProjects(paths: string[], ...maps: Map<string, unknown>[]) {
  for (const map of maps) for (const key of [...map.keys()]) if (!paths.includes(key)) map.delete(key);
}
const pruneRemoved = () => keepProjects(knownProjects(), read, failures, fetched);

// The first of `paths`, in project order, that `found` has: where the Git line looks with no selection.
export const firstProject = (paths: string[], found: Map<string, unknown>): string | undefined => paths.find((path) => found.has(path));

const fail = (project: string) => (error: unknown) => {
  failures.set(project, String(error));
  drawSidebar();
};

// The workspace rows of a project's group (#97): its Workspaces, then the new-task field and its error.
export function workspaceSection(project: string, sessions: Pick<Session, "cwd" | "landed">[]): HTMLElement[] {
  const found = read.get(project);
  if (!found) return [];
  const rows = workspaceRows(found, allWorkspaces(found).map(({ path }) => states.get(path) ?? null), sessions);
  const error = failures.get(project);
  const message = text("pre", error ?? "", "picker-error");
  message.dataset.key = "error";
  return [...rows.map((row) => workspaceRow(project, row)), creating(project), ...(error ? [message] : [])];
}

// The workspaces of the known projects as last read, by project.
export const foundWorkspaces = (): Map<string, Workspaces> =>
  new Map(knownProjects().flatMap((path) => { const found = read.get(path); return found ? [[path, found] as const] : []; }));

// The token of a session row: its workspace and branch.
export const tokenOf = (project: string, cwd: string): string | null => {
  const found = read.get(project);
  return found ? workspaceToken(found, cwd, (path) => states.get(path) ?? null) : null;
};

// A workspace's row: one line, its name then its facts; a click shows or hides its actions under it.
export function workspaceRow(project: string, { workspace, name, facts, actions, landed, pull }: WorkspaceRow): HTMLElement {
  const row = document.createElement("div");
  row.className = "workspace";
  row.dataset.key = workspace.path;
  const button = pressable(document.createElement("button"), false, () => {
    if (!opened.delete(workspace.path)) opened.add(workspace.path);
    drawSidebar();
  });
  button.classList.add("row");
  button.title = workspace.path;
  button.setAttribute("aria-expanded", String(opened.has(workspace.path)));
  const line = text("span", "", "l1");
  line.append(hover(text("span", name, "name"), name));
  if (facts) line.append(hover(text("span", facts, "facts"), facts));
  if (landed) line.append(glyph("idle", false, true));
  button.append(line);
  if (pull) {
    // Pull ↓N sits at the end of the row's line, beside its button: a button cannot nest in one.
    const line = text("div", "", "workspace-line");
    line.append(button, pullButton(project, pull));
    row.append(line);
  } else row.append(button);
  if (!opened.has(workspace.path)) return row;
  const act = (label: string) =>
    pressable(text("button", label, "small"), false, () => {
      opened.delete(workspace.path);
      const done = label === "Remove" ? removeWorktree(workspace.path, project).then(() => readWorkspaces(true)) : startLast(workspace.path, "");
      done.catch(fail(project));
    });
  const buttons = text("div", "", "workspace-actions");
  buttons.append(...actions.map(act));
  row.append(buttons);
  return row;
}

// Pull ↓N (#164): fast-forwards the default branch; refused, it is disabled with the reason as its title.
function pullButton(project: string, { behind, refused }: Pull): HTMLButtonElement {
  const button = pressable(text("button", `Pull ↓${behind}`, "small pull") as HTMLButtonElement, false, () => void pull([project]));
  button.dataset.key = "pull";
  button.title = refused ?? `Fast-forward the default branch to origin's, ${plural(behind, "commit")}`;
  button.disabled = refused !== null;
  return button;
}

// A new task in a project, named by its branch; a session starts in it.
function creating(project: string): HTMLElement {
  const form = newTask("New task: branch name", "Create", async (name) => invoke<Workspace>("create_task", { project, name }).finally(() => readWorkspaces(true)), false, fail(project));
  form.dataset.key = "new-task";
  return form;
}

// The workspace the Git line shows, with its project: that of the selected session, or the first project's main checkout with none.
function shown(): { project: string; workspaces: Workspaces; workspace: Workspace } | null {
  const cwd = studio.selection?.session.cwd;
  const owner = cwd ? ownerOf([...read.keys()], read, cwd) : null;
  if (owner) return owner;
  const project = firstProject(knownProjects(), read);
  const workspaces = project === undefined ? undefined : read.get(project);
  return project !== undefined && workspaces ? { project, workspaces, workspace: workspaces.main } : null;
}

// The project the Git line shows, else the first one: where the palette's Switch branch works.
export const shownProject = async (): Promise<string> => shown()?.project ?? mainProject();

// The repository a worktree belongs to: its own main checkout, project or not, never another project; null when
// none is found (so a removal there cannot work).
export const repositoryOfWorktree = async (path: string, ask: (folder: string) => Promise<string | null> = (folder) => invoke<string | null>("repository_of", { folder })): Promise<string | null> =>
  (await ask(path).catch(() => null)) || null;

// The Git line: the workspace of the selected session, or the main checkout's with none, and its tokens.
function drawLine() {
  const line = element("git-line");
  const now = shown();
  line.hidden = !now;
  if (!now) return;
  const { project, workspaces, workspace } = now;
  const repo = lastSegment(workspaces.main.path);
  const name = workspace === workspaces.main ? repo : `${repo} › ${lastSegment(workspace.path)}`;
  const state = states.get(workspace.path) ?? null;
  const tokens = gitTokens(state, workspace === workspaces.main ? undefined : lastSegment(workspace.path));
  line.title = [[name, ...tokens.map((token) => token.text)].join("  "), ...gitTokens(state).map((token) => token.title), fetchNote(fetched.get(project) ?? { at: null, error: null }, Date.now())].join("\n");
  // The main checkout's branch token opens the branch picker (#95); a task's worktree keeps its branch.
  const shownTokens = tokens.map((token, i) => {
    if (i > 0 || workspace !== workspaces.main) return Object.assign(text("span", token.text, token.bad ? "git-bad" : ""), { title: token.title });
    const button = pressable(text("button", token.text, "git-branch"), false, () => void showBranches(project));
    button.title = `${token.title}\nSwitch branch`;
    return button;
  });
  line.replaceChildren(text("span", name, "git-workspace"), ...shownTokens);
}

function draw() {
  drawLine();
  drawSidebar();
}

// A workspace's state is read behind a throttle each (#26, Freshness, local state): at once on the first
// event, then once per 2 s while events keep coming, and once after the last.
const throttles = new Map<string, () => void>();
function touch(project: string, path: string) {
  let event = throttles.get(path);
  if (!event) {
    const readState = () =>
      invoke<GitState>("workspace_git_state", { project, path })
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

// The workspaces whose state shows: every workspace of a project that is not collapsed, and the Git line's.
function touchShown() {
  const now = shown();
  if (now) touch(now.project, now.workspace.path);
  for (const [project, found] of read) if (projectOpen(project)) for (const workspace of allWorkspaces(found)) touch(project, workspace.path);
}

// The configured projects, null when the read failed: the sidebar then shows why.
async function readProjects(): Promise<string[] | null> {
  try {
    return await projects();
  } catch {
    drawSidebar();
    return null;
  }
}

// Reads the workspaces of every project from git. At most one read at a time and, unless `now`, one per 10 s, as the list
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
    const paths = await readProjects();
    if (!paths) {
      // The next read tries again, not 10 s later.
      readAt = 0;
      return;
    }
    pruneRemoved();
    await Promise.all(
      paths.map(async (project) => {
        try {
          read.set(project, await invoke<Workspaces>("workspaces", { project }));
          failures.delete(project);
        } catch (e) {
          failures.set(project, String(e));
        }
      }),
    );
    pruneRemoved();
    draw();
    touchShown();
  } finally {
    reading = false;
  }
  if (again) {
    again = false;
    await readWorkspaces(true);
  }
}

// A project added or removed: every project's workspaces are read again at once.
onProjects(() => {
  pruneRemoved();
  void readWorkspaces(true);
});

// The list was drawn, or the selection changed: the session counts follow at once, and the state of
// every workspace a session works in is read behind its throttle, a hook event having come.
export function showWorkspaces() {
  drawLine();
  readWorkspaces();
  if (!read.size) return;
  const paths = [...read.keys()];
  const working = new Set(studio.listed.map(({ cwd }) => ownerOf(paths, read, cwd)?.workspace.path));
  for (const [project, found] of read) for (const workspace of allWorkspaces(found)) if (working.has(workspace.path)) touch(project, workspace.path);
  touchShown();
}

// After a branch switch: Workspaces and the main checkout's git state are read again at once.
export async function refreshAfterSwitch(project: string) {
  await readWorkspaces(true);
  const found = read.get(project);
  if (found) touch(project, found.main.path);
}

// A repository's fetch, once for all its workspaces (#33): when it last succeeded, and git's message
// if the latest failed. A failed fetch leaves the refs as they were, so the old numbers stay.
export type FetchState = { at: number | null; error: string | null };
const fetched = new Map<string, FetchState>();
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

// Fetches every project now, then reads every workspace again; the palette's Fetch and the 5-minute timer. One at a time.
export async function fetchNow() {
  if (fetching) return;
  fetching = true;
  try {
    const paths = await readProjects();
    if (!paths) return;
    await Promise.all(
      paths.map(async (project) => {
        try {
          await invoke("fetch_repository", { project });
          fetched.set(project, { at: Date.now(), error: null });
        } catch (e) {
          fetched.set(project, { at: fetched.get(project)?.at ?? null, error: String(e) });
        }
      }),
    );
  } finally {
    fetching = false;
  }
  for (const [project, found] of read) for (const workspace of allWorkspaces(found)) touch(project, workspace.path);
  draw();
}

// The repositories are fetched once as watching starts, so the first distances are not stale. Then,
// while the window is focused, every shown workspace is read every 5 s and the repositories are fetched
// every 5 minutes; started by `main.ts`, not on import.
export function watchWorkspaces() {
  void fetchNow();
  setInterval(() => document.hasFocus() && touchShown(), 5000);
  setInterval(() => document.hasFocus() && void fetchNow(), 5 * 60_000);
}

// Pulls each of `paths`' main checkout (#164), then reads every workspace and each main checkout's git state again, so the
// rows and the git line show the new distance. A pull that failed shows git's reason on its project's group, after that read.
async function pull(paths: string[]) {
  const errors = await Promise.all(
    paths.map((project) => {
      const found = read.get(project);
      const refused = found && pullRefusal(found, states.get(found.main.path) ?? null, studio.listed);
      return refused ? Promise.resolve(refused) : invoke("pull_main", { project }).then(() => null, (e) => String(e));
    }),
  );
  await readWorkspaces(true);
  paths.forEach((project, i) => {
    const found = read.get(project);
    if (found) touch(project, found.main.path);
    if (errors[i]) failures.set(project, errors[i]);
  });
  drawSidebar();
}

// Pull main for every project, the palette's action (#164).
export async function pullAll() {
  const paths = await readProjects();
  if (paths) await pull(paths);
}
