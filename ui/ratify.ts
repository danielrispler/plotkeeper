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

// Records a decision through `record`; for an overturn, `copy` puts its prompt on the clipboard too and its outcome is returned. The copy
// starts first, inside the click: a browser may refuse the clipboard once the click has awaited a round trip (port fix F5). `copy` must
// not reject (`copyOverturn` does not).
export async function decideWith<T>(overturn: boolean, copy: () => Promise<T>, record: () => Promise<void>): Promise<T | null> {
  const copied = overturn ? copy() : null;
  await record();
  return copied ? await copied : null;
}

// An overturn's note, kept for the item it was made for: the line, and for a failed copy the prompt to copy by hand.
export type Overturned = { project: string; pr: number; kind: string; id: string; line: string; manual: string | null };

// The note to show where `holds` says which items are drawn (a part's sheet, a brief's ratify step) in `project`: only the one made
// for an item there.
export function noteFor(last: Overturned | null, project: string, holds: (item: Pick<RatifyRow, "pr" | "kind" | "id">) => boolean): Overturned | null {
  return last && last.project === project && holds(last) ? last : null;
}

// The brief's last step when there is anything to ratify, or Guard with a law to offer (#401, a bug fix's brief, before and after it
// merges): it holds the items and the guard's row, drawn by `map-brief.ts`. `on` is the parts the brief lit, so the map stays as the brief
// left it.
export const ratifyStep = (rows: RatifyRow[], on: string[] = [], guard = false): BriefStep | null => {
  const step = { k: "Ratify", on, hops: [], law: null, covered_by: null, list: [], risks: [], fig: null, check: null, cites: [], flags: [] };
  if (rows.length) return { ...step, h: "Accept or overturn", b: "What changed the map, and what the agent decided. Nothing waits on this." };
  return guard ? { ...step, h: "Guard with a law", b: "This PR fixed a bug and left no law behind it." } : null;
};

// The rows still to ratify.
export const openRows = (rows: RatifyRow[]) => rows.filter((row) => row.status === "not-yet");
