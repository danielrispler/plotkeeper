// The page's small parts: the server's commands, events and log, and the elements every region builds.

import { showError } from "./notice.js";
import type { Args, Command, Reply, Step } from "./types.js";

type None = Record<string, never>;

// The seam `transport.ts` installs, read at each call so a test may stand in for it. This is the one place outside `transport.ts` that
// names it.
const seam = () => window.__TAURI__;

/** Only a command of the map, with its arguments' and its reply's types from there (#144, #554); one with no arguments takes none. */
export const invoke = <C extends Command>(command: C, ...args: Args<C> extends None ? [] : [args: Args<C>]) =>
  seam().core.invoke<Reply<C>>(command, args[0]);
export const listen = <T>(event: string, handler: (payload: T) => void) =>
  seam().event.listen<T>(event, (e) => handler(e.payload));

/**
 * Tells the newest of a series of requests: the promise it returns for a request resolves to `undefined` once a later request was
 * made, so a slow, older reply never lands over a newer one.
 */
export function newest() {
  let latest = 0;
  return <T>(request: Promise<T>) => {
    const mine = ++latest;
    return request.then((value): T | undefined => (mine === latest ? value : undefined));
  };
}

// The Studio log (#190): the line is the command's name and the error's message, nothing else of the error. `error` is for a
// failed action Daniel asked for, `warn` for a call that degrades and carries on. A log that fails is dropped.
/** The words of a caught error: an `Error`'s message, such as a refused command's reason, else the value as text. */
export const messageOf = (e: unknown): string => (e instanceof Error ? e.message : String(e));
// What of a line may reach the log, as Rust's `scrubbed` keeps it: one line, up to the first quote mark, so a writer's words never do.
const kept = (line: string) =>
  (line.replace(/\s*\n\s*/g, " ").split('"')[0] ?? "").replace(/[: ]+$/, "");
/** An `error` also shows in the notice line (#191), whole; the server does not send the UI's own lines back as `server-error`. */
export function logLine(level: "warn" | "error", line: string) {
  if (level === "error") {
    showError(line);
  }
  try {
    seam().log[level](kept(line)).catch(() => {});
  } catch {}
}
export const failure = (command: string, level: "warn" | "error" = "warn") => (e: unknown) => logLine(level, `${command}: ${messageOf(e)}`);

/** An uncaught error or unhandled rejection in the page goes to the log at `error`. */
export function logUncaught(target: Pick<Window, "addEventListener">) {
  target.addEventListener("error", (event) => logLine("error", `uncaught: ${event.message}`));
  target.addEventListener("unhandledrejection", (event) => logLine("error", `unhandled rejection: ${messageOf(event.reason)}`));
}

/**
 * The page's element `id`, which `index.html` holds; with `type`, checked to be one, so a caller needs no cast. A missing or mistyped
 * element is a broken page, and fails here by its id.
 */
export function element(id: string): HTMLElement;
export function element<T extends HTMLElement>(id: string, type: abstract new () => T): T;
export function element(id: string, type?: abstract new () => HTMLElement): HTMLElement {
  const found = document.getElementById(id);
  if (!found) throw new Error(`#${id} is not on the page`);
  const kind = found.constructor.name;
  if (type && !(found instanceof type)) throw new Error(`#${id} is ${kind}, not ${type.name}`);
  return found;
}

/**
 * A question before a button destroys something (#459): the dialog's heading, what is lost, and the confirm button's words, which repeat
 * the button's verb.
 */
export type Confirm = { heading: string; lost: string; verb: string };
/**
 * Opens the confirm dialog with Cancel focused, as Delete's is (`story.ts`), and resolves to true only when Daniel confirms; Cancel, Esc
 * or any other close resolve to false. The answer comes from the button that submitted the form and from the dialog's cancel, which fire
 * with the press, not from its close event, which Chrome fires at the next frame and snap's Chrome never draws. Since it comes a frame
 * late, a close that finds the dialog open is an earlier ask's, after the dialog was opened again; it leaves this ask waiting.
 */
