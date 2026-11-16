// The session list, on the window's right: one line per session, sorted by state, its status shown
// by shape (danielrispler/Falinks#572), each session's subagents nested under it. When the window is narrow it collapses to
// a strip of glyphs and labels, which opens the full list on hover, on click or on ⌘1.

import { agentMark } from "./capabilities.js";
import { age, element, invoke, patch, prose, pressable, stepText, text } from "./dom.js";
import { counts, times, type Time } from "./times.js";
import { shortPaths } from "./paths.js";
import { historyLink } from "./history.js";
import { knownProjects, onProjects, projects, projectsError, setProjects } from "./projects.js";
import { addProject, refuse } from "./picker.js";
import { projectGroups, foundWorkspaces, tokenOf, workspaceSection, type ProjectGroup } from "./workspaces.js";
import { redrawHistory, redrawHome, showHistory } from "./panes.js";
import { isSelected, select } from "./selection.js";
import { studio } from "./state.js";
import { stateLabels, type Context, type Session, type SessionState, type Subagent } from "./types.js";
import { askDelete, report } from "./story.js";

// The state groups folded in a project, idle only, and the idle groups Daniel opened, by project and state,
// which stay open when the list is drawn again.
const collapsed: SessionState[] = ["idle"];
const opened = new Set<string>();
// The projects whose group Daniel folded: its header stays, with its rollup.
const shut = new Set<string>();
export const projectOpen = (key: string) => !shut.has(key);

// What the strip calls a session: the issue number of an `issue-<n>-…` folder, else its folder's
// first letters.
export function stripLabel(folder: string): string {
  return /^issue-(\d+)/.exec(folder)?.[1] ?? folder.slice(0, 4);
}

// What a session is called: Daniel's name for it, else the name written from its first prompt with
// the identity's issue in front ("#37 · One event internal"), else its issue, PR and worktree
// (danielrispler/Falinks#480), else its folder.
export function sessionName(
  session: Pick<Session, "name" | "identity" | "folder_name"> & { prompt_name?: string | null },
): string {
  if (session.name) return session.name;
  if (session.prompt_name) return session.identity.issue === null ? session.prompt_name : `#${session.identity.issue} · ${session.prompt_name}`;
  return session.identity.name || session.folder_name;
}

const thousands = (n: number) => `${Math.round(n / 1000).toLocaleString("en-US")}k`;

// How full a session's context is (danielrispler/Falinks#517, decisions 2 and 3): "context 9%", its tokens such as "85k"
// when the window is unknown, or "" before it is known.
export function contextText(context: Context | null): string {
  if (!context) return "";
  return context.share === null ? thousands(context.tokens) : `context ${context.share}%`;
}

// The context share's hover: its tokens of the window, and what Claude's status line shows: its own
// percent left.
export function contextTitle(context: Context | null): string {
  if (!context) return "";
  const used = thousands(context.tokens);
  if (context.window === null || context.left === null) return `${used} tokens used · the window size is unknown`;
  return `${used} of ${thousands(context.window)} tokens used · Claude's status line shows this as ${context.left}% left`;
}

// The muted mark after a row's name: "terminal" for a plain terminal's row, "status only" for another agent's row
// (danielrispler/Falinks#560), so it never reads as a Claude session with missing data; else the context share.
export function rowMark(session: Pick<Session, "shell" | "status_only" | "transcript" | "capabilities">): string {
  if (session.shell) return "terminal";
  if (session.status_only) return "status only";
  return agentMark(session) || contextText(session.transcript.context);
}

// The name's hover: the identity's PR and worktree, which a prompt name leaves out.
function nameTitle(session: Session): string {
  return session.name || !session.prompt_name ? "" : session.identity.name;
}

// The mark's node, with the context share's hover.
function markNode(session: Session): HTMLElement {
  const title = session.shell
    ? "A plain terminal"
    : session.status_only
    ? "Another agent, followed by its reported status only"
    : agentMark(session)
      ? `${agentMark(session)}: followed by what it reports, which has no context share`
      : contextTitle(session.transcript.context);
  return Object.assign(text("span", rowMark(session), "id"), { title });
}

