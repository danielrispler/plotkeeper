// Ratifying (spec danielrispler/plotkeeper#334, ticket #343), the pure part: what the part sheet and the brief's ratify step say of an item,
// and what an overturn copies. `map-sheet.ts` and `map-brief.ts` draw it, the server (`server/src/ratify.rs`) keeps the records.

import type { Drawer } from "./map-model.js";
import type { BriefStep, ImportSite, LinkSites, MapReview, MapReviewFlag, PendingItem, RatifyDetail, RatifyRow, SiteSnippet } from "./types.js";

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
  return { laws, links: links.flatMap((link) => {
    const pr = found.get(link.id);
    return (link.from === part || link.to === part) && pr !== undefined ? [{ ...link, pr }] : [];
  }) };
}

// One line of a detail: what changed, its text before and after, null on a side that has none. `code` marks paths and a link's kind,
// which are names in code and are never translated.
export type DetailLine = { label: string; before: string | null; after: string | null; code?: true };

const list = (patterns: string[]) => (patterns.length ? patterns.join(", ") : null);

// The lines of a part's or a link's detail that changed, old beside new; an added item's every line, with nothing before. A link whose ends
// swapped says its Direction. A part's own
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
    // Only a link whose ends swapped has a direction to tell (#643).
    if (detail.direction) words("Direction", detail.direction);
    words("Kind", detail.kind, true);
    words("Reason", detail.reason);
    return lines;
  }
  words("Name", detail.name);
  // After the name, as the plate reads (#643): an Edited part whose caption alone changed still says what changed.
  words("Caption", detail.caption);
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
const siteLabel = (site: ImportSite) => (site.line ? `${site.file}:${site.line}` : site.file);

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
// id and its wording, for an added link the import sites behind it (#499), and for a row the Map text review flagged its why (`flags`,
// `reviewFlags`, #645). A Still true? row (#698) names its review item (`item`, the flag's id) and asks for a correction.
export function overturnPrompt(
  row: Pick<RatifyRow, "pr" | "kind" | "id" | "text" | "change"> & { detail?: RatifyDetail | null; evidence?: LinkSites; flags?: string[]; item?: string },
  branch: string | null,
): string {
  const where = branch ? `PR #${row.pr} (branch ${branch})` : `PR #${row.pr}`;
  if (row.kind === "still-true") {
    const item = row.item ?? row.id;
    const why = row.flags?.length ? ` The map text review flagged it: ${row.flags.join(" ")}` : "";
    return `In ${where}, I overturn the map text ${item}: “${row.text}”, which the PR left unchanged.${why} Correct the map text to what the code does, or the code when the text states the intent, and tell me what you changed.`;
  }
  const what =
    row.kind === "decision"
      ? `the decision ${row.id}: “${row.text}”`
      : `the ${kindWords[row.kind] ?? row.kind} ${row.id}${row.change ? ` (${row.change})` : ""}: “${row.text}”${newText(row.detail)}`;
  const sites = row.evidence?.sites ?? [];
  const imports = sites.length ? ` Its imports at the PR's head: ${sites.map(siteLabel).join(", ")}.` : "";
  const flagged = row.flags?.length ? ` The map text review flagged it: ${row.flags.join(" ")}` : "";
  return `In ${where}, I overturn ${stopped(what)}${imports}${flagged} Undo it and tell me what you changed.`;
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
  return copied === null ? null : await copied;
}

// An overturn's note, kept for the item it was made for: the line, and for a failed copy the prompt to copy by hand.
export type Overturned = { project: string; pr: number; kind: string; id: string; line: string; manual: string | null };

// The note to show where `holds` says which items are drawn (a part's sheet, a brief's ratify step) in `project`: only the one made
// for an item there.
export function noteFor(last: Overturned | null, project: string, holds: (item: Pick<RatifyRow, "pr" | "kind" | "id">) => boolean): Overturned | null {
  return last && last.project === project && holds(last) ? last : null;
}

// The brief's last step when there is anything to ratify, Guard with a law to offer (#401, a bug fix's brief, before and after it
// merges), or a line at Ratify's head (`head`, #641: the map check failing at the PR's head): it holds the head's lines, the items and the
// guard's row, drawn by `map-brief.ts`. `on` is the parts the brief lit, so the map stays as the brief left it.
export const ratifyStep = (rows: RatifyRow[], on: string[] = [], guard = false, head = false): BriefStep | null => {
  const step = { k: "Ratify", on, hops: [], law: null, covered_by: null, list: [], risks: [], fig: null, check: null, cites: [], flags: [] };
  if (rows.length) return { ...step, h: "Accept or overturn", b: "What this PR changed or decided, and map text the review doubts. Nothing waits on this." };
  if (guard) return { ...step, h: "Guard with a law", b: "This PR fixed a bug and left no law behind it." };
  return head ? { ...step, h: "The map at this head", b: "Nothing here to accept or overturn." } : null;
};

// A link the part sheet marks, as an item to ratify: its ends by id as the brief's row says them, and its kind and reason as the new side
// an overturn quotes. The sheet has no old side, so `before` is null.
export function sheetLinkItem(link: { id: string; from: string; to: string; kind?: string; reason?: string }, pr: number) {
  const detail: RatifyDetail = { of: "link", kind: { before: null, after: link.kind ?? "" }, reason: { before: null, after: link.reason ?? "" } };
  return { pr, kind: "link", id: link.id, text: `${link.from} to ${link.to}`, change: "", detail };
}

