// Ratifying (spec danielrispler/plotkeeper#334, ticket #343), the pure part: what the part sheet and the brief's ratify step say of an item,
// and what an overturn copies. `map-sheet.ts` and `map-brief.ts` draw it, the server (`server/src/ratify.rs`) keeps the records.

import type { Drawer } from "./map-model.js";
import type { BriefStep, ImportSite, LinkSites, PendingItem, RatifyDetail, RatifyRow, SiteSnippet } from "./types.js";

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

// One line of a detail: what changed, its text before and after, null on a side that has none. `code` marks paths and a link's kind,
// which are names in code and are never translated.
export type DetailLine = { label: string; before: string | null; after: string | null; code?: true };

const list = (patterns: string[]) => (patterns.length ? patterns.join(", ") : null);

// The lines of a part's or a link's detail that changed, old beside new; an added item's every line, with nothing before. A part's own
// paths and each held item's show the patterns removed beside those added.
export function detailLines(detail: RatifyDetail | null | undefined): DetailLine[] {
  if (!detail) return [];
  const lines: DetailLine[] = [];
  const words = (label: string, text: { before: string | null; after: string | null }, code = false) => {
    // An empty text and none are the same: an import link added without a reason has no Reason to show.
    if ((text.before || null) !== (text.after || null)) lines.push(code ? { label, ...text, code } : { label, ...text });
  };
  const paths = (label: string, added: string[], removed: string[]) => {
    if (added.length || removed.length) lines.push({ label, before: list(removed), after: list(added), code: true });
  };
  if (detail.of === "link") {
    words("Kind", detail.kind, true);
    words("Reason", detail.reason);
    return lines;
  }
  words("Name", detail.name);
  words("Purpose", detail.purpose);
  paths("Paths", detail.paths_added, detail.paths_removed);
  for (const held of detail.holds) {
    words(`Holds ${held.name}`, held.text);
    paths(`${held.name}: paths`, held.paths_added, held.paths_removed);
  }
  return lines;
}

// What an overturn quotes of a part's or a link's new side: a part's purpose and the names of what it holds, a link's kind and reason.
function newText(detail: RatifyDetail | null | undefined): string {
  if (!detail) return "";
  if (detail.of === "link") {
    if (detail.kind.after === null) return "";
    return `, kind ${detail.kind.after || "none"}${detail.reason.after ? `, reason “${detail.reason.after}”` : ""}`;
  }
  if (detail.purpose.after === null) return "";
  const held = detail.holds.filter((h) => h.text.after !== null).map((h) => h.name);
  return `${detail.purpose.after ? `, purpose: “${detail.purpose.after}”` : ""}${held.length ? `, and it holds ${held.join(", ")}` : ""}`;
}

// Link evidence (#499): an import site as a reader names it, `file:line`, or the file alone when its line is not known.
export const siteLabel = (site: ImportSite) => (site.line ? `${site.file}:${site.line}` : site.file);

// An added link's second line in the ratify step: its first site, then "(+N imports across M files)" for the others; null with no evidence.
export function evidenceLine(evidence: LinkSites | undefined): { site: string; more: string | null } | null {
  const [first, ...rest] = evidence?.sites ?? [];
  if (!first) return null;
  return { site: siteLabel(first), more: rest.length ? `(+${counted(rest)})` : null };
}

const plural = (n: number, one: string) => `${n} ${one}${n === 1 ? "" : "s"}`;
// "3 imports across 2 files".
const counted = (sites: ImportSite[]) => `${plural(sites.length, "import")} across ${plural(new Set(sites.map((site) => site.file)).size, "file")}`;
const short = (head: string) => head.slice(0, 7);

// The drawer of one site's snippet, read from the commit `head`: numbered from its first line, the site's own line marked.
export function siteDrawer(snippet: SiteSnippet, head: string): Drawer {
  const block = { label: snippet.file, code: snippet.lines.join("\n"), jumps: [], first: snippet.first };
  return {
    title: siteLabel(snippet),
    note: `As the PR's head ${short(head)} holds it`,
    blocks: [snippet.line ? { ...block, mark: snippet.line } : block],
    jumps: [],
  };
}

// The drawer of every site behind an added link, each opening its snippet through `open`.
export function sitesDrawer(row: Pick<RatifyRow, "text" | "evidence">, open: (site: ImportSite) => void): Drawer {
  const sites = row.evidence?.sites ?? [];
  return {
    title: `Imports behind ${row.text}`,
    note: `${counted(sites)}, as the PR's head ${short(row.evidence?.head ?? "")} holds them`,
    blocks: [],
    jumps: [],
    picks: sites.map((site) => ({ label: siteLabel(site), title: "Show this import's code", open: () => open(site) })),
  };
}

// A full stop after `words`, unless they already end with one, inside a closing quote or not.
const stopped = (words: string) => (/[.!?]”?$/.test(words) ? words : `${words}.`);

const kindWords: Record<string, string> = { part: "part", actor: "outside actor", link: "link", law: "law", flow: "flow", decision: "decision" };

// The prompt an overturn puts on the clipboard: it stands alone for any agent, naming the PR, its branch while it is open, the item's
// id and its wording, and for an added link the import sites behind it (#499).
export function overturnPrompt(
  row: Pick<RatifyRow, "pr" | "kind" | "id" | "text" | "change"> & { detail?: RatifyDetail | null; evidence?: LinkSites },
  branch: string | null,
): string {
  const where = branch ? `PR #${row.pr} (branch ${branch})` : `PR #${row.pr}`;
  const what =
    row.kind === "decision"
      ? `the decision ${row.id}: “${row.text}”`
      : `the ${kindWords[row.kind] ?? row.kind} ${row.id}${row.change ? ` (${row.change})` : ""}: “${row.text}”${newText(row.detail)}`;
  const sites = row.evidence?.sites ?? [];
  const imports = sites.length ? ` Its imports at the PR's head: ${sites.map(siteLabel).join(", ")}.` : "";
  return `In ${where}, I overturn ${stopped(what)}${imports} Undo it and tell me what you changed.`;
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

// A link the part sheet marks, as an item to ratify: its ends by id as the brief's row says them, and its kind and reason as the new side
// an overturn quotes. The sheet has no old side, so `before` is null.
export function sheetLinkItem(link: { id: string; from: string; to: string; kind?: string; reason?: string }, pr: number) {
  const detail: RatifyDetail = { of: "link", kind: { before: null, after: link.kind ?? "" }, reason: { before: null, after: link.reason ?? "" } };
  return { pr, kind: "link", id: link.id, text: `${link.from} to ${link.to}`, change: "", detail };
}
