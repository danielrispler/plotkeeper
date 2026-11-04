// The session list, on the window's right: the sessions grouped by state, each session's subagents
// nested under it. When the window is narrow it collapses to a strip of state dots and issue numbers,
// which opens the full list on click or ⌘1.

import { age, element, prose, pressable, stepText, text } from "./dom.js";
import { select } from "./selection.js";
import { studio } from "./state.js";
import { stateLabels, type Session, type SessionState, type Subagent } from "./types.js";

// The collapsed groups Daniel opened, which stay open when the list is drawn again.
const collapsed: SessionState[] = ["idle", "ended"];
const opened = new Set<SessionState>();

// What the strip calls a session: the issue number of an `issue-<n>-…` folder, else its folder's
// first letters.
export function stripLabel(folder: string): string {
  return /^issue-(\d+)/.exec(folder)?.[1] ?? folder.slice(0, 4);
}

// A subagent's state label and its state: "Needs you" while it asks, else its status.
export function agentState(session: Session, agent: Subagent): [string, string] {
  return session.asker?.agent === agent.id ? [stateLabels.needs_you, "needs_you"] : [agent.status, agent.status];
}

// The dot of a state: only "needs you" takes the colour.
function dot(state: string): HTMLElement {
  const node = text("span", "", "dot");
  node.dataset.state = state;
  return node;
}

// A session's subagents by the id of the agent that spawned them: "session", "unattached" or a
// subagent's id. Built once per draw, so the tree and the "done" counts do not scan every subagent.
type Children = Map<string, Subagent[]>;
function childrenOf(session: Session): Children {
  const children: Children = new Map();
  for (const agent of session.agents) {
    const parent = typeof agent.parent === "string" ? agent.parent : agent.parent.agent;
    if (!children.has(parent)) children.set(parent, []);
    children.get(parent)!.push(agent);
  }
  return children;
}

// Whether a subagent's line is not drawn: it faded, and nothing below it is drawn (#434, decision 43).
function hidden(children: Children, agent: Subagent): boolean {
  return agent.faded && (children.get(agent.id) ?? []).every((child) => hidden(children, child));
}

// The count of a parent's helpers that faded into its line.
function done(all: Children, children: Subagent[] = []): HTMLElement[] {
  const faded = children.filter((agent) => hidden(all, agent)).length;
  return faded ? [text("span", `${faded} done`, "id")] : [];
}

// A row's two lines: its dot, name and state, then what it is doing.
function rowLines(button: HTMLElement, state: string, label: string, name: HTMLElement[], below: HTMLElement[]) {
  const first = text("span", "", "l1");
  const stateNode = text("span", label, "row-state");
  stateNode.dataset.state = state;
  first.append(dot(state), ...name, stateNode);
  const second = text("span", "", "l2");
  second.append(...below);
  button.append(first, second);
  return stateNode;
}

// One row: the folder, the session's short id, its state and what it is doing.
function row(session: Session, children: Children): HTMLElement {
  const selection = studio.selection;
  const pressed = session.id === selection?.session.id && selection.agent === null;
  const button = pressable(document.createElement("button"), pressed, () => select(session.id));
  button.className = "row";
  rowLines(
    button,
    session.state,
    stateLabels[session.state],
    [text("span", session.name ?? session.folder_name, "name"), text("span", session.id.slice(0, 8), "id")],
    [...done(children, children.get("session")), rowLine(session.line ?? "")],
  );
  return button;
}

// A row's line, such as a card summary stored in Hebrew, laid out by its own direction, its code spans
// cleaned as a card's are (#434, decision 40).
function rowLine(line: string): HTMLElement {
  const node = prose("span", line);
  node.className = "row-step";
  node.dir = "auto";
  return node;
}

// A subagent's row, indented under the agent that spawned it. A worker shows its task, status and
// last step; a helper its task, type, status, duration and result. A failed child stands out until
// Daniel opens it.
function agentRow(session: Session, children: Children, agent: Subagent, depth: number): HTMLElement {
  const button = pressable(document.createElement("button"), agent.id === studio.selection?.agent, () =>
    select(session.id, agent.id),
  );
  button.className = `row ${agent.kind}`;
  button.style.paddingInlineStart = `${8 + 14 * depth}px`;
  const [label, stateName] = agentState(session, agent);
  const details =
    agent.kind === "worker"
      ? [stepText(agent.last_step)]
      : [agent.agent_type, age(agent.started_at, agent.ended_at ?? Date.now()), agent.result];
  const state = rowLines(
    button,
    stateName,
    label,
    [text("span", agent.name, "name")],
    [...done(children, children.get(agent.id)), rowLine(details.filter(Boolean).join(" · "))],
  );
  state.classList.toggle("failed", agent.failed && !agent.opened);
  return button;
}

// A session's row, then its subagents under the agent that spawned them, at any depth, and those
// with no known spawn under "Unattached". A faded helper is not drawn, and its parent's line counts
// it, unless anything below it is drawn: then it keeps its line, so its children stay under it.
function tree(session: Session): HTMLElement[] {
  const children = childrenOf(session);
  const nested = (parent: string, depth: number): HTMLElement[] =>
    (children.get(parent) ?? []).flatMap((agent) => {
      const below = nested(agent.id, depth + 1);
      return [...(hidden(children, agent) ? [] : [agentRow(session, children, agent, depth)]), ...below];
    });
  const unattached = nested("unattached", 2);
  const label = text("p", "Unattached", "unattached");
  return [row(session, children), ...nested("session", 1), ...(unattached.length ? [label, ...unattached] : [])];
}

// The strip: a button per session that is not idle or ended, by its dot and strip label, then one
// count for those collapsed, which opens the full list.
function strip(sessions: Session[]): HTMLElement[] {
  const toggle = () => document.body.classList.toggle("list-open");
  const open = pressable(text("button", "☰", "open"), false, toggle);
  open.title = "Open list (⌘1)";
  const shown = sessions.filter((session) => !collapsed.includes(session.state));
  const rest = sessions.length - shown.length;
  return [
    open,
    ...shown.map((session) => {
      const pressed = session.id === studio.selection?.session.id;
      const button = pressable(document.createElement("button"), pressed, () => select(session.id));
      button.title = `${session.folder_name} · ${stateLabels[session.state]}`;
      button.append(dot(session.state), stripLabel(session.folder_name));
      return button;
    }),
    ...(rest ? [Object.assign(pressable(text("button", String(rest)), false, toggle), { title: "Idle and ended" })] : []),
  ];
}

// The core sends the sessions grouped by state, so a group starts where the state changes. Idle and
// ended sessions collapse at the bottom.
export function showSessions(sessions: Session[]) {
  studio.listed = sessions;
  const groups: Session[][] = [];
  for (const session of sessions) {
    const last = groups[groups.length - 1];
    if (last?.[0].state === session.state) last.push(session);
    else groups.push([session]);
  }
  element("groups").replaceChildren(
    ...groups.map((rows) => {
      const state = rows[0].state;
      const title = `${stateLabels[state]} · ${rows.length}`;
      if (!collapsed.includes(state)) {
        const group = document.createElement("section");
        group.append(text("h1", title), ...rows.flatMap(tree));
        return group;
      }
      const group = document.createElement("details");
      group.open = opened.has(state);
      group.addEventListener("toggle", () => (group.open ? opened.add(state) : opened.delete(state)));
      group.append(text("summary", title), ...rows.flatMap(tree));
      return group;
    }),
  );
  element("strip").replaceChildren(...strip(sessions));
  element("no-sessions").hidden = sessions.length > 0;
}
