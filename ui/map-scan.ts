// The scan screen (spec danielrispler/plotkeeper#753, "The free scan"; tickets #760, #761): a git repository with no map opens on its free scan. The
// proposed parts are grey plates, each with its folder and file count, with no fog, badges or footprints, and the import links the readers
// computed are lines between them. The panel beside them holds the languages line, the skeleton's optional edits (the folders to ignore, the
// loose files and tests to give a part), Draft the map with its time hint, and the copy-prompt for another agent. A plate's press opens its
// Merge into… and Split by subfolders. Each edit goes to the server, which computes the lines again (`map_scan_edit`); its answer redraws the
// plates and lines and patches the panel in place, so the panel keeps its scroll and its focus. Nothing here is saved: the skeleton is the
// value `map.ts` holds, the plates are placed afresh on each draw, and the camera only fits them in the canvas. `map-model.ts` holds its words.

import { availability, hint, isUnavailable, patch, pressable, svg, text } from "./dom.js";
import { fitCamera } from "./map-camera.js";
import { autoPlace, draftTime, languagesLine, linkLines, looseGroups, type MapDoc, parseMap, plateSize, rimPoint, scanPrompt } from "./map-model.js";
import { copyButton } from "./copy-button.js";
import type { MapScan, MapState, ScanEdit } from "./types.js";

type Doc = Extract<MapDoc, { ok: true }>;
/**
 * What the scan screen's presses do: `edit` sends one edit to the server, and `note` is why the last one did not apply, or null; `draft`
 * starts the drafting run from the skeleton as the edits leave it (#763), and `refused` is why the last start was refused, or null.
 */
export type ScanActs = { edit: (edit: ScanEdit) => void; note: string | null; draft: () => void; refused: string | null };

// The gutter kept around the plates when they are fitted to the canvas: `--map-gutter` in tokens.css.
const gutter = 32;
const name = (path: string) => path.split("/").filter(Boolean).pop() ?? path;
const filesWord = (n: number) => `${n} ${n === 1 ? "file" : "files"}`;
// "a/, b/ and c/".
const listed = (items: string[]) => (items.length < 2 ? items.join("") : `${items.slice(0, -1).join(", ")} and ${items.at(-1)}`);
// The plate to focus once the next draw is in: a merge's target, or the split part; its plate's key.
let focusAfter: string | null = null;
// The scan each screen's plates were drawn from.
const canvasOf = new WeakMap<HTMLElement, MapScan>();

// A plate's actions: Merge into… the other parts, and Split by subfolders, muted with a reason when its folders have none to split by.
function plateActions(menu: HTMLElement, doc: Doc, scan: MapScan, id: string, acts: ScanActs) {
  const part = doc.parts.find((each) => each.id === id);
  if (!part) return;
  const close = () => menu.hidePopover();
  const nodes: HTMLElement[] = [text("p", part.name, "map-scan-actions-title")];
  const others = doc.parts.filter((each) => each.id !== id);
  if (others.length) {
    const merge = text("form", "", "map-scan-merge");
    const pick = text("select", "", "map-scan-pick");
    pick.id = "map-scan-merge-into";
    pick.name = "into";
    pick.append(...others.map((other) => Object.assign(text("option", other.name), { value: other.id })));
    const label = text("label", "Merge into", "map-scan-label");
    label.htmlFor = pick.id;
    const go = Object.assign(pressable(text("button", "Merge", "btn small"), null), { type: "submit" });
    go.dataset.key = `scan merge ${id}`;
    hint(go, `Put ${part.name}'s folders into the part chosen; the lines are drawn again`);
    merge.append(label, pick, go);
    merge.addEventListener("submit", (event) => {
      event.preventDefault();
      focusAfter = `scan ${pick.value}`;
      close();
      acts.edit({ kind: "merge", part: id, into: pick.value });
    });
    nodes.push(merge);
  }
  const into = scan.splits[id];
  const split = pressable(text("button", "Split by subfolders", "btn small"), null, () => {
    if (isUnavailable(split)) return;
    focusAfter = `scan ${id}`;
    close();
    acts.edit({ kind: "split", part: id });
  });
  split.dataset.key = `scan split ${id}`;
  availability(split, `Make a part of each of ${listed(into ?? [])}`, into ? null : "Its folders have no subfolders of code to split by");
  nodes.push(split);
  menu.replaceChildren(...nodes);
}