// A subagent's state label and its state: "Needs you" while it asks, else its status.
export function agentState(session: Session, agent: Subagent): [string, string] {
  return session.asker?.agent === agent.id ? [stateLabels.needs_you, "needs_you"] : [agent.status, agent.status];
}

// A status's shape and its word (danielrispler/Falinks#572): a "Needs you" pill, the only colour (danielrispler/Falinks#565, decision 1); a
// spinner while running; a dot at your turn, filled once it finished unseen, idle too (danielrispler/Falinks#556); a ring when idle,
// ended or, for a subagent, done.
export type Glyph = "pill" | "spinner" | "dot" | "filled" | "ring" | "check";
// A landed task's session at your turn, idle or ended shows a check mark and these words instead (#53, Landed).
export const landedWord = "Landed — safe to delete";
export const showsLanded = (state: string, landed: boolean) => landed && ["your_turn", "idle", "ended"].includes(state);
export function status(state: string, unseen = false, landed = false): [Glyph, string] {
  const label = stateLabels[state as SessionState] ?? state;
  if (showsLanded(state, landed)) return ["check", landedWord];
  if (state === "needs_you") return ["pill", label];
  if (state === "running") return ["spinner", label];
  // The core marks a turn unseen only at your turn and once idle after it, which keeps the mark.
  if (unseen) return ["filled", `${label}, not seen`];
  if (state === "your_turn") return ["dot", label];
  return ["ring", label];
}

// A status's glyph, named by its word, so the shape is never alone. The pill shows its word.
export function glyph(state: string, unseen = false, landed = false): HTMLElement {
  const [shape, word] = status(state, unseen, landed);
  const node = text("span", shape === "pill" ? word : "", "glyph");
  node.dataset.glyph = shape;
  node.setAttribute("role", "img");
  node.setAttribute("aria-label", word);
  if (shape === "check") node.title = word;
  return node;
}

// A row's second line, only while it runs or needs Daniel: the current step, or the open question's
// first line, else who asks (a plan or a permission); null otherwise.
export function secondLine(session: Pick<Session, "state" | "line" | "questions"> & { status_only?: boolean }): string | null {
  // A status-only row has no step or question to show (danielrispler/Falinks#560).
  if (session.status_only) return null;
  if (session.state === "running") return session.line ?? "";
  if (session.state !== "needs_you") return null;
  const open = session.questions.filter((question) => question.answer === "open").at(-1);
  // A question with no text, as a redacted log holds, falls back to the row's line rather than throw.
  return open?.questions[0]?.question?.split("\n")[0] ?? session.line ?? "";
}

// The top bar's chip: how many sessions need Daniel, or "" when none.
export function chipText(sessions: Pick<Session, "state">[]): string {
  const count = sessions.filter((session) => session.state === "needs_you").length;
  return count ? `${count} ${count === 1 ? "needs" : "need"} you` : "";
}

// A session's subagents by the id of the agent that spawned them: "session", "unattached" or a
// subagent's id. Built once per draw, so the tree and the "done" counts do not scan every subagent.
type Children = Map<string, Subagent[]>;
export function childrenOf(session: Pick<Session, "agents">): Children {
  const children: Children = new Map();
  for (const agent of session.agents) {
    const parent = typeof agent.parent === "string" ? agent.parent : agent.parent.agent;
    if (!children.has(parent)) children.set(parent, []);
    children.get(parent)!.push(agent);
  }
  return children;
}

// Whether a subagent's line is not drawn: it faded, and nothing below it is drawn (danielrispler/Falinks#434, decision 43).
export function hidden(children: Children, agent: Subagent): boolean {
  return agent.faded && (children.get(agent.id) ?? []).every((child) => hidden(children, child));
}