export function confirmed(ask: Confirm): Promise<boolean> {
  const dialog = element("confirm-dialog", HTMLDialogElement);
  element("confirm-heading").textContent = ask.heading;
  element("confirm-lost").textContent = ask.lost;
  element("confirm-yes").textContent = ask.verb;
  const form = dialog.querySelector("form");
  if (!form) throw new Error("#confirm-dialog has no form");
  return new Promise((resolve) => {
    form.onsubmit = (event) => resolve((event.submitter as HTMLButtonElement | null)?.value === "confirm");
    dialog.oncancel = () => resolve(false);
    dialog.onclose = () => {
      if (!dialog.open) resolve(false);
    };
    dialog.showModal();
  });
}

export function text<K extends keyof HTMLElementTagNameMap>(tag: K, content: string, className = ""): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  node.className = className;
  node.textContent = content;
  return node;
}

/**
 * The page's own storage under `key`: a per-browser convenience, read and written in a try, so a browser that refuses it (blocked site
 * data, a private window) reads none and keeps nothing.
 */
export const pageStore = (key: string) => ({
  get(): string | null {
    try {
      return localStorage.getItem(key);
    } catch {
      return null;
    }
  },
  set(value: string) {
    try {
      localStorage.setItem(key, value);
    } catch {}
  },
  remove() {
    try {
      localStorage.removeItem(key);
    } catch {}
  },
});

/** Drops every entry of `cache` whose key `keep` refuses: a page cache pruned to what is still listed (spec #548, Group 4). */
export function keepOnly<K>(cache: { keys(): Iterable<K>; delete(key: K): unknown }, keep: (key: K) => boolean) {
  for (const key of [...cache.keys()]) if (!keep(key)) cache.delete(key);
}

/** An SVG element `tag` with its attributes. */
export function svg(tag: string, attrs: Record<string, string> = {}): SVGElement {
  const node = document.createElementNS("http://www.w3.org/2000/svg", tag);
  for (const [key, value] of Object.entries(attrs)) node.setAttribute(key, value);
  return node;
}

/** A glyph drawn for the eye alone, such as ☰ or ✗, hidden from assistive tech (#456); its button or row is named in words. */
export function glyphOnly(glyph: string, className = ""): HTMLElement {
  const node = text("span", glyph, className);
  node.setAttribute("aria-hidden", "true");
  return node;
}

/** Words for assistive tech alone, not drawn (#456), such as "failed" beside a ✕. */
export const unseenWords = (words: string): HTMLElement => text("span", words, "visually-hidden");

/**
 * Enter in `field` runs `run`, except the Enter that confirms an input method's word (#461); Safari's out-of-order confirming key
 * down reports keyCode 229 with isComposing false.
 */
export function onEnter(field: HTMLElement, run: () => void) {
  field.addEventListener("keydown", (event) => {
    if (event.key !== "Enter") return;
    event.preventDefault();
    if (!event.isComposing && event.keyCode !== 229) run();
  });
}

/** The prompt in a read-only box, to copy by hand when the clipboard refused it. */
export function promptBox(prompt: string, rows = 4): HTMLTextAreaElement {
  const area = document.createElement("textarea");
  area.className = "ratify-prompt";
  area.value = prompt;
  area.readOnly = true;
  area.rows = rows;
  area.name = "prompt";
  area.autocomplete = "off";
  area.spellcheck = false;
  area.setAttribute("aria-label", "The prompt, to copy by hand");
  return area;
}

/** A `<pre>` that scrolls on its own, so it takes focus, named `name`, for the keys to scroll it (#462). */
export function scrollingPre(words: string, className: string, name: string): HTMLElement {
  const pre = text("pre", words, className);
  pre.setAttribute("tabindex", "0");
  pre.setAttribute("aria-label", name);
  return pre;
}

/** An empty polite live region, drawn for assistive tech alone (#456): made before its words, so each change of them is heard. */
export function statusRegion(): HTMLElement {
  const live = text("p", "", "visually-hidden");
  live.setAttribute("role", "status");
  return live;
}

