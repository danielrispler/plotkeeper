// Ratifying (spec danielrispler/plotkeeper#334, ticket #343), the pure part: what the part sheet and the brief's ratify step say of an item,
// and where an overturn goes. `map-sheet.ts` and `map-brief.ts` draw it, the shell (`src-tauri/src/ratify.rs`) keeps the records.

import type { BriefStep, PendingItem, RatifyRow, Session } from "./types.js";

// The state of an item as the brief says it: nothing to ratify when it landed before the first run.
export const statusWords: Record<RatifyRow["status"], string> = {
  ratified: "Ratified",
  accepted: "Accepted",
  overturned: "Overturned",
  "not-yet": "Not yet ratified",
};

// What the part sheet marks for `part`: the laws and the links at it that are not yet ratified, with the PR that changed each.
export function sheetPending(pending: PendingItem[], part: string, links: { id: string; from: string; to: string }[]) {
  const laws = new Map<string, number>();
  const found = new Map<string, number>();
  for (const item of pending) {
    if (item.kind === "law" && !laws.has(item.id)) laws.set(item.id, item.pr);
    if (item.kind === "link" && !found.has(item.id)) found.set(item.id, item.pr);
  }
  return { laws, links: links.filter((link) => (link.from === part || link.to === part) && found.has(link.id)).map((link) => ({ ...link, pr: found.get(link.id)! })) };
}

const kindWords: Record<string, string> = { part: "part", actor: "outside actor", link: "link", law: "law", flow: "flow", decision: "decision" };

// The prompt an overturn hands to the agent: it quotes the item.
export function overturnPrompt(row: Pick<RatifyRow, "pr" | "kind" | "id" | "text" | "change">): string {
  const what = row.kind === "decision" ? `the decision “${row.text}”` : `the ${kindWords[row.kind] ?? row.kind} ${row.id}${row.change ? ` (${row.change})` : ""}: “${row.text}”`;
  return `I overturn ${what} from PR #${row.pr}. Undo it and tell me what you changed.`;
}

// Where an overturn goes: the PR's own session while it is live and has a terminal, else New session in the project.
export function overturnRoute(sessions: Pick<Session, "id" | "state" | "terminal" | "identity">[], pr: number): { kind: "live"; id: string; terminal: string } | { kind: "new" } {
  const live = sessions.find((s) => s.identity.pr === pr && s.state !== "ended" && s.terminal !== null);
  return live ? { kind: "live", id: live.id, terminal: live.terminal! } : { kind: "new" };
}

// The brief's last step when there is anything to ratify: it holds the items, drawn by `map-brief.ts`.
// `on` is the parts the brief lit, so the map stays as the brief left it.
export const ratifyStep = (rows: RatifyRow[], on: string[] = []): BriefStep | null =>
  rows.length === 0
    ? null
    : { k: "Ratify", h: "Accept or overturn", b: "What changed the map, and what the agent decided. Nothing waits on this.", on, hops: [], law: null, covered_by: null, list: [], risks: [], fig: null, check: null, cites: [], flags: [] };

// The rows still to ratify.
export const openRows = (rows: RatifyRow[]) => rows.filter((row) => row.status === "not-yet");
