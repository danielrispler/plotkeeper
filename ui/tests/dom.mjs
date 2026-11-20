// One small stand-in DOM for the node tests: element and text nodes that keep their identity and children, so a test can
// assert that a node is the same object after an update. Not a browser: it holds what the tests read and the calls the code makes.

export class Text {
  nodeType = 3;
  nodeName = "#text";
  constructor(value) {
    this.nodeValue = value;
  }
  get text() {
    return this.nodeValue;
  }
}

const node = (item) => (typeof item === "string" ? new Text(item) : item);

export class El {
  nodeType = 1;
  attrs = new Map();
  listeners = {};
  classList = { add() {} };
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
  get textContent() {
    return this.text;
  }
  set textContent(value) {
    this.childNodes = [new Text(value)];
  }
  get text() {
    return this.childNodes.map((c) => c.text).join("");
  }
  find(tag, out = []) {
    if (this.tag === tag) out.push(this);
    for (const c of this.childNodes) if (c instanceof El) c.find(tag, out);
    return out;
  }
}
