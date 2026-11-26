// One small stand-in DOM for the node tests: element and text nodes that keep their identity and children, so a test can
// assert that a node is the same object after an update. Not a browser: it holds what the tests read and the calls the code makes.

class TextNode {
  nodeType = 3;
  nodeName = "#text";
  constructor(value) {
    this.nodeValue = value;
  }
  get text() {
    return this.nodeValue;
  }
}

const node = (item) => (typeof item === "string" ? new TextNode(item) : item);

// Whether `el` fits `selector`: a comma list of compounds, each a tag or `*`, then any `.class`, `[name]`, `[name="value"]` and
// `:not(compound)`. No combinators: what the page's code under test asks of one node.
const fits = (el, selector) => splitList(selector).some((one) => fitsCompound(el, one.trim()));
const splitList = (selector) => {
  const out = [""];
  let depth = 0;
  for (const c of selector) {
    if (c === "(") depth++;
    if (c === ")") depth--;
    if (c === "," && depth === 0) out.push("");
    else out[out.length - 1] += c;
  }
  return out;
};
function fitsCompound(el, compound) {
  let rest = compound;
  const tag = /^[a-zA-Z*][\w-]*/.exec(rest)?.[0];
  if (tag) {
    if (tag !== "*" && el.tag !== tag.toLowerCase()) return false;
    rest = rest.slice(tag.length);
  }
  // Each simple selector: its pattern, and whether a node fits it given the pattern's match.
  const simple = [
    { pattern: /^\.([\w-]+)/, test: (m) => el.className.split(" ").includes(m[1]) },
    { pattern: /^\[([\w-]+)(?:="([^"]*)")?\]/, test: (m) => (m[2] === undefined ? el.hasAttribute(m[1]) : el.getAttribute(m[1]) === m[2]) },
    { pattern: /^:not\(((?:[^()]|\([^()]*\))*)\)/, test: (m) => !fits(el, m[1]) },
  ];
  while (rest) {
    const found = simple.map(({ pattern, test }) => ({ m: pattern.exec(rest), test })).find(({ m }) => m);
    if (!found?.m) throw new Error(`the stand-in DOM does not read the selector ${JSON.stringify(compound)}`);
    const { m, test } = found;
    if (!test(m)) return false;
    rest = rest.slice(m[0].length);
  }
  return true;
}

// Which stand-in each node was put in last, for `remove()`; weak, so it holds no node alive.
const held = new WeakMap();
const hold = (parent, item) => {
  if (typeof item === "object" && item !== null) held.set(item, parent);
  return item;
};

class StandIn {
  nodeType = 1;
  attrs = new Map();
  listeners = {};
  dataset = {};
  style = {};
  classList = { add() {}, remove() {}, toggle() {}, contains: () => false };
  className = "";
  constructor(tag, attrs = {}, children = []) {
    this.tag = tag;
    this.nodeName = tag.toUpperCase();
    for (const [k, v] of Object.entries(attrs)) this.attrs.set(k, v);
    this.childNodes = children.map(node);
  }
  get children() {
    return this.childNodes;
  }
  get attributes() {
    return [...this.attrs].map(([name, value]) => ({ name, value }));
  }
  getAttribute(name) {
    return this.attrs.get(name) ?? null;
  }
  setAttribute(name, value) {
    this.attrs.set(name, value);
  }
  removeAttribute(name) {
    this.attrs.delete(name);
  }
  toggleAttribute() {}
  hasAttribute(name) {
    return this.attrs.has(name);
  }
  hasChildNodes() {
    return this.childNodes.length > 0;
  }
  append(...items) {
    this.childNodes.push(...items.map((item) => hold(this, node(item))));
  }
  replaceChildren(...items) {
    this.childNodes = items.map(node);
  }
  prepend(...items) {
    this.childNodes.unshift(...items.map((item) => hold(this, node(item))));
  }
  appendChild(item) {
    this.childNodes.push(hold(this, item));
  }
  // Takes the node out of whichever stand-in holds it: the one it was last put in by `append`, `appendChild`, `prepend` or
  // `insertBefore`.
  remove() {
    const from = held.get(this);
    if (from?.childNodes.includes(this)) from.removeChild(this);
  }
  insertBefore(item, ref) {
    const was = this.childNodes.indexOf(item);
    if (was >= 0) this.childNodes.splice(was, 1);
    const at = ref ? this.childNodes.indexOf(ref) : this.childNodes.length;
    this.childNodes.splice(at < 0 ? this.childNodes.length : at, 0, hold(this, item));
  }
  removeChild(item) {
    this.childNodes.splice(this.childNodes.indexOf(item), 1);
  }
  replaceChild(item, old) {
    this.childNodes[this.childNodes.indexOf(old)] = item;
  }
  // `listeners[name]` calls every listener added for `name`, in order, as a browser's dispatch does, and returns the last one's
  // result; `added[name]` holds them, so a test can count them.
  added = {};
  addEventListener(name, run) {
    if (!this.added[name]) this.added[name] = [];
    const all = this.added[name];
    all.push(run);
    this.listeners[name] ??= (...args) => {
      let result;
      for (const each of [...all]) result = each(...args);
      return result;
    };
  }
  removeEventListener(name, run) {
    const all = this.added[name] ?? [];
    if (all.includes(run)) all.splice(all.indexOf(run), 1);
    if (run === undefined || all.length === 0) {
      delete this.listeners[name];
      delete this.added[name];
    }
  }
  get textContent() {
    return this.text;
  }
  set textContent(value) {
    this.childNodes = [new TextNode(value)];
  }
  get text() {
    return this.childNodes.map((c) => c.text).join("");
  }
  // A popover's state, and the geometry and focus a toast's placing reads (all zero here).
  popoverOpen = false;
  source = null;
  showPopover(options) {
    this.popoverOpen = true;
    this.source = options?.source ?? null;
  }
  hidePopover() {
    this.popoverOpen = false;
  }
  matches(selector) {
    return selector === ":popover-open" && this.popoverOpen;
  }
  contains(other) {
    return other === this || this.childNodes.some((c) => c instanceof StandIn && c.contains(other));
  }
  getBoundingClientRect() {
    return { top: 0, right: 0, bottom: 0, left: 0, width: 0, height: 0 };
  }
  offsetWidth = 0;
  offsetHeight = 0;
  // How the node was last brought into view, null while it never was.
  scrolledIntoView = null;
  scrollIntoView(options) {
    this.scrolledIntoView = options ?? {};
  }
  focus() {}
  // Compound selectors only (`fits`), as the views under test ask for.
  querySelectorAll(selector) {
    const out = [];
    const walk = (node) => {
      for (const c of node.childNodes) {
        if (!(c instanceof StandIn)) continue;
        if (fits(c, selector)) out.push(c);
        walk(c);
      }
    };
    walk(this);
    return out;
  }
  querySelector(selector) {
    return this.querySelectorAll(selector)[0] ?? null;
  }
  find(tag, out = []) {
    if (this.tag === tag) out.push(this);
    for (const c of this.childNodes) if (c instanceof StandIn) c.find(tag, out);
    return out;
  }
}

// An element with the form-control fields a view reads and writes.
class FieldStandIn extends StandIn {
  title = "";
  disabled = false;
  dir = "";
}

// A stand-in that is in a page, as a browser's node is (spec #661): putting it somewhere takes it out of where it was, it is connected
// while a root (`connect`) holds it, and taking the focused node out of the page drops the focus (`document.activeElement`), as a
// browser does. A `<details>`'s `open` is its attribute. Opt-in, for the tests that read what a draw keeps in the page.
const roots = new WeakSet();
/** @returns {any} */
const page = () => globalThis.document;
class LiveStandIn extends FieldStandIn {
  // Its inline style writes its `style` attribute, as a browser's does, so `patch` copies a new draw's style onto a kept node.
  style = new Proxy(/** @type {Record<string, string>} */ ({}), {
    set: (props, name, value) => {
      props[/** @type {string} */ (name)] = value;
      this.setAttribute("style", Object.entries(props).map(([k, v]) => `${k}: ${v}`).join("; "));
      return true;
    },
  });
  get parentNode() {
    const from = held.get(this);
    return from?.childNodes.includes(this) ? from : null;
  }
  get isConnected() {
    if (roots.has(this)) return true;
    return this.parentNode?.isConnected ?? false;
  }
  get open() {
    return this.hasAttribute("open");
  }
  set open(value) {
    if (value) this.setAttribute("open", "");
    else this.removeAttribute("open");
  }
  focus() {
    if (this.isConnected) page().activeElement = this;
  }
  cloneNode() {
    const copy = new LiveStandIn(this.tag);
    for (const [k, v] of this.attrs) copy.attrs.set(k, v);
    copy.className = this.className;
    return copy;
  }
  #take(items) {
    for (const item of items) if (item instanceof LiveStandIn && item.parentNode) item.parentNode.removeChild(item);
  }
  append(...items) {
    this.#take(items);
    super.append(...items);
  }
  prepend(...items) {
    this.#take(items);
    super.prepend(...items);
  }
  appendChild(item) {
    this.#take([item]);
    super.appendChild(item);
  }
  insertBefore(item, ref) {
    if (item.parentNode !== this) this.#take([item]);
    super.insertBefore(item, ref);
  }
  replaceChild(item, old) {
    this.#take([item]);
    super.replaceChild(hold(this, item), old);
  }
  replaceChildren(...items) {
    for (const child of [...this.childNodes]) this.removeChild(child);
    this.append(...items);
  }
  removeChild(item) {
    const focused = page()?.activeElement;
    if (focused && item instanceof StandIn && item.contains(focused)) page().activeElement = null;
    super.removeChild(item);
  }
  set textContent(value) {
    for (const child of [...this.childNodes]) this.removeChild(child);
    super.textContent = value;
  }
  get textContent() {
    return super.textContent;
  }
}
/** `node`, a live stand-in, as the page's own: it and what it holds are connected. */
export const connect = (node) => {
  roots.add(node);
  return node;
};

