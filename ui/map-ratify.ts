// The accept and overturn buttons of an item to ratify (spec danielrispler/plotkeeper#334, ticket #343), shared by the part sheet's law and
// link rows and the brief's ratify step. Ratifying never blocks anything: Accept only records that Daniel has looked. Overturn records that
// too, then copies a prompt that quotes the item to the clipboard for him to paste into his agent (port decision Q2); when the clipboard
// refuses, the prompt shows in a small box with a Copy button.

import { failure, hint, invoke, pressable, promptBox, sayAction, text } from "./dom.js";
import { currentMap } from "./map.js";
import { openDrawer } from "./map-drawer.js";
import { copyOverturn, decideWith, detailLines, evidenceLine, mapCheckLine, noteFor, overturnPrompt, overturnReport, siteDrawer, sitesDrawer, type Overturned } from "./ratify.js";
import { logStop } from "./stop-rules.js";
import type { ImportSite, LinkSites, RatifyRow } from "./types.js";

export type Ratifiable = Pick<RatifyRow, "pr" | "kind" | "id" | "text" | "change"> & { detail?: RatifyRow["detail"]; evidence?: LinkSites };

/** The branch of PR `pr` while it is open: the Map view's ready PRs name it. */
export const branchOf = (pr: number): string | null => currentMap()?.state.ready?.find((ready) => ready.number === pr)?.branch || null;

// What the last overturn did with its prompt, and for which item, kept so the sheet and the brief that hold the item, which draw again
// once the decision is recorded, show it.
let last: Overturned | null = null;

// Puts the overturn's prompt on the clipboard (`navigator.clipboard.writeText()`); a refusal leaves it for `overturnNote` to show in a box.
async function overturnOut(project: string, item: Ratifiable): Promise<Overturned> {
  const prompt = overturnPrompt(item, branchOf(item.pr));
  const outcome = await copyOverturn(prompt, navigator.clipboard?.writeText?.bind(navigator.clipboard));
  return { project, pr: item.pr, kind: item.kind, id: item.id, ...overturnReport(outcome, prompt) };
}

/**
 * The note of the last overturn, where `holds` says its item is drawn in `project`: "Prompt copied, paste it into your agent", or, when
 * the copy failed, the prompt in a small box with a Copy button. Null before any overturn, and on another item's sheet or brief. The note
 * is drawn again with its sheet or brief, so it is not live: the overturn says its line (`sayAction`, #465).
 */
export function overturnNote(project: string, holds: (item: Pick<Ratifiable, "pr" | "kind" | "id">) => boolean): HTMLElement | null {
  const shown = noteFor(last, project, holds);
  if (!shown) return null;
  const note = text("div", "", "ratify-note");
  note.append(text("p", shown.line, "ratify-copied"));
  const manual = shown.manual;
  if (manual === null) return note;
  const area = promptBox(manual);
  const copy = pressable(text("button", "Copy", "btn quiet ratify-copy"), null);
  hint(copy, "Copy the prompt to paste into your agent");
  copy.addEventListener("click", () => {
    area.select();
    // The clipboard refused already, so this is the browser's older route, which a click may use.
    if (document.execCommand("copy")) copy.textContent = "Copied";
  });
  note.append(area, copy);
  return note;
}

/**
 * A part's or a link's detail in the brief's ratify step (#485): each changed text, old beside new, in words as well as marks, so the
 * change does not rest on colour or a strike alone. Null for a row with nothing to show.
 */
export function ratifyDetail(row: Ratifiable): HTMLElement | null {
  const lines = detailLines(row.detail);
  if (!lines.length) return null;
  const list = text("dl", "", "ratify-detail");
  for (const line of lines) {
    const said = text("dd", "");
    if (line.code) said.translate = false;
    const side = (label: string, words: string, tag: "del" | "ins") => {
      const each = text("p", "", `ratify-${tag === "del" ? "was" : "now"}`);
      each.append(text("span", label, "ratify-side"), text(tag, words || "none"));
      said.append(each);
    };
    if (line.before !== null) side("Was", line.before, "del");
    if (line.after !== null) side("Now", line.after, "ins");
    list.append(text("dt", line.label), said);
  }
  return list;
}

/**
 * An added link's evidence in the brief's ratify step (#499), its second line: one import site, which opens its code in the drawer as
 * the PR's head holds it, then "(+N imports across M files)", which opens every site there, each opening its code. Null with none.
 */