// `box`'s children become `nodes`, then its one status region saying `words` (#452's review). The region is made once and never leaves
// the box while the rest is drawn afresh, so a redraw that keeps the words says nothing again. A region the box did not hold yet takes
// its words a moment after it is drawn, once a screen reader knows it is there.
const regions = new WeakMap<HTMLElement, HTMLElement>();
export function drawWithStatus(box: HTMLElement, nodes: Node[], words: string) {
  const live = regions.get(box) ?? statusRegion();
  regions.set(box, live);
  const fresh = ![...box.childNodes].includes(live);
  for (const child of [...box.childNodes]) if (child !== live) box.removeChild(child);
  // A region the box dropped, as a closed overlay's emptied box does, comes back empty, so its words are heard again.
  if (fresh) {
    live.textContent = "";
    box.appendChild(live);
  }
  for (const node of nodes) box.insertBefore(node, live);
  live.dataset.words = words;
  const say = () => {
    if (live.textContent !== live.dataset.words) live.textContent = live.dataset.words ?? "";
  };
  if (fresh && words) setTimeout(say, 100);
  else say();
}

// The page's one status region for what a click just did (#465): a copied prompt, an overturn. Lines drawn again with every map draw,
// such as a run's or a world's copy note, are not live; their words are said here, once, by the click. The region is put in the body at
// its first use and never moved or drawn again, so a draw says nothing. It is emptied first and takes the words a moment later, so the
// same words said by a second click are heard again, and a region the page did not hold yet is known before its words come.
// A second click within that moment drops the first one's words, so only the last are said.
let actionRegion: HTMLElement | null = null;
let actionTimer: ReturnType<typeof setTimeout> | undefined;
export function sayAction(words: string) {
  if (!actionRegion) {
    actionRegion = statusRegion();
    document.body.append(actionRegion);
  }
  const live = actionRegion;
  live.textContent = "";
  clearTimeout(actionTimer);
  actionTimer = setTimeout(() => (live.textContent = words), 100);
}

/**
 * `node` as cut text that scrolls in place (#582): its text stays whole in the page, in an inner box the `cut` class cuts with an ellipsis
 * and moves on hover or focus, only when it overflows (`style.css`). With `words`, they replace what the node held, as for a node the
 * page draws once and fills again.
 */
export function cut<T extends HTMLElement>(node: T, words?: string): T {
  if (!/(^| )cut( |$)/.test(node.className)) node.className = `${node.className} cut`.trim();
  const inner = document.createElement("span");
  if (words === undefined) inner.append(...node.childNodes);
  else inner.textContent = words;
  node.replaceChildren(inner);
  return node;
}

/** `node` with `key` as its `data-key`, the name its hint and `patch` know it by; returns `node`. */
export function keyed<T extends HTMLElement>(node: T, key: string): T {
  node.setAttribute("data-key", key);
  return node;
}

// The page's hints (spec #582): each trigger's own `popover="hint"`, kept in `#hints` by id, so a trigger drawn again, as the list's rows
// are on every update, finds its hint and does not make another. A hint whose trigger the page dropped for good goes once the trigger is collected.
const hints = new Map<string, HTMLElement>();
// A trigger kept off the page (a box a view keeps and puts back) keeps its hint: only a trigger the page has dropped for good loses it.
const owners = new Map<string, WeakRef<Element>>();
let loose = 0;
let pruneQueued = false;
const hintId = (key: string) => `hint-${key.replace(/[^\w-]/g, (c) => `_${c.charCodeAt(0).toString(16)}`)}`;
const squeezed = (words: string | null) => (words ?? "").replace(/\s+/g, " ").trim();

// The hint `trigger` names now: its `interestfor` or its `data-hint`.
const named = (trigger: Element) => trigger.getAttribute("interestfor") ?? trigger.getAttribute("data-hint");

// A hint stays while a trigger on the page names it, or while its trigger, kept off the page, still does. A trigger drawn again with
// other words, as an unkeyed one is, names a new hint, so the old one goes.
function pruneHints() {
  pruneQueued = false;
  if (!document.querySelectorAll) return;
  const wired = new Set([...document.querySelectorAll("[interestfor], [data-hint]")].map(named));
  for (const [id, node] of hints) {
    const owner = owners.get(id)?.deref();
    if (wired.has(id) || (owner && named(owner) === id)) continue;
    node.remove();
    owners.delete(id);
    hints.delete(id);
  }
}
function queuePrune() {
  if (pruneQueued) return;
  pruneQueued = true;
  setTimeout(pruneHints, 300);
}