// A grey plate: its name, its folder (and how many more a merged part has) and file count, on a button that opens its actions.
function plateOf(part: Doc["parts"][number], files: number, press: (plate: HTMLElement, open: HTMLElement, id: string, event: MouseEvent) => void): HTMLElement {
  const plate = text("div", "", "plate draft");
  plate.dataset.id = part.id;
  const where = part.paths.length > 1 ? `${part.paths[0]} +${part.paths.length - 1}` : (part.paths[0] ?? "");
  const open: HTMLButtonElement = pressable(text("button", "", "btn plate-open"), null, (event) => press(plate, open, part.id, event));
  open.append(text("span", part.name, "plate-name"), text("span", `${where} · ${filesWord(files)}`, "plate-caption"));
  open.setAttribute("aria-expanded", "false");
  // A long folder is cut on the plate: its hint holds it whole, with every folder of a merged part.
  open.dataset.key = `scan ${part.id}`;
  hint(open, `${part.name}: ${part.paths.join(", ")} · ${filesWord(files)}\nMerge or split it`);
  plate.append(open);
  return plate;
}

// Places an open `menu` under `plate`'s box on the screen, over it when there is no room below, always inside the window.
function placeBeside(menu: HTMLElement, plate: HTMLElement) {
  const box = plate.getBoundingClientRect();
  const [w, h, room] = [menu.offsetWidth, menu.offsetHeight, 8];
  const below = box.bottom + room + h <= window.innerHeight - room;
  menu.style.left = `${Math.max(room, Math.min(box.left, window.innerWidth - w - room))}px`;
  menu.style.top = `${Math.max(room, below ? box.bottom + room : box.top - room - h)}px`;
}

/**
 * `doc`'s plates, each made by `plateOf`, and its lines, fitted to a canvas now and again whenever the canvas changes size; `menu`, a popover
 * the canvas holds, is closed by a fit. The scan's grey plates and the Map draft's (#763) are placed the same way, afresh on each draw.
 */
export function plateCanvas(doc: Doc, plateOf: (part: Doc["parts"][number]) => HTMLElement, menu: HTMLElement | null = null): HTMLElement {
  const canvas = text("div", "", "map-scan-canvas");
  const world = text("div", "", "map-world");
  const links = svg("svg", { class: "map-links", "aria-hidden": "true" });
  const at = autoPlace(doc.parts, doc.links, {});
  const plates = doc.parts.map((part) => {
    const plate = plateOf(part);
    const spot = at[part.id];
    if (spot) Object.assign(plate.style, { left: `${spot.x}px`, top: `${spot.y}px` });
    return plate;
  });
  links.replaceChildren(...linkLines(doc.links).flatMap((line) => {
    const [from, to] = [at[line.from], at[line.to]];
    if (!from || !to) return [];
    const [a, b] = [rimPoint(from, plateSize, to), rimPoint(to, plateSize, from)];
    return [svg("line", { class: "link", "data-link": line.ids.join(" "), x1: String(a.x), y1: String(a.y), x2: String(b.x), y2: String(b.y) })];
  }));
  world.append(links, ...plates);
  canvas.append(world, ...(menu ? [menu] : []));
  const spots = Object.values(at);
  const fit = () => {
    if (menu?.matches(":popover-open")) menu.hidePopover();
    if (!spots.length) return;
    const box = {
      x0: Math.min(...spots.map((p) => p.x)) - plateSize.w / 2, y0: Math.min(...spots.map((p) => p.y)) - plateSize.h / 2,
      x1: Math.max(...spots.map((p) => p.x)) + plateSize.w / 2, y1: Math.max(...spots.map((p) => p.y)) + plateSize.h / 2,
    };
    const { scale, x, y } = fitCamera(box, { w: canvas.clientWidth, h: canvas.clientHeight }, gutter);
    world.style.transformOrigin = "0 0";
    world.style.transform = `translate(${x}px, ${y}px) scale(${scale})`;
  };
  const resized = new ResizeObserver(() => (canvas.isConnected ? fit() : resized.disconnect()));
  resized.observe(canvas);
  return canvas;
}

