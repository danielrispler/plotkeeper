// Responses render as Markdown (danielrispler/Falinks#525, decision 21): tables, lists, code and links. The text is
// untrusted and the page can call the server, so raw HTML stays text, only `http(s)` links
// become links, and an image shows its alt text. marked loads as a plain script from `vendor/`.

declare const marked: typeof import("marked");

const entities: Record<string, string> = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" };
const escapeHtml = (text: string) => text.replace(/[&<>"']/g, (c) => entities[c]);

// A link's hover names its target (#220): the page has no status bar to show it.
const opens = (target: string) => escapeHtml(`Open ${target} in a new tab`);

// Built at first use, so modules that import this one load where marked is not (the UI tests).
const create = () =>
  new marked.Marked({
    gfm: true,
    renderer: {
      html: ({ text, block }) => (block ? `<p>${escapeHtml(text)}</p>` : escapeHtml(text)),
      image: ({ text }) => escapeHtml(text),
      // An `http(s)` link opens in a new tab; any other link, a file's included, is its text: the page has no editor to open it in.
      link({ href, tokens }) {
        const inner = this.parser.parseInline(tokens);
        return /^https?:\/\//i.test(href) ? `<a href="${escapeHtml(href)}" target="_blank" rel="noopener noreferrer" title="${opens(href)}">${inner}</a>` : inner;
      },
    },
  });
let renderer: ReturnType<typeof create> | undefined;

// `text` as safe HTML.
export const markdown = (text: string) => (renderer ??= create()).parse(text, { async: false });

// An element showing `text` rendered; its links open in a new tab.
export function rendered(tag: string, text: string, className = ""): HTMLElement {
  const node = document.createElement(tag);
  node.className = `markdown ${className}`.trim();
  node.dir = "auto";
  node.innerHTML = markdown(text);
  return node;
}
