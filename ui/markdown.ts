// Responses render as Markdown (#525, decision 21): tables, lists, code and links. The text is
// untrusted and the page can call the shell, so raw HTML stays text, only `http(s)` and file links
// (#557) become links, and an image shows its alt text. marked loads as a plain script from `vendor/`.

import { invoke } from "./dom.js";
import { codePath, fileTarget, openFile } from "./paths.js";

declare const marked: typeof import("marked");

const entities: Record<string, string> = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" };
const escape = (text: string) => text.replace(/[&<>"']/g, (c) => entities[c]);

// Built at first use, so modules that import this one load where marked is not (the UI tests).
const create = () =>
  new marked.Marked({
    gfm: true,
    renderer: {
      html: ({ text, block }) => (block ? `<p>${escape(text)}</p>` : escape(text)),
      image: ({ text }) => escape(text),
      // Inline code that is a whole file path links to the editor (#581).
      codespan({ text }) {
        const code = `<code>${escape(text)}</code>`;
        return codePath(text) ? `<a href="${escape(text)}" data-file>${code}</a>` : code;
      },
      link({ href, tokens }) {
        const inner = this.parser.parseInline(tokens);
        if (/^https?:\/\//i.test(href)) return `<a href="${escape(href)}">${inner}</a>`;
        return !href.startsWith("#") && fileTarget(href) ? `<a href="${escape(href)}" data-file>${inner}</a>` : inner;
      },
    },
  });
let renderer: ReturnType<typeof create> | undefined;

// `text` as safe HTML.
export const markdown = (text: string) => (renderer ??= create()).parse(text, { async: false });

// An element showing `text` rendered; a link click opens in the browser through the shell, a file
// link in Daniel's editor, a relative one from `cwd`, the folder of the session that wrote it.
export function rendered(tag: string, text: string, className = "", cwd: string | null = null): HTMLElement {
  const node = document.createElement(tag);
  node.className = `markdown ${className}`.trim();
  node.dir = "auto";
  node.innerHTML = markdown(text);
  node.addEventListener("click", (event) => {
    const link = (event.target as Element).closest("a");
    if (!link) return;
    event.preventDefault();
    // marked percent-encodes a link's path; a `file://` URL is decoded with its host dropped.
    const href = link.getAttribute("href") ?? "";
    if (link.hasAttribute("data-file")) openFile(href.startsWith("file:") ? href : decodeURI(href), cwd);
    else invoke("open_link", { target: link.href }).catch((e) => console.warn(e));
  });
  return node;
}