// The count of a parent's subagents that faded into its line, workers as helpers (danielrispler/Falinks#535).
export const doneCount = (all: Children, children: Subagent[] = []) => children.filter((agent) => hidden(all, agent)).length;
// Whether a subagent shows under a collapsed row: it needs Daniel, or it failed and he has not opened it.
const standsOut = (session: Pick<Session, "asker">, agent: Subagent) =>
  session.asker?.agent === agent.id || (agent.failed && !agent.opened);

// The rows collapsed, by session id then "session" or a subagent's id, kept while the app runs.
const folded = new Set<string>();
type Folded = { has(id: string): boolean };
const foldedIn = (session: Pick<Session, "id">): Folded => ({ has: (id) => folded.has(`${session.id} ${id}`) });

// The subagents drawn under `parent`, at any depth, each with its depth. Under a collapsed row only
// those that stand out are drawn, at their own depth.
export type Drawn = { agent: Subagent; depth: number };
export function drawn(session: Pick<Session, "asker">, children: Children, parent: string, depth: number, shut: Folded, under = false): Drawn[] {
  return (children.get(parent) ?? []).flatMap((agent) => {
    const below = drawn(session, children, agent.id, depth + 1, shut, under || shut.has(agent.id));
    const shown = !hidden(children, agent) && (!under || standsOut(session, agent));
    return [...(shown ? [{ agent, depth }] : []), ...below];
  });
}

// A row's fold toggle, for the row keyed `parents[0]` whose subagents spawn from `parents`: whether it
// is open and how many rows it hides; null when nothing is drawn under it.
export function folds(session: Pick<Session, "asker">, children: Children, parents: string[], shut: Folded): { open: boolean; hiding: number } | null {
  const count = (under: boolean) => parents.flatMap((parent) => drawn(session, children, parent, 0, shut, under)).length;
  const all = count(false);
  if (!all) return null;
  const open = !shut.has(parents[0]);
  return { open, hiding: open ? 0 : all - count(true) };
}

function done(all: Children, children: Subagent[] = []): HTMLElement[] {
  const faded = doneCount(all, children);
  return faded ? [text("span", `${faded} done`, "id")] : [];
}

// A row's lines: its name and status glyph, then, if any, what it is doing, muted. Its title pairs
// the name with the status's word.
function rowLines(button: HTMLElement, state: string, name: HTMLElement[], below: HTMLElement[] | null, unseen = false, landed = false) {
  const first = text("span", "", "l1");
  first.append(...name, glyph(state, unseen, landed));
  button.append(first);
  button.title = `${name[0].textContent} · ${status(state, unseen, landed)[1]}`;
  button.classList.toggle("unseen", unseen);
  if (below) {
    const second = text("span", "", "l2");
    second.append(...below);
    button.append(second);
  }
  return first;
}

// A row's button, keyed by what it selects (`patch` keeps it across updates). It selects on the press,
// so the row shows selected at once (danielrispler/Falinks#570); a click without a pointer, from the keyboard, selects too.
function selecting(id: string, agent: string | null, key = `${id} ${agent ?? ""}`): HTMLButtonElement {
  const button = pressable(document.createElement("button"), isSelected(id, agent), (event: MouseEvent) => event.detail === 0 && select(id, agent));
  button.dataset.key = key;
  button.addEventListener("pointerdown", (event) => event.button === 0 && select(id, agent));
  return button;
}

// One row: the session's name, its mark and its status glyph; while it runs or needs
// Daniel, what it is doing and, on the row being looked at, the turn's clock.
export function row(session: Session, children: Children, token: string | null = null): HTMLElement {
  const button = selecting(session.id, null);
  button.classList.add("row", "session-row");
  const line = secondLine(session);
  const turn = line === null ? null : rowTime(session, isSelected(session.id));
  const clock = text("span", "", "clock");
  rowLines(
    button,
    session.state,
    [Object.assign(text("span", sessionName(session), "name"), { title: nameTitle(session) }), markNode(session), ...(token ? [text("span", token, "id token")] : [])],
    line === null
      ? null
      : [...done(children, children.get("session")), rowLine(line), ...(turn ? [Object.assign(counts(clock, turn[1]), { title: turn[0] })] : [])],
    session.unseen,
    session.landed,
  );
  return button;
}