// Ratify's head line for the map check at the PR's head (#641, spec #634 "The head's map check"), from the problems stored with the
// brief; none when the check passed, the head has no map, or the brief was stored before it.
export const mapCheckLine = (problems: string[] | undefined): string | null =>
  problems?.length ? `The map check fails at this head: ${problems.length} ${problems.length === 1 ? "problem" : "problems"}` : null;

// The Map text review's item ids (#642) by the Ratify row they belong to: a part's own text and its held items are its part row's.
const reviewPrefixes: Record<string, string[]> = { part: ["part"], link: ["link"], law: ["law"] };
const fieldWords: Record<string, string> = { name: "Name", purpose: "Purpose", caption: "Caption", reason: "Reason", text: "Text" };

/**
 * What the Map text review flagged on `row` (#645, spec #634 "Ratify's display"), one line per flagged item, as the not-confirmed popover
 * lists it: the field, the verdict, the reviewer's why and the lines it cites. A held item is named as the row's detail names it. None for
 * an unflagged row, a decision, or no review.
 */
export function reviewFlags(review: MapReview | null, row: Pick<RatifyRow, "kind" | "id" | "detail">): string[] {
  const prefixes = row.kind === "part" ? ["part", "held"] : (reviewPrefixes[row.kind] ?? []);
  const holds = row.detail?.of === "part" ? row.detail.holds : [];
  return (review?.items ?? []).flatMap((item) => {
    const at = item.id.indexOf(":");
    const slash = item.id.lastIndexOf("/");
    if (!item.flagged || at < 0 || slash < at) return [];
    const [kind, id, field] = [item.id.slice(0, at), item.id.slice(at + 1, slash), item.id.slice(slash + 1)];
    if (!prefixes.includes(kind) || id !== row.id) return [];
    const label = kind === "held" ? `Holds ${holds[Number(field) - 1]?.name ?? `item ${field}`}` : (fieldWords[field] ?? field);
    const cites = item.cites.length ? ` See ${item.cites.join(", ")}.` : "";
    return [`${label} (${item.verdict}): ${stopped(item.why)}${cites}`];
  });
}

/**
 * The flagged still-true items as Ratify rows of kind `still-true` (#698, spec #687 "Still true? rows"): the id is the flag's `ratify_id`,
 * which `ratify_set` keeps the decision under, and the text the head's. A flag decided before is left out; one decided while the brief
 * plays (`here`, by ratify id) stays, with its decision, as any Ratify row does. `map-brief.ts` draws them as "Still true?" rows.
 */
export const stillTrueRows = (review: MapReview | null, pr: number, here: ReadonlyMap<string, RatifyRow["status"]> = new Map()): RatifyRow[] =>
  (review?.still_true ?? [])
    .filter((flag) => !flag.decided || here.has(flag.ratify_id))
    .map((flag) => ({ pr, kind: "still-true", id: flag.ratify_id, text: flag.text, change: "", status: here.get(flag.ratify_id) ?? "not-yet", detail: null }));

/** What the not-confirmed popover lists for a flagged still-true item: the verdict, the reviewer's why and the lines it cites. */
export const stillTrueFlags = (flag: MapReviewFlag): string[] => [`Unchanged text (${flag.verdict}): ${stopped(flag.why)}${flag.cites.length ? ` See ${flag.cites.join(", ")}.` : ""}`];

/**
 * The Map text review's line at Ratify's head (#645): its words, and the button that starts it, "Review map text" before any review of a
 * PR not by Daniel's login, "Run again" after a failure (both `map_review_start`). None when the head changed no map text and its review did not
 * fail (#698).
 */
export function reviewLine(review: MapReview | null): { line: string | null; start: "Review map text" | "Run again" | null } | null {
  // A head that changed no map text has still-true items alone (#698): only a failure says anything, with Run again.
  if (!review || (!review.changed && review.state !== "failed")) return null;
  // Items whose id the review does not take are left out of its prompt (`map_review.rs`, `takes`), and the line says so.
  const left = review.left_out ?? 0;
  if (review.state === "idle") return { line: left ? `${left} left out for an id the review does not take` : null, start: "Review map text" };
  if (review.state === "reviewing") return { line: "Reviewing map text…", start: null };
  if (review.state === "failed") return { line: review.line ? `Map text review failed: ${review.line}` : "Map text review failed", start: "Run again" };
  const out = left ? `, ${left} left out for an id it does not take` : "";
  return { line: `Map text review: ${review.checked} checked, ${review.flagged} flagged${out}`, start: null };
}

// Whether Ratify's head alone brings the Ratify step, with nothing to ratify (`ratifyStep`'s `head`): the map check fails at the PR's head
// (#641), or the Map text review of a head that changed map text runs, has flagged something or left an item out (#645), or offers its
// button: "Review map text" before it ran, for a PR not by Daniel's login, and "Run again" after any review failed, which would be
// unreachable otherwise. A still-true flag brings the step as a row (`stillTrueRows`, #698), not here.
export const headBrings = (problems: string[] | undefined, review: MapReview | null): boolean =>
  !!mapCheckLine(problems) || review?.state === "failed" || (!!review?.changed && ["reviewing", "idle"].includes(review.state)) || (review?.flagged ?? 0) > 0 || (review?.left_out ?? 0) > 0;