// The listener path, for what `interestfor` cannot reach (a summary, an input, a select, and the marks of ticket 4): the trigger's own
// pointer-enter/leave and focus/blur open and close its hint, with `showPopover({ source })` so the trigger is its implicit anchor, after
// the page's `--hint-delay` (0.5 s) and Chromium's end delay (0.15 s). The pointer may rest on the hint. Esc and light dismiss are the
// popover's own. A trigger a neighbour's hint is open beside, one sharing its parent, shows at once, as the CSS does for the other path.
// The trigger's listeners open the hint its `data-hint` names when they run, so a trigger `patch` kept, which took a new draw's
// `data-hint`, opens that draw's hint; a hint's own listeners act for the trigger that opened it, so no hint holds a trigger the page dropped.
const listened = new Set(["SUMMARY", "INPUT", "SELECT", "TEXTAREA"]);
// What `interestfor` reaches: any other trigger (a mark, a span with words) is pointer-only, or by focus too when it has a tabindex.
const interested = new Set(["BUTTON", "A", "AREA"]);
// Whether the pointer (on the trigger or its open hint) or the focus holds a trigger's hint, and its pending open or close.
type Interest = { pointer: boolean; focus: boolean; timer?: ReturnType<typeof setTimeout> };
const interests = new WeakMap<Element, Interest>();
const HINT_END_DELAY = 150;
// The one open hint of the listener path, and the trigger that opened it.
let openTrigger: Element | null = null;
let openHint: HTMLElement | null = null;

// An SVG shape cannot be an anchor, nor the `source` of `showPopover`: its hint anchors to this box, an invisible HTML one laid over
// the shape, a single one since only one hint is open at a time.
let anchorBox: HTMLElement | null = null;
function anchorOver(shape: Element): HTMLElement {
  if (!anchorBox) {
    anchorBox = document.createElement("div");
    anchorBox.className = "hint-anchor";
    element("hints").append(anchorBox);
  }
  const at = shape.getBoundingClientRect();
  Object.assign(anchorBox.style, { left: `${at.left}px`, top: `${at.top}px`, width: `${at.width}px`, height: `${at.height}px` });
  return anchorBox;
}
const isSvg = (node: Element) => "namespaceURI" in node && node.namespaceURI === "http://www.w3.org/2000/svg";

function startDelay(): number {
  const read = typeof getComputedStyle === "function" ? Number.parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--hint-delay")) : Number.NaN;
  return Number.isFinite(read) ? read * 1000 : 500;
}

const hintNamed = (trigger: Element) => hints.get(trigger.getAttribute("data-hint") ?? "") ?? null;

function showFor(trigger: Element, interest: Interest) {
  const node = hintNamed(trigger);
  if (!node || !(interest.pointer || interest.focus) || node.matches(":popover-open")) return;
  node.showPopover({ source: isSvg(trigger) ? anchorOver(trigger) : (trigger as HTMLElement) });
  openTrigger = trigger;
  openHint = node;
}
function hideFor(trigger: Element, interest: Interest) {
  if (interest.pointer || interest.focus || openTrigger !== trigger) return;
  if (openHint?.matches(":popover-open")) openHint.hidePopover();
  openTrigger = openHint = null;
}
function interestIn(trigger: Element, interest: Interest) {
  clearTimeout(interest.timer);
  const beside = openTrigger !== null && openTrigger !== trigger && openTrigger.isConnected !== false && openTrigger.parentElement === trigger.parentElement;
  interest.timer = setTimeout(() => showFor(trigger, interest), beside ? 0 : startDelay());
}
function loseInterest(trigger: Element, interest: Interest) {
  clearTimeout(interest.timer);
  interest.timer = setTimeout(() => hideFor(trigger, interest), HINT_END_DELAY);
}

// A hint's own listeners, added once when it is made: the pointer resting on it holds it open for the trigger that opened it.
function listenOnHint(node: HTMLElement) {
  const opener = () => (openHint === node && openTrigger ? openTrigger : null);
  node.addEventListener("pointerenter", () => {
    const trigger = opener();
    const interest = trigger && interests.get(trigger);
    if (!interest) return;
    interest.pointer = true;
    clearTimeout(interest.timer);
  });
  node.addEventListener("pointerleave", () => {
    const trigger = opener();
    const interest = trigger && interests.get(trigger);
    if (!interest) return;
    interest.pointer = false;
    loseInterest(trigger, interest);
  });
  node.addEventListener("toggle", (event) => {
    if ((event as ToggleEvent).newState === "closed" && openHint === node) openTrigger = openHint = null;
  });
}

