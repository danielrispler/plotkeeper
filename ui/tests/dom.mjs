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
    this.childNodes.push(...items.map(node));
  }
  replaceChildren(...items) {
    this.childNodes = items.map(node);
  }
  prepend(...items) {
    this.childNodes.unshift(...items.map(node));
  }
  appendChild(item) {
    this.childNodes.push(item);
  }
  insertBefore(item, ref) {
    const was = this.childNodes.indexOf(item);
    if (was >= 0) this.childNodes.splice(was, 1);
    const at = ref ? this.childNodes.indexOf(ref) : this.childNodes.length;
    this.childNodes.splice(at < 0 ? this.childNodes.length : at, 0, item);
  }
  removeChild(item) {
    this.childNodes.splice(this.childNodes.indexOf(item), 1);
  }
  replaceChild(item, old) {
    this.childNodes[this.childNodes.indexOf(old)] = item;
  }
  addEventListener(name, run) {
    this.listeners[name] = run;
  }
  removeEventListener(name) {
    delete this.listeners[name];
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
  focus() {}
  // Only a `.class` or a tag, as the views under test ask for.
  querySelectorAll(selector) {
    const out = [];
    const walk = (node) => {
      for (const c of node.childNodes) {
        if (!(c instanceof StandIn)) continue;
        if (selector.startsWith(".") ? c.className.split(" ").includes(selector.slice(1)) : c.tag === selector) out.push(c);
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

// The stand-ins as the tests see them: any, at this one boundary, since they stand in for the DOM's types without being them, so a
// test may hand one to the page's code typed with the real ones (#554).
/** @type {any} */
export const El = StandIn;
/** @type {any} */
export const Field = FieldStandIn;

// A document whose elements are `Element`s, for `globalThis.document`; `extra` adds or replaces members.
/** @returns {any} */
export const fakeDocument = (extra = {}, Element = El) => ({
  createElement: (tag) => new Element(tag),
  getElementById: () => new Element("div"),
  querySelector: () => null,
  activeElement: null,
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
