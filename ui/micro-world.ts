// Micro-worlds on the page (spec danielrispler/plotkeeper#378, ticket #399). "Explore this" on a flow (in a part's refresher) or on a PR brief's
// figure asks the server to build a small simulator of the mechanism with a background run (`server/src/micro_worlds.rs`); the map's payload says
// where each stands (`MapState.micro_worlds`). Without `microWorlds.dshVisual` it offers only the prompt to copy. A checked page is fetched
// through the API and shown by `srcdoc` in a frame sandboxed to `allow-scripts`, never `allow-same-origin`, so the page has an opaque origin, no
// network (its policy says so too) and no way to plotkeeper's API. A frame can still navigate itself, so plotkeeper's page is served with
// `frame-src 'none'` (`server/src/web.rs`): no frame of it loads a URL. The frame sits in an overlay in the glass the size of the settings
// dialog, the first layer Esc closes.

import { element, failure, invoke, text } from "./dom.js";
import { onDraw, registerLayer, reloadMap, shownProject } from "./map.js";
import { copyOverturn, overturnReport } from "./ratify.js";
import { logStop } from "./stop-rules.js";
import type { MicroWorldView, MicroWorlds } from "./types.js";

// What a world's row offers: its words, and what a click does; `act` null is a word with no click, such as "Building…".
export type Offer = { label: string; title: string; act: "explore" | "open" | "copy" | null };
export type ExploreActs = { explore: () => void; open: () => void; copy: (prompt: string) => void };
export type WorldActs = { rebuild: () => void; close: () => void };

const staleWords = "This may be out of date: a part on it changed since it was built";

// What "Explore this" offers for world `key`, by where it stands. Without the setting there is no run, so only the prompt to copy, which the
// server gives in place of a run; a page built before still opens.
export function exploreOffers(worlds: MicroWorlds | null | undefined, key: string): Offer[] {
  const world: MicroWorldView | undefined = worlds?.worlds[key];
  const open: Offer[] = world?.page ? [{ label: "Open", title: "Open the micro-world", act: "open" }] : [];
  if (!worlds?.on) return open.length ? open : [{ label: "Copy prompt", title: "Copy a prompt that builds this micro-world, to paste into your own agent", act: "explore" }];
  if (!world) return [{ label: "Explore this", title: "Build a small simulator of this mechanism from the real code", act: "explore" }];
  switch (world.phase) {
    case "building":
      return [...open, { label: "Building…", title: "A background run is building it", act: null }];
    case "ready":
      return world.stale ? [...open, { label: "Rebuild", title: "Build it again from the code as it is now", act: "explore" }] : open;
    case "failed":
      return [
        ...open,
        { label: "Copy prompt", title: "Copy a prompt that builds it, to paste into your own agent", act: "copy" },
        { label: "Try again", title: "Start another run", act: "explore" },
      ];
  }
}

const button = (words: string, title: string, click: (() => void) | null) => {
  const node = text("button", words, "btn quiet");
  node.setAttribute("type", "button");
  node.title = title;
  if (click) node.addEventListener("click", click);
  else (node as HTMLButtonElement).disabled = true;
  return node;
};

// The row under a flow or a figure: the offers, "may be out of date" when a part on it changed, and why it failed.
export function exploreNode(worlds: MicroWorlds | null | undefined, key: string, act: ExploreActs): HTMLElement {
  const node = text("div", "", "micro-world-row");
  node.dataset.world = key;
  const world = worlds?.worlds[key];
  const click = (offer: Offer) => {
    if (offer.act === "copy") return () => act.copy(world?.prompt ?? "");
    return offer.act ? act[offer.act] : null;
  };
  node.append(...exploreOffers(worlds, key).map((offer) => button(offer.label, offer.title, click(offer))));
  if (world?.page && world.stale) node.append(text("span", staleWords, "micro-world-stale"));
  if (world?.phase === "failed" && world.line) node.append(text("p", world.line, "micro-world-why"));
  return node;
}

// The frame a checked page is shown in: by `srcdoc`, never a URL, sandboxed to scripts alone, so it has an opaque origin, sends no cookie and
// cannot reach plotkeeper's API.
export function frameNode(html: string): HTMLElement {
  const frame = document.createElement("iframe");
  frame.className = "micro-world-frame";
  frame.setAttribute("sandbox", "allow-scripts");
  frame.setAttribute("referrerpolicy", "no-referrer");
  frame.setAttribute("title", "Micro-world");
  frame.setAttribute("srcdoc", html);
  return frame;
}