/**
 * Wires `trigger` to open and close the hint its `data-hint` names, by pointer and, when `keyboard`, by focus. A mark that stays out of
 * the Tab order passes false: it is pointer-only. The trigger's listeners are added once.
 */
function listenForHint(trigger: Element, keyboard: boolean) {
  if (interests.has(trigger)) return;
  const interest: Interest = { pointer: false, focus: false };
  interests.set(trigger, interest);
  trigger.addEventListener("pointerenter", () => {
    interest.pointer = true;
    interestIn(trigger, interest);
  });
  trigger.addEventListener("pointerleave", () => {
    interest.pointer = false;
    loseInterest(trigger, interest);
  });
  if (!keyboard) return;
  trigger.addEventListener("focus", () => {
    if (!trigger.matches(":focus-visible")) return;
    interest.focus = true;
    interestIn(trigger, interest);
  });
  trigger.addEventListener("blur", () => {
    interest.focus = false;
    loseInterest(trigger, interest);
  });
}

/**
 * `trigger`, an enabled `<button>` or `<a>`, or a `<summary>`, `<input>` or `<select>` (the listener path), with its own hint of `words`, the one hover of the page: it shows on hover and on focus,
 * closes on Esc and sits beside its trigger (`interestfor`, `popover="hint"`; `style.css`). The words are plain text, known when the
 * trigger is drawn. A hint that adds to the trigger's name (its `aria-label`, else its text) is its description: `role="tooltip"` on the
 * hint and `aria-describedby` on the trigger, unless the author set one; a hint that only repeats the name adds nothing, as it would be
 * read twice. Any `title` goes. The trigger's `data-key` or `id` pins its hint across draws; give a trigger one. A trigger this has
 * already wired keeps its hint when drawn again.
 */
export function hint<T extends Element>(trigger: T, words: string): T {
  const id = named(trigger) ?? hintId(trigger.getAttribute("data-key") ?? trigger.getAttribute("id") ?? `n${++loose}`);
  let node = hints.get(id);
  if (!node) {
    node = document.createElement("div");
    node.setAttribute("id", id);
    node.setAttribute("popover", "hint");
    node.className = "hint";
    hints.set(id, node);
    listenOnHint(node);
    element("hints").append(node);
  }
  node.textContent = words;
  owners.set(id, new WeakRef(trigger));
  const adds = squeezed(words) !== squeezed(trigger.getAttribute("aria-label") ?? trigger.textContent);
  if (adds) node.setAttribute("role", "tooltip");
  else node.removeAttribute("role");
  trigger.removeAttribute("title");
  if (interested.has(trigger.nodeName)) trigger.setAttribute("interestfor", id);
  else {
    trigger.setAttribute("data-hint", id);
    listenForHint(trigger, listened.has(trigger.nodeName) || trigger.hasAttribute("tabindex"));
  }
  const described = trigger.getAttribute("aria-describedby");
  if (adds && (described === null || described === id)) trigger.setAttribute("aria-describedby", id);
  else if (!adds && described === id) trigger.removeAttribute("aria-describedby");
  queuePrune();
  return trigger;
}

/**
 * `node`, a mark or a dot that explains itself (a state's shape, a law's mark, an agent's logo): `role="img"` named by `words`, with
 * `words` as its hint on the pointer alone (#582). It stays out of the Tab order, so what it says must also reach the keyboard some
 * other way, in the part sheet's law list or the row's own words. Give it a `data-key` or an `id`, so its hint is the same on every draw.
 */
export function mark<T extends Element>(node: T, words: string): T {
  node.setAttribute("role", "img");
  node.setAttribute("aria-label", words);
  return hint(node, words);
}

/**
 * `button` available, with `words` as its hint, or, given `why`, unavailable: `aria-disabled="true"` instead of `disabled`, so it
 * stays focusable and its hint says why (#582). Its handlers check `isUnavailable` first, as a click or Enter still reaches them.
 */