// The grey plates and their lines; a plate's press opens its actions.
function drawCanvas(doc: Doc, scan: MapScan, acts: ScanActs): HTMLElement {
  // The actions of the plate last pressed: one popover in the top layer, placed by the plate's box on the screen, as the badges' list is.
  const menu = text("div", "", "menu map-scan-actions");
  menu.setAttribute("popover", "auto");
  menu.setAttribute("aria-label", "Change this part");
  let opener: HTMLElement | null = null;
  menu.addEventListener("toggle", (event) => {
    if ((event as ToggleEvent).newState === "closed") opener?.setAttribute("aria-expanded", "false");
  });
  const press = (plate: HTMLElement, open: HTMLElement, id: string, event: MouseEvent) => {
    if (opener === open && menu.matches(":popover-open")) return menu.hidePopover();
    plateActions(menu, doc, scan, id, acts);
    opener = open;
    menu.showPopover({ source: open });
    open.setAttribute("aria-expanded", "true");
    placeBeside(menu, plate);
    // A keyboard press (no pointer, `detail` 0) goes into the actions.
    if (event.detail === 0) menu.querySelector<HTMLElement>("select, button")?.focus();
  };
  return plateCanvas(doc, (part) => plateOf(part, scan.files[part.id] ?? 0, press), menu);
}

// The folders that look like copies or vendored code, ticked while the skeleton ignores them (#753, story 7).
function ignoreBlock(scan: MapScan, acts: ScanActs): HTMLElement | null {
  if (!scan.ignore_candidates.length) return null;
  const set = text("fieldset", "", "map-scan-set");
  set.dataset.key = "scan ignore";
  set.append(text("legend", "Copies and vendored code"), text("p", "Ticked folders are left out of the map.", "map-scan-note"));
  for (const candidate of scan.ignore_candidates) {
    const row = text("label", "", "map-scan-check");
    row.dataset.key = `scan ignore ${candidate.path}`;
    const box = text("input", "");
    box.type = "checkbox";
    box.name = "ignore";
    box.value = candidate.path;
    if (candidate.ticked) box.setAttribute("checked", "");
    box.checked = candidate.ticked;
    box.addEventListener("change", () => acts.edit({ kind: "ignore", folder: candidate.path, ignored: box.checked }));
    const path = text("code", candidate.path);
    path.setAttribute("translate", "no");
    row.append(box, path);
    set.append(row);
  }
  return set;
}