// The stand-ins as the tests see them: any, at this one boundary, since they stand in for the DOM's types without being them, so a
// test may hand one to the page's code typed with the real ones (#554).
/** @type {any} */
export const El = StandIn;
/** @type {any} */
export const Field = FieldStandIn;
/** @type {any} */
export const Live = LiveStandIn;

// A document whose elements are `Element`s, for `globalThis.document`; `extra` adds or replaces members.
/** @returns {any} */
export const fakeDocument = (extra = {}, Element = El) => ({
  createElement: (tag) => new Element(tag),
  getElementById: () => new Element("div"),
  querySelector: () => null,
  activeElement: null,
  // The viewport's width, which the toasts are placed against (`toast.ts`).
  documentElement: { clientWidth: 0 },
  ...extra,
});

// Installs `window.__TAURI__` so that every `invoke` is recorded in `calls` as `[command, args]` and answers `reply(command, args)`.
/**
 * @param {unknown[][]} calls
 * @param {(command: string, args?: any) => unknown} [reply]
 */
export const tauri = (calls, reply = () => null) => {
  globalThis.window = /** @type {any} */ ({
    __TAURI__: {
      core: {
        // biome-ignore lint/suspicious/useAwait: a stand-in for an async API, so a throw in `reply` rejects as the real one's would
        invoke: async (command, args) => {
          calls.push([command, args]);
          return reply(command, args);
        },
      },
    },
  });
};