export function ratifyEvidence(project: string, row: Ratifiable): HTMLElement | null {
  const said = evidenceLine(row.evidence);
  const evidence = row.evidence;
  const first = evidence?.sites[0];
  if (!said || !evidence || !first) return null;
  const openSite = (site: ImportSite) =>
    invoke("ratify_site", { project, head: evidence.head, file: site.file, line: site.line ?? null })
      .then((found) => openDrawer(siteDrawer(found, evidence.head)))
      .catch(failure("ratify_site"));
  const line = text("p", "", "ratify-evidence");
  const site = pressable(text("button", said.site, "btn quiet ratify-site"), null, () => void openSite(first));
  site.translate = false;
  hint(site, "Show this import's code as the PR's head holds it");
  line.append(text("span", "Import", "ratify-side"), site);
  if (said.more) {
    const more = pressable(text("button", said.more, "btn quiet ratify-more"), null, () => openDrawer(sitesDrawer(row, openSite)));
    hint(more, "List every import behind this link");
    line.append(more);
  }
  return line;
}

/** `done` runs once the server has the decision. */
export function ratifyActions(project: string, item: Ratifiable, done: (state: "accepted" | "overturned") => void): HTMLElement {
  const box = text("span", "", "ratify-actions");
  const decide = (state: "accepted" | "overturned") => {
    box.querySelectorAll("button").forEach((button) => (button.disabled = true));
    decideWith(
      state === "overturned",
      () => overturnOut(project, item),
      async () => void (await invoke("ratify_set", { project, pr: item.pr, kind: item.kind, id: item.id, state, text: item.text })),
    )
      .then((overturned) => {
        logStop("ratify", project, item.pr);
        if (overturned) {
          last = overturned;
          sayAction(overturned.line);
        }
        done(state);
      })
      .catch((e) => {
        box.querySelectorAll("button").forEach((button) => (button.disabled = false));
        failure("ratify_set")(e);
      });
  };
  const accept = pressable(text("button", "Accept", "btn quiet ratify-accept"), null);
  hint(accept, "Record that you have looked at this and it stands; nothing waits on it");
  accept.addEventListener("click", () => decide("accepted"));
  const overturn = pressable(text("button", "Overturn", "btn quiet ratify-overturn"), null);
  hint(overturn, "Undo it: copies a prompt that quotes it, to paste into your agent");
  overturn.addEventListener("click", () => decide("overturned"));
  box.append(accept, overturn);
  return box;
}

/**
 * Ratify's head (#641): the lines above the step's items that say how the PR's head stands, the map check's and, later, the Map text
 * review's (spec #634), in the order given. Null with none.
 */
export function ratifyHead(lines: HTMLElement[]): HTMLElement | null {
  if (!lines.length) return null;
  const head = text("div", "", "ratify-head");
  head.append(...lines);
  return head;
}

// The map check lines left open, by project, PR and head: the step is drawn again on every map draw, and an open list stays open.
const openChecks = new Set<string>();

/**
 * The head's map check line (#641): "The map check fails at this head: N problems", a disclosure that opens to the problems, each as the
 * check words it. `key` names the brief (project, PR and head). Null when the check passed.
 */
export function mapCheckNode(key: string, problems: string[]): HTMLElement | null {
  const line = mapCheckLine(problems);
  if (!line) return null;
  const node = text("details", "", "ratify-check");
  node.open = openChecks.has(key);
  node.addEventListener("toggle", () => (node.open ? openChecks.add(key) : openChecks.delete(key)));
  const list = text("ul", "", "ratify-problems");
  for (const problem of problems) {
    const each = text("li", problem);
    // The check's words name files and ids, which a translator must leave alone.
    each.translate = false;
    list.append(each);
  }
  const summary = text("summary", line);
  hint(summary, "List what system-map check finds wrong at this PR's head");
  node.append(summary, list);
  return node;
}

// The marks the pointed Ratify row lit (#643), and which row lit them: one row at a time, so a row drawn again before its leave was seen
// leaves nothing lit once another is pointed.
let lit: { row: HTMLElement; marks: Element[] } | null = null;
const unlightMarks = () => {
  for (const node of lit?.marks ?? []) node.classList.remove("ratify-lit");
  lit = null;
};

/**
 * A Ratify row's light (#643, spec #634 "The canvas"): the row carries the id of the mark it lights on the Map diff (`mark`, from
 * `pr_ratify`), and hovering or focusing it lights that plate, line or law mark in `canvas` with `ratify-lit`, a class of its own: the
 * brief adds and strips `.lit` itself, and story-to-map clears `.turn-lit` from every plate whenever the map repaints. Leave and blur
 * clear it.
 */
export function lightsMark(row: HTMLElement, mark: string, canvas: () => ParentNode | null) {
  row.dataset.mark = mark;
  const on = () => {
    unlightMarks();
    const marks = [...(canvas()?.querySelectorAll(`[data-mark="${mark.replaceAll('"', '\\"')}"]`) ?? [])];
    for (const node of marks) node.classList.add("ratify-lit");
    lit = { row, marks };
  };
  const off = () => {
    if (lit?.row === row) unlightMarks();
  };
  row.addEventListener("pointerenter", on);
  row.addEventListener("focusin", on);
  row.addEventListener("pointerleave", off);
  row.addEventListener("focusout", off);
}