// The loose files or tests, one row a folder, each offered a part or a new part (#753, story 8).
function looseBlock(kind: "files" | "tests", files: string[], doc: Doc, acts: ScanActs): HTMLElement | null {
  const groups = looseGroups(files);
  if (!groups.length) return null;
  const set = text("fieldset", "", "map-scan-set");
  set.dataset.key = `scan loose ${kind}`;
  set.append(
    text("legend", kind === "files" ? "Loose files" : "Loose tests"),
    text("p", kind === "files" ? "No part holds these yet." : "No part holds these tests yet.", "map-scan-note"),
  );
  groups.forEach((group, i) => {
    const row = text("form", "", "map-scan-loose");
    row.dataset.key = `scan loose ${kind} ${group.folder}`;
    // The files go with the form, so a row the panel keeps across a redraw sends what the latest draw listed.
    row.dataset.files = JSON.stringify(group.files);
    const where = text("p", "", "map-scan-where");
    const folder = text("code", group.label === "The root" ? group.label : group.folder);
    folder.setAttribute("translate", "no");
    where.append(folder, text("span", ` · ${group.names}`, "map-scan-names"));
    const pick = text("select", "", "map-scan-pick");
    pick.id = `map-scan-home-${kind}-${i}`;
    pick.name = "part";
    pick.append(Object.assign(text("option", "A new part"), { value: "" }), ...doc.parts.map((part) => Object.assign(text("option", part.name), { value: part.id })));
    const label = text("label", "Give to", "map-scan-label");
    label.htmlFor = pick.id;
    const add = Object.assign(pressable(text("button", "Add", "btn small"), null), { type: "submit" });
    add.dataset.key = `scan add ${kind} ${group.folder}`;
    hint(add, kind === "files" ? "Give these files to the part chosen, or to a new part" : "Give these tests to the part chosen, or to a new part");
    row.append(where, label, pick, add);
    row.addEventListener("submit", (event) => {
      event.preventDefault();
      const form = event.currentTarget as HTMLFormElement;
      const chosen = form.querySelector("select")?.value ?? "";
      acts.edit({ kind: "home", files: JSON.parse(form.dataset.files ?? "[]") as string[], part: chosen || null });
    });
    set.append(row);
  });
  return set;
}

// Draft the map, its time hint, and why it is muted when `claude` was not found; it starts the drafting run (#762) with the skeleton
// (#763), and says why when the server refused it.
function draftBlock(parts: number, claude: boolean, acts: ScanActs): HTMLElement {
  const block = text("div", "", "map-scan-draft");
  block.dataset.key = "scan draft";
  const button = pressable(text("button", "Draft the map", "btn primary"), null, () => {
    if (!isUnavailable(button)) acts.draft();
  });
  button.dataset.key = "scan draft button";
  const why = claude ? null : "Draft the map needs the claude CLI, which plotkeeper did not find on your PATH. Give the prompt to your own agent instead.";
  availability(button, "Name and describe these parts with Claude Code, in the background; nothing is written until you accept", why);
  block.append(button, text("p", `${draftTime(parts)} · Uses your Claude Code plan`, "map-scan-time"));
  if (why) block.append(text("p", why, "map-scan-why"));
  if (acts.refused) {
    const refused = text("p", `Draft the map did not start: ${acts.refused}`, "map-scan-why");
    refused.dataset.key = "scan draft refused";
    refused.setAttribute("role", "status");
    block.append(refused);
  }
  return block;
}

// The copy-prompt for another agent: the skeleton, where to write the map and the check to pass. The copy reads the prompt as drawn, so a
// panel kept across an edit copies the edited skeleton.
function promptBlock(skeleton: string, leads: boolean): HTMLElement {
  const block = text("div", "", "map-scan-prompt");
  block.dataset.key = "scan prompt";
  const prompt = scanPrompt(skeleton);
  const row = text("div", "", "map-draft-row");
  const pre = text("pre", prompt, "map-draft");
  // The prompt is for an agent, with the skeleton's JSON in it: a page translator leaves it as it is.
  pre.setAttribute("translate", "no");
  const copy = copyButton("Copy the prompt", "Copy this prompt, to paste into your own agent in the project", () => row.querySelector("pre")?.textContent ?? prompt);
  copy.dataset.key = "scan copy";
  row.append(pre, copy);
  block.append(text("p", leads ? "Give this to your own agent in the project:" : "Or give this to your own agent in the project:", "empty"), row);
  return block;
}

