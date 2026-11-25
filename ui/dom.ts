// The window's small parts: the shell's calls, and the elements every region builds.

import { showError } from "./notice.js";
import type { Args, Command, Reply, Step } from "./types.js";

type None = Record<string, never>;

declare global {
  interface Window {
    __TAURI__: {
      core: { invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> };
      log: { warn(message: string): Promise<void>; error(message: string): Promise<void> };
      event: { listen<T>(event: string, handler: (event: { payload: T }) => void): Promise<() => void> };
    };
  }
}

// Only a command of the map, with its arguments' and its reply's types from there (#144, #554); one with no arguments takes none.
export const invoke = <C extends Command>(command: C, ...args: Args<C> extends None ? [] : [args: Args<C>]) =>
  window.__TAURI__.core.invoke<Reply<C>>(command, args[0]);
export const listen = <T>(event: string, handler: (payload: T) => void) =>
  window.__TAURI__.event.listen<T>(event, (e) => handler(e.payload));

// Tells the newest of a series of requests: the promise it returns for a request resolves to `undefined` once a later request was
// made, so a slow, older reply never lands over a newer one.
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
// An `error` also shows in the notice line (#191), whole; the server does not send the UI's own lines back as `server-error`.
export function logLine(level: "warn" | "error", line: string) {
  if (level === "error") {
    showError(line);
  }
  try {
    window.__TAURI__.log[level](kept(line)).catch(() => {});
  } catch {}
}
export const failure = (command: string, level: "warn" | "error" = "warn") => (e: unknown) => logLine(level, `${command}: ${messageOf(e)}`);

// An uncaught error or unhandled rejection in the page goes to the log at `error`.
export function logUncaught(target: Pick<Window, "addEventListener">) {
  target.addEventListener("error", (event) => logLine("error", `uncaught: ${event.message}`));
  target.addEventListener("unhandledrejection", (event) => logLine("error", `unhandled rejection: ${messageOf(event.reason)}`));
}

// The page's element `id`, which `index.html` holds; with `type`, checked to be one, so a caller needs no cast. A missing or mistyped
// element is a broken page, and fails here by its id.
export function element(id: string): HTMLElement;
export function element<T extends HTMLElement>(id: string, type: abstract new () => T): T;
export function element(id: string, type?: abstract new () => HTMLElement): HTMLElement {
  const found = document.getElementById(id);
  if (!found) throw new Error(`#${id} is not on the page`);
  const kind = found.constructor.name;
  if (type && !(found instanceof type)) throw new Error(`#${id} is ${kind}, not ${type.name}`);
  return found;
}

// A question before a button destroys something (#459): the dialog's heading, what is lost, and the confirm button's words, which repeat
// the button's verb.
export type Confirm = { heading: string; lost: string; verb: string };
// Opens the confirm dialog with Cancel focused, as Delete's is (`story.ts`), and resolves to true only when Daniel confirms; Cancel, Esc
// or any other close resolve to false. The answer comes from the button that submitted the form and from the dialog's cancel, which fire
// with the press, not from its close event, which Chrome fires at the next frame and snap's Chrome never draws. Since it comes a frame
// late, a close that finds the dialog open is an earlier ask's, after the dialog was opened again; it leaves this ask waiting.
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

/** An SVG element `tag` with its attributes. */
export function svg(tag: string, attrs: Record<string, string> = {}): SVGElement {
  const node = document.createElementNS("http://www.w3.org/2000/svg", tag);
  for (const [key, value] of Object.entries(attrs)) node.setAttribute(key, value);
  return node;
}

// A glyph drawn for the eye alone, such as ☰ or ✗, hidden from assistive tech (#456); its button or row is named in words.
export function glyphOnly(glyph: string, className = ""): HTMLElement {
  const node = text("span", glyph, className);
  node.setAttribute("aria-hidden", "true");
  return node;
}

// Words for assistive tech alone, not drawn (#456), such as "failed" beside a ✕.
export const unseenWords = (words: string): HTMLElement => text("span", words, "visually-hidden");

// Enter in `field` runs `run`, except the Enter that confirms an input method's word (#461); Safari's out-of-order confirming key
// down reports keyCode 229 with isComposing false.
export function onEnter(field: HTMLElement, run: () => void) {
  field.addEventListener("keydown", (event) => {
    if (event.key !== "Enter") return;
    event.preventDefault();
    if (!event.isComposing && event.keyCode !== 229) run();
  });
}

// The prompt in a read-only box, to copy by hand when the clipboard refused it.
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

// A `<pre>` that scrolls on its own, so it takes focus, named `name`, for the keys to scroll it (#462).
export function scrollingPre(words: string, className: string, name: string): HTMLElement {
  const pre = text("pre", words, className);
  pre.setAttribute("tabindex", "0");
  pre.setAttribute("aria-label", name);
  return pre;
}

// An empty polite live region, drawn for assistive tech alone (#456): made before its words, so each change of them is heard.
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

// `node` with `full` as its native hover title: the full text of what the layout cuts (#162). No title for an empty text,
// which would hide the tooltip of the element under it.
export function fullTitle<T extends HTMLElement>(node: T, full: string | null | undefined): T {
  if (full) node.title = full;
  return node;
}

export const stepText = (step: Step | null) => (step ? `${step.tool} ${step.detail ?? ""}` : "No step yet");

// The time from `from` to `to`, such as "3m".
export function age(from: number, to = Date.now()): string {
  const minutes = Math.floor((to - from) / 60_000);
  return minutes < 60 ? `${minutes}m` : `${Math.floor(minutes / 60)}h`;
}

// A button: the one button component (danielrispler/Falinks#566), its type, pressed state and click. Only a real toggle has a pressed
// state; any other button, a disclosure among them (which says `aria-expanded`), passes null and has no `aria-pressed` (#456).
export function pressable<T extends HTMLElement>(button: T, pressed: boolean | null, click: (event: MouseEvent) => void): T {
  button.classList.add("btn");
  button.setAttribute("type", "button");
  if (pressed !== null) button.setAttribute("aria-pressed", String(pressed));
  button.addEventListener("click", click);
  return button;
}

// Makes `parent`'s children match `next`, keeping each node that stays, so the focused element and
// the text selection survive an update (danielrispler/Falinks#570). A node stays when its tag and `data-key` match, and
// takes the new one's attributes and text. A keyed node is found by its key wherever it was, so rows added or dropped before it
// do not replace it; an unkeyed one is matched by its place. Give a node with a listener a key that pins what the
// listener does, such as its session's id.
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

// Code, left to right, in its own isolate.
export function code(text: string): HTMLElement {
  const node = document.createElement("code");
  node.dir = "ltr";
  node.textContent = text;
  return node;
}

// The writer's text, whose code is in backticks. It sets its own direction, so a card stored in Hebrew
// still reads right to left; each code span is a left-to-right isolate, so `a <= b` is not mirrored.
export function prose(tag: string, text: string): HTMLElement {
  const node = document.createElement(tag);
  node.dir = "auto";
  node.append(...text.split("`").map((part, i) => (i % 2 ? code(part) : part)));
  return node;
}
