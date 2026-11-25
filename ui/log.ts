// A subagent's log in the pane's place (danielrispler/plotkeeper#54): read-only. The core reads the log file from an
// offset and the pane keeps that offset, reading on when the shell re-sends the selection. Tool calls show as rows,
// folded once they have a result. The header's name and state come from the core's tracking, never from the log.

import { fullTitle, invoke, pressable, scrollingPre, text } from "./dom.js";
import type { LogCall, LogEntry, LogResult, LogRow, Subagent, SubagentLog } from "./types.js";

// Pairs each result with its call by id, in order of the calls. A call without a result yet is running
// (`result: null`); a result without a call is dropped.
export function logRows(entries: LogEntry[]): LogRow[] {
  const rows: LogRow[] = [];
  const calls = new Map<string, LogCall>();
  for (const entry of entries) {
    if (entry.kind === "call") {
      const row: LogCall = { kind: "call", id: entry.id, tool: entry.tool, detail: entry.detail, result: null };
      calls.set(entry.id, row);
      rows.push(row);
    } else if (entry.kind === "result") {
      const call = calls.get(entry.id);
      if (call) call.result = { text: entry.text, failed: entry.failed, cut: entry.cut } satisfies LogResult;
    } else rows.push(entry);
  }
  return rows;
}

// What the pane says in place of a log the core could not find: it may not exist yet while the subagent runs.
export const logNote = (log: SubagentLog | null, status: string): string | null =>
  log && log.entries.length > 0 ? null : status === "running" ? "Starting…" : "The log isn't on disk.";

// The header's state: `running` or `done` (`completed`), else the core's own word, such as `killed`.
export const stateLabel = (status: string) => (status === "completed" ? "done" : status);

// The log being shown: whose, and what has been read of it (null while it has no file), for reading on.
export type ShownLog = { session: string; agent: string; log: SubagentLog | null };
const shownLog: ShownLog = { session: "", agent: "", log: null };

// The shown log with a read from its offset added: only new entries, so a call's result pairs with it when it
// arrives. A read that found no file changes nothing.
export function readOn(shown: ShownLog, read: SubagentLog | null): ShownLog {
  if (!read) return shown;
  return { ...shown, log: { entries: [...(shown.log?.entries ?? []), ...read.entries], offset: read.offset } };
}

// Whether the view was at the bottom, so new entries take it along; a pixel or two short still counts.
export const followsBottom = (v: { scrollTop: number; clientHeight: number; scrollHeight: number }) =>
  v.scrollHeight - v.scrollTop - v.clientHeight <= 4;

// Where the view opens: a running subagent's log at the bottom, so following works; a finished one at the top.
export const opensAtBottom = (status: string) => status === "running";

// The scroll position after a redraw: the bottom if the view was there, else where Daniel had it.
export const scrollAfter = (before: { scrollTop: number; clientHeight: number; scrollHeight: number }, height: number) =>
  followsBottom(before) ? height : before.scrollTop;

// The calls whose rows are open, by id: a redraw (reading on) keeps each row as Daniel left it.
const expanded = new Set<string>();

// Draws the log view of a subagent the session does not have: the no-log state, never the previous pane.
export function showMissing(place: HTMLElement, toParent: () => void) {
  Object.assign(shownLog, { session: "", agent: "", log: null });
  drawLog(place, null, null, toParent);
}

// Reads subagent `agent`'s log from the start and draws it in `place`. `toParent` shows the parent's terminal.
export async function showLog(place: HTMLElement, session: string, agent: Subagent, toParent: () => void) {
  expanded.clear();
  const mine = { session, agent: agent.id, log: null as SubagentLog | null };
  Object.assign(shownLog, mine);
  const log = await invoke("subagent_log", { session, agentId: agent.id, offset: 0 });
  if (shownLog.session !== session || shownLog.agent !== agent.id || shownLog.log) return; // a newer selection took over
  shownLog.log = log;
  drawLog(place, agent, shownLog.log, toParent);
  place.scrollTop = opensAtBottom(agent.status) ? place.scrollHeight : 0;
}

// Reads on from the offset when the shell re-sends the selection of the subagent shown, and draws it, following
// the bottom only if the view was there.
export async function followLog(place: HTMLElement, session: string, agent: Subagent, toParent: () => void) {
  if (shownLog.session !== session || shownLog.agent !== agent.id) return;
  const from = shownLog.log?.offset ?? 0;
  const read = await invoke("subagent_log", { session, agentId: agent.id, offset: from });
  // Another read or selection since: the offset moved, or it is another log.
  if (shownLog.session !== session || shownLog.agent !== agent.id || (shownLog.log?.offset ?? 0) !== from) return;
  if (!read || read.entries.length === 0) return; // nothing new: no redraw
  const before = { scrollTop: place.scrollTop, clientHeight: place.clientHeight, scrollHeight: place.scrollHeight };
  Object.assign(shownLog, readOn(shownLog, read));
  drawLog(place, agent, shownLog.log, toParent);
  place.scrollTop = scrollAfter(before, place.scrollHeight);
}

// A tool row's title: `Tool · detail`.
export const callTitle = (call: { tool: string; detail: string | null }) => (call.detail ? `${call.tool} · ${call.detail}` : call.tool);

// `agent` is null for a subagent the session does not have.
function drawLog(place: HTMLElement, agent: Subagent | null, log: SubagentLog | null, toParent: () => void) {
  place.className = "pane subagent-log";
  const head = document.createElement("header");
  head.append(
    fullTitle(text("span", agent?.name ?? "Subagent", "log-title"), agent?.name ?? "Subagent"),
    ...(agent?.agent_type ? [text("span", agent.agent_type, "log-type")] : []),
    ...(agent ? [text("span", stateLabel(agent.status), `log-state ${agent.status === "running" ? "running" : "done"}`)] : []),
    fullTitle(pressable(text("button", "Parent's terminal"), null, toParent), "Back to the terminal of the session that started this subagent"),
  );
  const note = logNote(log, agent?.status ?? "");
  const body = note ? [text("p", note, "empty")] : logRows(log!.entries).map(drawRow);
  place.replaceChildren(head, ...body);
}

function drawRow(row: LogRow): HTMLElement {
  if (row.kind === "prompt") return text("pre", row.text, "log-prompt");
  if (row.kind === "reply") return text("p", row.text, "log-reply");
  if (!row.result) return text("div", callTitle(row), "log-call running");
  const { failed, text: out, cut } = row.result;
  const fold = document.createElement("details");
  fold.open = expanded.has(row.id);
  fold.addEventListener("toggle", () => (fold.open ? expanded.add(row.id) : expanded.delete(row.id)));
  fold.className = failed ? "log-call failed" : "log-call";
  const summary = fullTitle(text("summary", callTitle(row)), `Show or hide this call's result: ${callTitle(row)}`);
  if (failed) summary.append(text("span", "error", "log-error"));
  const result = scrollingPre(out + (cut ? "\n… cut" : ""), "log-result", `Result of ${callTitle(row)}`);
  fold.append(summary, result);
  return fold;
}
