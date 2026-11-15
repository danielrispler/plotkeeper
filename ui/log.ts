// A subagent's log in the pane's place (danielrispler/plotkeeper#54): read-only, plain for now. The core reads the log
// file from an offset and the pane keeps that offset, so reading on (#76) and the structured look (#77) extend
// this module. The header's name and state come from the core's tracking, never from the log.

import { element, invoke, pressable, text } from "./dom.js";
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
  log ? null : status === "running" ? "Starting…" : "The log isn't on disk.";

// The header's state: `running` or `done` (`completed`), else the core's own word, such as `killed`.
export const stateLabel = (status: string) => (status === "completed" ? "done" : status);

// The log being shown: whose, what was read, and the offset it was read to, for reading on.
export const shownLog = { session: "", agent: "", entries: [] as LogEntry[], offset: 0 };

// Reads subagent `agent`'s log from the start and draws it in `place`. `toParent` shows the parent's terminal.
export async function showLog(place: HTMLElement, session: string, agent: Subagent, toParent: () => void) {
  const log = await invoke<SubagentLog | null>("subagent_log", { session, agentId: agent.id, offset: 0 });
  Object.assign(shownLog, { session, agent: agent.id, entries: log?.entries ?? [], offset: log?.offset ?? 0 });
  drawLog(place, agent, log, toParent);
}

function drawLog(place: HTMLElement, agent: Subagent, log: SubagentLog | null, toParent: () => void) {
  place.className = "pane subagent-log";
  const head = document.createElement("header");
  const title = [agent.name, agent.agent_type, stateLabel(agent.status)].filter(Boolean).join(" · ");
  head.append(text("span", title, "log-title"), pressable(text("button", "Parent's terminal"), false, toParent));
  const note = logNote(log, agent.status);
  const body = note ? [text("p", note, "empty")] : logRows(log!.entries).map(drawRow);
  place.replaceChildren(head, ...body);
}

function drawRow(row: LogRow): HTMLElement {
  if (row.kind === "prompt") return text("pre", row.text, "log-prompt");
  if (row.kind === "reply") return text("p", row.text, "log-reply");
  const line = text("div", `${row.tool}${row.detail ? ` ${row.detail}` : ""}`, "log-call");
  if (!row.result) {
    line.classList.add("running");
    return line;
  }
  const out = text("pre", row.result.text + (row.result.cut ? "\n… cut" : ""), row.result.failed ? "log-result failed" : "log-result");
  const wrap = document.createElement("div");
  wrap.append(line, out);
  return wrap;
}