// The row's clock: the turn time, only on the row being looked at; the rail has all three (danielrispler/Falinks#517,
// decision 4; danielrispler/Falinks#571).
export function rowTime(session: Pick<Session, "state" | "started_at" | "last_event_at" | "turn_span">, selected: boolean): Time | null {
  return (selected && session.turn_span && times(session, null)[1]) || null;
}

// A row line as plain text (danielrispler/Falinks#571): Markdown's emphasis, headings and links dropped outside code spans.
export function plain(line: string): string {
  return line
    .split("`")
    .map((part, i) => (i % 2 ? part : part.replace(/\[([^\]]*)\]\([^)]*\)/g, "$1").replace(i ? /\*\*|__/g : /\*\*|__|^#{1,6}\s+/g, "")))
    .join("`");
}

// A row line as shown, and its hover title: the original line whenever a link's URL or a long path's
// start is dropped, so the full line stays reachable (danielrispler/Falinks#571); else "".
export function rowText(line: string): [shown: string, title: string] {
  const [shown, full] = shortPaths(plain(line));
  return [shown, full || /\]\([^)]*\)/.test(line) ? line : ""];
}

// A row's line, such as a card summary stored in Hebrew, laid out by its own direction, its code spans
// cleaned as a card's are (danielrispler/Falinks#434, decision 40), its paths shortened with the full line on hover (danielrispler/Falinks#571).
function rowLine(line: string): HTMLElement {
  const [shown, full] = rowText(line);
  const node = prose("span", shown);
  node.className = "row-step";
  node.dir = "auto";
  if (full) node.title = full;
  return node;
}

// A subagent row's second line, as a session row's (danielrispler/Falinks#572): only while it runs or needs Daniel.
export function agentLine(state: string, details: (string | null)[]): string | null {
  return state === "running" || state === "needs_you" ? details.filter(Boolean).join(" · ") : null;
}

// A subagent's row, indented under the agent that spawned it. A worker shows its task, status and
// last step; a helper its task, type, status, duration and result. A failed child stands out until
// Daniel opens it.
function agentRow(session: Session, children: Children, agent: Subagent, depth: number): HTMLElement {
  const button = selecting(session.id, agent.id);
  button.classList.add("row", agent.kind);
  button.style.paddingInlineStart = `calc(var(--sp-4) + var(--indent) * ${depth + 1})`;
  const [, stateName] = agentState(session, agent);
  const details =
    agent.kind === "worker"
      ? [stepText(agent.last_step)]
      : [agent.agent_type, age(agent.started_at, agent.ended_at ?? Date.now()), agent.result];
  const line = agentLine(stateName, details);
  const first = rowLines(
    button,
    stateName,
    [text("span", agent.name, "name")],
    line === null ? null : [...done(children, children.get(agent.id)), rowLine(line)],
  );
  first.classList.toggle("failed", agent.failed && !agent.opened);
  return button;
}

// A row control's icon: a 14px inline SVG on the current colour, in a 20x20 button (the tokens' --control and --icon).
const CHEVRON = '<path d="M5 3l4 4-4 4"/>';
const CROSS = '<path d="M3.5 3.5l7 7M10.5 3.5l-7 7"/>';
function icon(name: string, path: string, click: () => void): HTMLElement {
  const button = pressable(text("button", "", name), false, click);
  button.removeAttribute("aria-pressed");
  button.innerHTML = `<svg viewBox="0 0 14 14" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">${path}</svg>`;
  return button;
}