// The overlay's nodes: the head with the world's name, "may be out of date" with Rebuild, and Close; then the frame.
export function worldNodes(name: string, html: string, world: MicroWorldView | undefined, act: WorldActs): HTMLElement[] {
  const head = text("div", "", "micro-world-head");
  head.append(text("h2", name, "micro-world-title"));
  if (world?.stale) head.append(text("span", staleWords, "micro-world-stale"), button("Rebuild", "Build it again from the code as it is now", act.rebuild));
  head.append(button("Close", "Close the micro-world (Esc)", act.close));
  return [head, frameNode(html)];
}

// The overlay as it is now: the project, the world's key and name, and its page; null while closed.
type Shown = { project: string; key: string; name: string; html: string; explore: () => void };
let shown: Shown | null = null;
const overlay = () => element("map-micro-world");

// Draws the world into `box`, the overlay; a box that already shows the same page keeps its frame and gets a new head alone.
export function paintWorld(box: HTMLElement, name: string, html: string, world: MicroWorldView | undefined, act: WorldActs) {
  const [head, frame] = worldNodes(name, html, world, act);
  const [shownHead, shownFrame] = box.children;
  if (shownHead && shownFrame?.getAttribute("srcdoc") === html) box.replaceChild(head, shownHead);
  else box.replaceChildren(head, frame);
}

function paint(worlds?: MicroWorlds) {
  const box = overlay();
  if (!shown) {
    box.hidden = true;
    box.replaceChildren();
    return;
  }
  const now = shown;
  paintWorld(box, now.name, now.html, worlds?.worlds[now.key], { rebuild: () => (close(), now.explore()), close });
  box.hidden = false;
}

function close() {
  shown = null;
  paint();
}

// Opens world `key` of `project` (named `name`): its checked page fetched through the API, logged for the stop rules.
export async function openMicroWorld(project: string, key: string, name: string, worlds: MicroWorlds | undefined, explore: () => void) {
  try {
    const html = await invoke("micro_world_page", { project, key });
    if (shownProject() !== project) return;
    shown = { project, key, name, html, explore };
    logStop("micro-world-opened", project);
    paint(worlds);
  } catch (e) {
    failure("micro_world_page", "error")(e);
  }
}

// What a copy left to show, by world: its line, and the prompt in a box when the clipboard refused.
const copied = new Map<string, { line: string; manual: string | null }>();

function noteNodes(key: string): HTMLElement[] {
  const note = copied.get(key);
  if (!note) return [];
  const line = text("p", note.line, "run-copied");
  line.setAttribute("role", "status");
  if (note.manual === null) return [line];
  const area = document.createElement("textarea");
  area.className = "ratify-prompt";
  area.value = note.manual;
  area.readOnly = true;
  area.rows = 4;
  area.name = "prompt";
  area.autocomplete = "off";
  area.spellcheck = false;
  area.setAttribute("aria-label", "The prompt, to copy by hand");
  return [line, area];
}

// The copy's outcome is drawn in `row` at once: a copy changes nothing on the server, so the map is not drawn again; later draws read it
// from `copied`.
async function copyPrompt(key: string, prompt: string, row: HTMLElement) {
  copied.set(key, overturnReport(await copyOverturn(prompt, navigator.clipboard?.writeText?.bind(navigator.clipboard)), prompt));
  for (const child of [...row.children]) if (child.className === "run-copied" || child.className === "ratify-prompt") row.removeChild(child);
  row.append(...noteNodes(key));
}

// "Explore this" (or Rebuild, or Try again) on `subject` of `project`: a run starts, or the server gives the prompt to copy, shown in `row`.
export function exploreSubject(project: string, key: string, subject: { kind: "flow"; id: string } | { kind: "figure"; pr: number; chapter: number; step: number }, row: HTMLElement) {
  invoke("micro_world_explore", { project, subject })
    .then((explored) => (explored.started || !explored.prompt ? reloadMap() : copyPrompt(key, explored.prompt, row)))
    .catch(failure("micro_world_explore", "error"));
}

// The row of world `key` on the map `worlds`, wired to the page's commands; `subject` is what the server builds it from.
export function exploreRow(project: string, worlds: MicroWorlds | undefined, key: string, name: string, subject: Parameters<typeof exploreSubject>[2]): HTMLElement {
  const explore = () => exploreSubject(project, key, subject, node);
  const node: HTMLElement = exploreNode(worlds, key, {
    explore,
    open: () => void openMicroWorld(project, key, name, worlds, explore),
    copy: (prompt) => void copyPrompt(key, prompt, node),
  });
  node.append(...noteNodes(key));
  return node;
}

registerLayer("micro-world", { open: () => shown !== null, close });
// Another project closes the overlay; otherwise its head is drawn again over the new payload, so "may be out of date" follows the map, and
// its frame stays.
onDraw((view) => {
  if (shown && (view.project !== shown.project || view.scrub)) shown = null;
  paint(view.state.micro_worlds);
});