// The panel: what the scan found, the skeleton's optional edits, Draft the map and the copy-prompt; each node keyed, so a kept panel is patched.
function panelOf(state: MapState, doc: Doc, scan: MapScan, acts: ScanActs): HTMLElement {
  const panel = text("aside", "", "map-scan-panel");
  panel.setAttribute("aria-label", "Free scan");
  const count = doc.parts.length;
  const said = (line: string, className: string, key: string) => {
    const node = text("p", line, className);
    node.dataset.key = key;
    return node;
  };
  panel.append(
    said("No map yet", "map-empty-title", "scan title"),
    said(`${name(state.project)} has no system-map.json on ${state.reference ?? "the default branch"}.`, "empty", "scan none"),
    said("A map names your parts and their rules; changes light them up.", "empty", "scan teach"),
    said(count ? `A free scan of its folders proposes ${count === 1 ? "this part" : `these ${count} parts`}, with the links its imports show. Nothing is written until you draft.` : "A free scan of its folders found no code to propose parts from.", "empty", "scan proposes"),
  );
  const languages = languagesLine(scan.read_languages, scan.unread_languages);
  if (languages) panel.append(said(languages, "map-scan-languages", "scan languages"));
  // The edits are optional (#753, story 10): Draft the map needs none of them.
  if (count) panel.append(said("Fix the grouping first if you like; none of it is needed. Press a plate to merge or split it.", "map-scan-note", "scan edits"));
  if (acts.note) {
    const note = said(`That change did not apply: ${acts.note}. Try it again, or draft the map as it is.`, "map-scan-why", "scan failed");
    note.setAttribute("role", "status");
    panel.append(note);
  }
  panel.append(...[ignoreBlock(scan, acts), looseBlock("files", scan.loose_files, doc, acts), looseBlock("tests", scan.loose_tests, doc, acts)].filter((node) => node !== null));
  // Without `claude` the copy-prompt leads (#753, story 12).
  const [draft, prompt] = [draftBlock(count, scan.claude, acts), promptBlock(scan.map, !scan.claude)];
  panel.append(...(scan.claude ? [draft, prompt] : [prompt, draft]));
  return panel;
}

/**
 * Draws `state`'s project's scan screen into `body`; a scan whose map does not read draws nothing and says false. A screen already drawn for
 * the project keeps its panel: the plates and lines are drawn again and the panel is patched, so its scroll and focus stay.
 */
export function drawScan(body: HTMLElement, state: MapState, scan: MapScan, acts: ScanActs): boolean {
  const doc = parseMap(scan.map);
  if (!doc.ok) return false;
  const panel = panelOf(state, doc, scan, acts);
  const key = `scan ${state.project}`;
  const drawn = body.firstElementChild;
  const same = drawn instanceof HTMLElement && drawn.classList.contains("map-scan") && drawn.dataset.key === key && body.childElementCount === 1;
  // A draw of the scan already drawn (a focus, a session's event) keeps its plates, and a plate's open actions with them.
  const canvas = same && canvasOf.get(drawn) === scan ? null : drawCanvas(doc, scan, acts);
  if (same) {
    if (canvas) drawn.querySelector(".map-scan-canvas")?.replaceWith(canvas);
    const kept = drawn.querySelector(".map-scan-panel");
    if (kept) patch(kept, [...panel.childNodes]);
    // A kept box the person ticked says the skeleton's tick again, as an edit that did not apply leaves it.
    for (const box of drawn.querySelectorAll<HTMLInputElement>(".map-scan-check input")) box.checked = box.hasAttribute("checked");
  } else {
    const screen = text("div", "", "map-scan");
    screen.dataset.key = key;
    screen.append(canvas ?? drawCanvas(doc, scan, acts), panel);
    body.replaceChildren(screen);
  }
  canvasOf.set(body.firstElementChild as HTMLElement, scan);
  if (canvas && focusAfter !== null) {
    const want = focusAfter;
    focusAfter = null;
    const plates = [...body.querySelectorAll<HTMLElement>(".map-scan-canvas .plate-open")];
    (plates.find((plate) => plate.dataset.key === want) ?? plates[0])?.focus({ preventScroll: true });
  }
  return true;
}