// A row in the list: its toggle, when anything is drawn under it, then the row, then its delete. They
// are the row's siblings, as a button holds no button, so pressing them does not select the row.
export function item(key: string, depth: number, toggle: { open: boolean; hiding: number } | null, row: HTMLElement, remove: HTMLElement | null) {
  const node = document.createElement("div");
  node.className = "item";
  node.dataset.key = key;
  if (toggle) {
    const fold = icon("fold", CHEVRON, () => {
      if (!folded.delete(key)) folded.add(key);
      drawSidebar();
    });
    fold.title = toggle.open ? "Collapse subagents" : "Expand subagents";
    fold.setAttribute("aria-expanded", String(toggle.open));
    // The count goes after the name, as the toggle's column has room only for its chevron.
    if (toggle.hiding) row.querySelector(".name")?.after(text("span", `${toggle.hiding} hidden`, "id"));
    fold.style.insetInlineStart = `calc(var(--indent) * ${depth})`;
    node.append(fold);
  }
  node.append(row, ...(remove ? [remove] : []));
  return node;
}

// A session row's delete (decision of the session list's fold and delete): the rail's dialog, asked from the row.
export function deleting(session: Session): HTMLElement | null {
  if (session.status_only) return null;
  // `patch` keeps the first button and its listener, so the press reads the session as it is now.
  const now = () => studio.listed.find((listed) => listed.id === session.id) ?? session;
  const button = icon("delete", CROSS, () => void askDelete(now()).catch(report));
  button.title = "Delete…";
  button.setAttribute("aria-label", `Delete ${sessionName(session)}…`);
  return button;
}

// A session's row, then its subagents under the agent that spawned them, at any depth, and those
// with no known spawn under "Unattached". A faded subagent is not drawn, and its parent's line counts
// it, unless anything below it is drawn: then it keeps its line, so its children stay under it. A
// collapsed row hides its subagents, but not one that needs Daniel or failed unopened.
function tree(session: Session, token: string | null): HTMLElement[] {
  const children = childrenOf(session);
  const shut = foldedIn(session);
  const rows = (parent: string, depth: number, under: boolean) =>
    drawn(session, children, parent, depth, shut, under).map(({ agent, depth }) =>
      item(`${session.id} ${agent.id}`, depth, folds(session, children, [agent.id], shut), agentRow(session, children, agent, depth), null),
    );
  const under = shut.has("session");
  const unattached = rows("unattached", 2, under);
  const label = text("p", "Unattached", "unattached");
  const top = item(`${session.id} session`, 0, folds(session, children, ["session", "unattached"], shut), row(session, children, token), deleting(session));
  return [top, ...rows("session", 1, under), ...(unattached.length ? [label, ...unattached] : [])];
}

// The strip's labels, one per session, from its name: its issue number, else the name's first
// letters; sessions whose labels would match are numbered, so none reads "Fali" five times (danielrispler/Falinks#572).
export function stripLabels(sessions: Pick<Session, "name" | "identity" | "folder_name">[]): string[] {
  const base = sessions.map((session) =>
    session.identity.issue === null ? stripLabel(sessionName(session)) : String(session.identity.issue),
  );
  const nth = new Map<string, number>();
  return base.map((label) => {
    if (base.indexOf(label) === base.lastIndexOf(label)) return label;
    nth.set(label, (nth.get(label) ?? 0) + 1);
    return `${label}·${nth.get(label)}`;
  });
}

// The strip: a button per session that is not idle or ended, by its glyph and strip label, then one
// count for those collapsed, which opens the full list.
function strip(sessions: Session[]): HTMLElement[] {
  const toggle = () => document.body.classList.toggle("list-open");
  const open = pressable(text("button", "☰", "open"), false, toggle);
  open.title = "Open list (⌘1)";
  const shown = sessions.filter((session) => !collapsed.includes(session.state));
  const labels = stripLabels(shown);
  const rest = sessions.length - shown.length;
  return [
    open,
    ...shown.map((session, i) => {
      const button = selecting(session.id, null, `strip ${session.id}`);
      button.setAttribute("aria-pressed", String(isSelected(session.id)));
      button.title = `${sessionName(session)} · ${status(session.state, session.unseen, session.landed)[1]}`;
      button.append(glyph(session.state, session.unseen, session.landed), labels[i]);
      return button;
    }),
    ...(rest ? [Object.assign(pressable(text("button", `${rest} idle`), false, toggle), { title: "Idle" })] : []),
  ];
}