export function availability(button: HTMLElement, words: string, why: string | null = null) {
  if (why === null) button.removeAttribute("aria-disabled");
  else button.setAttribute("aria-disabled", "true");
  hint(button, why ?? words);
}

export const isUnavailable = (button: HTMLElement) => button.getAttribute("aria-disabled") === "true";

/** `trigger`'s hint, for a test to read, or null when it has none. */
export const hintOf = (trigger: HTMLElement): HTMLElement | null => hints.get(trigger.getAttribute("interestfor") ?? trigger.getAttribute("data-hint") ?? "") ?? null;

export const stepText = (step: Step | null) => (step ? `${step.tool} ${step.detail ?? ""}` : "No step yet");

/** The time from `from` to `to`, such as "3m". */
export function age(from: number, to = Date.now()): string {
  const minutes = Math.floor((to - from) / 60_000);
  return minutes < 60 ? `${minutes}m` : `${Math.floor(minutes / 60)}h`;
}

/**
 * A button: the one button component (danielrispler/Falinks#566), its type, pressed state and click. Only a real toggle has a pressed
 * state; any other button, a disclosure among them (which says `aria-expanded`), passes null and has no `aria-pressed` (#456). A
 * button whose caller wires its own listeners passes no `click`.
 */
export function pressable<T extends HTMLElement>(button: T, pressed: boolean | null, click?: (event: MouseEvent) => void): T {
  button.classList.add("btn");
  button.setAttribute("type", "button");
  if (pressed !== null) button.setAttribute("aria-pressed", String(pressed));
  if (click) button.addEventListener("click", click);
  return button;
}

/**
 * Makes `parent`'s children match `next`, keeping each node that stays, so the focused element and
 * the text selection survive an update (danielrispler/Falinks#570). A node stays when its tag and `data-key` match, and
 * takes the new one's attributes and text. A keyed node is found by its key wherever it was, so rows added or dropped before it
 * do not replace it; an unkeyed one is matched by its place. Give a node with a listener a key that pins what the
 * listener does, such as its session's id.
 */
export function patch(parent: Node, next: Node[]) {
  const old = [...parent.childNodes];
  const byKey = new Map<string, Node>();
  for (const node of old) {
    const k = key(node);
    if (k !== null && !byKey.has(k)) byKey.set(k, node);
  }
  const claimed = new Set<Node>();
  const targets = next.map((node, i) => {
    const k = key(node);
    const was = k === null ? old[i] : byKey.get(k);
    if (!was || claimed.has(was) || was.nodeName !== node.nodeName || key(was) !== k) return { node, was: null };
    claimed.add(was);
    return { node: was, was };
  });
  for (const node of old) if (!claimed.has(node)) parent.removeChild(node);
  targets.forEach(({ node }, i) => {
    const at = parent.childNodes[i];
    if (at === node) return;
    if (at) parent.insertBefore(node, at);
    else parent.appendChild(node);
  });
  targets.forEach(({ was }, i) => {
    if (!was) return;
    const to = next[i];
    if (!to) return;
    if (was.nodeType !== 1) {
      if (was.nodeValue !== to.nodeValue) was.nodeValue = to.nodeValue;
      return;
    }
    const [from, goal] = [was as Element, to as Element];
    for (const { name, value } of [...goal.attributes]) if (from.getAttribute(name) !== value) from.setAttribute(name, value);
    for (const { name } of [...from.attributes]) if (!goal.hasAttribute(name)) from.removeAttribute(name);
    patch(from, [...goal.childNodes]);
  });
}
const key = (node: Node) => (node.nodeType === 1 ? (node as Element).getAttribute("data-key") : null);

/** Code, left to right, in its own isolate. */
export function code(text: string): HTMLElement {
  const node = document.createElement("code");
  node.dir = "ltr";
  node.textContent = text;
  return node;
}

/**
 * The writer's text, whose code is in backticks. It sets its own direction, so a card stored in Hebrew
 * still reads right to left; each code span is a left-to-right isolate, so `a <= b` is not mirrored.
 */
export function prose(tag: string, text: string): HTMLElement {
  const node = document.createElement(tag);
  node.dir = "auto";
  node.append(...text.split("`").map((part, i) => (i % 2 ? code(part) : part)));
  return node;
}
