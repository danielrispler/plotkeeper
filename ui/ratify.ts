// Ratifying (spec danielrispler/plotkeeper#334, ticket #343), the pure part: what the part sheet and the brief's ratify step say of an item,
// and what an overturn copies. `map-sheet.ts` and `map-brief.ts` draw it, the server (`server/src/ratify.rs`) keeps the records.

import type { BriefStep, PendingItem, RatifyRow } from "./types.js";

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

// The prompt an overturn puts on the clipboard: it stands alone for any agent, naming the PR, its branch while it is open, the item's
// id and its wording.
export function overturnPrompt(row: Pick<RatifyRow, "pr" | "kind" | "id" | "text" | "change">, branch: string | null): string {
  const where = branch ? `PR #${row.pr} (branch ${branch})` : `PR #${row.pr}`;
  const what =
    row.kind === "decision"
      ? `the decision ${row.id}: “${row.text}”`
      : `the ${kindWords[row.kind] ?? row.kind} ${row.id}${row.change ? ` (${row.change})` : ""}: “${row.text}”`;
  return `In ${where}, I overturn ${what} Undo it and tell me what you changed.`;
}

// Puts `prompt` on the clipboard through `writeText` (`navigator.clipboard.writeText`): "copied", or "manual" when there is no clipboard or
// the write fails.
export async function copyOverturn(prompt: string, writeText: ((text: string) => Promise<void> | void) | undefined): Promise<"copied" | "manual"> {
  if (!writeText) return "manual";
  try {
    await writeText(prompt);
    return "copied";
  } catch {
    return "manual";
  }
}

// What the overturn shows once the copy was tried: the line, and for a failed copy the prompt to copy by hand.
export function overturnReport(outcome: "copied" | "manual", prompt: string): { line: string; manual: string | null } {
  return outcome === "copied"
    ? { line: "Prompt copied, paste it into your agent", manual: null }
    : { line: "Copy this prompt and paste it into your agent", manual: prompt };
}

// The brief's last step when there is anything to ratify: it holds the items, drawn by `map-brief.ts`.
// `on` is the parts the brief lit, so the map stays as the brief left it.
export const ratifyStep = (rows: RatifyRow[], on: string[] = []): BriefStep | null =>
  rows.length === 0
    ? null
    : { k: "Ratify", h: "Accept or overturn", b: "What changed the map, and what the agent decided. Nothing waits on this.", on, hops: [], law: null, covered_by: null, list: [], risks: [], fig: null, check: null, cites: [], flags: [] };

// The rows still to ratify.
export const openRows = (rows: RatifyRow[]) => rows.filter((row) => row.status === "not-yet");