// The sessions the list shows: all but the ended ones, which are in History (#94).
export const sidebarSessions = <T extends Pick<Session, "state">>(sessions: T[]): T[] => sessions.filter((session) => session.state !== "ended");

// The main checkout of the repository a folder outside every project is in (#98), asked once each: only then its group
// may offer Add as project. Unknown until the answer comes, when the sidebar is drawn again.
const repositories = new Map<string, string | null>();
function repositoryOf(folder: string): string | null {
  const known = repositories.get(folder);
  if (known === undefined) {
    repositories.set(folder, null);
    invoke<string | null>("repository_of", { folder }).then((main) => {
      if (main) {
        repositories.set(folder, main);
        drawSidebar();
      }
    }, () => {});
  }
  return known ?? null;
}

// A project header's one action (#98): Remove on a project, Add as project on a folder outside every project whose
// repository (`main`, its main checkout, when known) isn't a project already. `key` names the action and its path, so
// the sidebar's patch replaces the button when the action changes instead of keeping the old listener.
export type HeadAction = { kind: "remove" | "add"; path: string; key: string };
export function headAction(group: { key: string; project: boolean }, main: string | null, projects: string[]): HeadAction | null {
  if (group.project) return { kind: "remove", path: group.key, key: `remove ${group.key}` };
  if (main && !projects.includes(main)) return { kind: "add", path: group.key, key: `add ${group.key}` };
  return null;
}

// A project's header (#97): its name and, even while folded, its most urgent state: needs you over running. The
// header carries the project's own actions (#98): Remove, and Add as project on a folder outside every project; a
// project's header is dragged to reorder the projects.
function projectHead(group: ProjectGroup<Session>): HTMLElement {
  const head = document.createElement("div");
  head.className = "project-head";
  head.dataset.key = `head ${group.key}`;
  head.dataset.project = group.key;
  head.draggable = group.project;
  const toggle = pressable(text("button", "", "project-toggle"), false, () => {
    if (!shut.delete(group.key)) shut.add(group.key);
    drawSidebar();
  });
  toggle.removeAttribute("aria-pressed");
  toggle.setAttribute("aria-expanded", String(projectOpen(group.key)));
  toggle.title = group.key;
  toggle.append(text("span", group.name, "name"));
  if (group.rollup) toggle.append(glyph(group.rollup));
  head.append(toggle);
  const action = headAction(group, group.project ? null : repositoryOf(group.key), knownProjects());
  if (action) {
    const remove = action.kind === "remove";
    const button = pressable(text("button", remove ? "×" : "Add as project", "project-action"), false, () => {
      if (remove) invoke<string[]>("remove_project", { project: action.path }).then(setProjects, refuse);
      else void addProject(action.path);
    });
    button.removeAttribute("aria-pressed");
    button.dataset.key = action.key;
    button.title = remove ? "Remove project: its worktrees and branches stay" : `Add ${action.path} as a project`;
    head.append(button);
  }
  return head;
}

