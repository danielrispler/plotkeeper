// The scan screen (spec danielrispler/plotkeeper#753, "The free scan"; ticket #760): a git repository with no map opens on its free scan. The
// proposed parts are grey plates, each with its folder and file count, with no fog, badges or footprints, and the import links the readers
// computed are lines between them. The panel beside them holds the languages line, Draft the map with its time hint, and the copy-prompt for
// another agent. Nothing here is saved: the plates are placed afresh on each draw, and the camera only fits them in the canvas.
// `map.ts` asks for the scan (`map_scan`) and picks this screen; `map-model.ts` holds its words.

import { availability, hint, pressable, svg, text } from "./dom.js";
import { fitCamera } from "./map-camera.js";
import { autoPlace, draftTime, languagesLine, linkLines, type MapDoc, parseMap, plateSize, rimPoint, scanPrompt } from "./map-model.js";
import { copyButton } from "./copy-button.js";
import type { MapScan, MapState } from "./types.js";

type Doc = Extract<MapDoc, { ok: true }>;

// The gutter kept around the plates when they are fitted to the canvas: `--map-gutter` in tokens.css.
const gutter = 32;
const name = (path: string) => path.split("/").filter(Boolean).pop() ?? path;
const filesWord = (n: number) => `${n} ${n === 1 ? "file" : "files"}`;

// The grey plates and their lines, fitted to the canvas now and again whenever the canvas changes size.
function drawCanvas(doc: Doc, files: Record<string, number>): HTMLElement {
  const canvas = text("div", "", "map-scan-canvas");
  const world = text("div", "", "map-world");
  const links = svg("svg", { class: "map-links", "aria-hidden": "true" });
  const at = autoPlace(doc.parts, doc.links, {});
  const plates = doc.parts.map((part) => {
    const plate = text("div", "", "plate draft");
    plate.dataset.id = part.id;
    const where = part.paths[0] ?? "";
    const caption = `${where} · ${filesWord(files[part.id] ?? 0)}`;
    plate.append(text("span", part.name, "plate-name"), text("span", caption, "plate-caption"));
    // A long folder is cut on the plate: its hint holds it whole.
    plate.dataset.key = `scan ${part.id}`;
    hint(plate, `${part.name}: ${caption}`);
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
  canvas.append(world);
  const spots = Object.values(at);
  const fit = () => {
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

// Draft the map, its time hint, and why it is muted when `claude` was not found. The run it starts is the drafting run's (#762).
function draftBlock(parts: number, claude: boolean): HTMLElement {
  const block = text("div", "", "map-scan-draft");
  const button = pressable(text("button", "Draft the map", "btn primary"), null);
  const why = claude ? null : "Draft the map needs the claude CLI, which plotkeeper did not find on your PATH. Give the prompt to your own agent instead.";
  availability(button, "Name and describe these parts with Claude Code, in the background; nothing is written until you accept", why);
  block.append(button, text("p", `${draftTime(parts)} · Uses your Claude Code plan`, "map-scan-time"));
  if (why) block.append(text("p", why, "map-scan-why"));
  return block;
}

// The copy-prompt for another agent: the skeleton, where to write the map and the check to pass.
function promptBlock(skeleton: string, leads: boolean): HTMLElement {
  const block = text("div", "", "map-scan-prompt");
  const prompt = scanPrompt(skeleton);
  const row = text("div", "", "map-draft-row");
  const pre = text("pre", prompt, "map-draft");
  // The prompt is for an agent, with the skeleton's JSON in it: a page translator leaves it as it is.
  pre.setAttribute("translate", "no");
  row.append(pre, copyButton("Copy the prompt", "Copy this prompt, to paste into your own agent in the project", () => prompt));
  block.append(text("p", leads ? "Give this to your own agent in the project:" : "Or give this to your own agent in the project:", "empty"), row);
  return block;
}

/** Draws `state`'s project's scan screen into `body`; a scan whose map does not read draws nothing and says false. */
export function drawScan(body: HTMLElement, state: MapState, scan: MapScan): boolean {
  const doc = parseMap(scan.map);
  if (!doc.ok) return false;
  const panel = text("aside", "", "map-scan-panel");
  panel.setAttribute("aria-label", "Free scan");
  const count = doc.parts.length;
  panel.append(
    text("p", "No map yet", "map-empty-title"),
    text("p", `${name(state.project)} has no system-map.json on ${state.reference ?? "the default branch"}.`, "empty"),
    text("p", "A map names your parts and their rules; changes light them up.", "empty"),
    text("p", count ? `A free scan of its folders proposes ${count === 1 ? "this part" : `these ${count} parts`}, with the links its imports show. Nothing is written until you draft.` : "A free scan of its folders found no code to propose parts from.", "empty"),
  );
  const languages = languagesLine(scan.read_languages, scan.unread_languages);
  if (languages) panel.append(text("p", languages, "map-scan-languages"));
  // Without `claude` the copy-prompt leads (#753, story 12).
  const [draft, prompt] = [draftBlock(count, scan.claude), promptBlock(scan.map, !scan.claude)];
  panel.append(...(scan.claude ? [draft, prompt] : [prompt, draft]));
  const screen = text("div", "", "map-scan");
  screen.append(drawCanvas(doc, scan.files), panel);
  body.replaceChildren(screen);
  return true;
}