// Dragging a project's header onto another moves it before that one, or after it from the lower half of the target;
// the order is written to `projects.json` and kept.
function dragProjects(groups: HTMLElement) {
  let dragged: string | null = null;
  const target = (event: DragEvent) => (event.target as HTMLElement).closest<HTMLElement>(".project-head[draggable=true]");
  groups.addEventListener("dragstart", (event) => {
    const head = target(event);
    dragged = head?.dataset.project ?? null;
    if (head && event.dataTransfer) event.dataTransfer.effectAllowed = "move";
  });
  groups.addEventListener("dragover", (event) => {
    if (dragged && target(event)) event.preventDefault();
  });
  groups.addEventListener("drop", (event) => {
    const head = target(event);
    const over = head?.dataset.project;
    if (!head || !over || !dragged || over === dragged) return;
    event.preventDefault();
    const box = head.getBoundingClientRect();
    const after = event.clientY > box.top + box.height / 2;
    const rest = knownProjects().filter((path) => path !== dragged);
    const at = rest.indexOf(over);
    if (at < 0) return;
    rest.splice(at + (after ? 1 : 0), 0, dragged);
    dragged = null;
    invoke<string[]>("reorder_projects", { order: rest }).then(setProjects, refuse);
  });
  groups.addEventListener("dragend", () => (dragged = null));
}
onProjects(() => drawSidebar());

// A project's group: its header, then, unless folded, its live sessions by state and its workspace rows. A group for a
// folder outside every project is dimmed and has no workspaces.
function projectNode(group: ProjectGroup<Session>): HTMLElement {
  const node = document.createElement("section");
  node.className = group.project ? "project" : "project outside";
  node.dataset.key = group.key;
  node.append(projectHead(group));
  if (!projectOpen(group.key)) return node;
  const body = document.createElement("div");
  body.className = "project-body";
  body.dataset.key = "body";
  for (const { state, sessions } of group.states) {
    const rows = sessions.flatMap((session) => tree(session, group.project ? tokenOf(group.key, session.cwd) : null));
    if (!collapsed.includes(state)) {
      const states = document.createElement("div");
      states.dataset.key = state;
      states.append(...rows);
      body.append(states);
      continue;
    }
    const folded = document.createElement("details");
    folded.dataset.key = state;
    const id = `${group.key} ${state}`;
    folded.open = opened.has(id);
    folded.addEventListener("toggle", () => (folded.open ? opened.add(id) : opened.delete(id)));
    folded.append(text("summary", `${stateLabels[state]} · ${sessions.length}`), ...rows);
    body.append(folded);
  }
  if (group.project) {
    const workspaces = document.createElement("div");
    workspaces.className = "project-workspaces";
    workspaces.dataset.key = "workspaces";
    workspaces.append(...workspaceSection(group.key, studio.listed.filter((session) => session.state !== "ended")));
    body.append(workspaces);
  }
  node.append(body);
  return node;
}

// The project groups (#97): one per project, in order, then one dimmed group for each folder outside every project.
// Patched, not rebuilt, so focus and text selection survive the update (danielrispler/Falinks#570).
let listening = false;
export function drawSidebar() {
  if (!listening) dragProjects(element("groups"));
  listening = true;
  const paths = knownProjects();
  const error = projectsError();
  const failure: HTMLElement[] = [];
  if (error) {
    const message = text("pre", `The projects could not be read: ${error}`, "picker-error");
    message.dataset.key = "projects-error";
    failure.push(message);
  }
  patch(element("groups"), [...projectGroups(paths, foundWorkspaces(), sidebarSessions(studio.listed)).map(projectNode), ...failure]);
}

// The sessions are grouped by project, then by state: needs you, running, your turn, idle; the idle ones
// fold. Ended ones are in History. The rows carry their state by shape, so the groups have no heading
// (danielrispler/Falinks#572).
export function showSessions(sessions: Session[]) {
  studio.listed = sessions;
  redrawHome();
  redrawHistory();
  const live = sidebarSessions(sessions);
  drawSidebar();
  // The projects are read once; until then the groups draw with none.
  if (!knownProjects().length) void projects().then(drawSidebar, () => {});
  patch(element("history-link"), historyLink(sessions, showHistory));
  patch(element("strip"), strip(live));
  element("no-sessions").hidden = sessions.length > 0;
  const chip = element("needs-you");
  chip.textContent = chipText(sessions);
  chip.hidden = chip.textContent === "";
}
