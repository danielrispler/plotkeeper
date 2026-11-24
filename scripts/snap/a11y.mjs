// The accessibility pass (#456), checked in every snap page after its fixture's script, beside the hover rule: every `button` and `a`
// has an accessible name that is neither a title alone nor a lone glyph, every `[role=img]` and every `dialog` has a name, and no element
// is both pressed and expanded. A broken rule logs a `snap:fail` line.
//
// `nameSource` defines `window.__snapName(node)`, which fixtures may call too: the node's accessible name, worked out in the page by a
// subset of the W3C's name computation (aria-labelledby, aria-label, a field's <label>s, an SVG's <title>, then the text of what it
// holds less what is aria-hidden or hidden). A `title` is left out on purpose: the pass counts it as no name. It is not Chrome's own
// accessibility tree, which snap's headless Chrome only gives over DevTools.
export const nameSource = `
  window.__snapName = (node) => {
    const words = (text) => (text ?? "").replace(/\\s+/g, " ").trim();
    const silent = (each) => each.getAttribute("aria-hidden") === "true" || each.hidden;
    const held = (each) => {
      if (each.nodeType === Node.TEXT_NODE) return each.data;
      if (each.nodeType !== Node.ELEMENT_NODE || silent(each)) return "";
      const label = each.getAttribute("aria-label");
      if (words(label)) return " " + label + " ";
      return " " + [...each.childNodes].map(held).join("") + " ";
    };
    const by = node.getAttribute("aria-labelledby");
    if (by) return words(by.split(/\\s+/).map((id) => document.getElementById(id)).filter(Boolean).map((each) => [...each.childNodes].map(held).join("")).join(" "));
    if (words(node.getAttribute("aria-label"))) return words(node.getAttribute("aria-label"));
    if (node.labels?.length) return words([...node.labels].map((label) => [...label.childNodes].map(held).join("")).join(" "));
    if (node instanceof SVGElement) return words(node.querySelector(":scope > title")?.textContent);
    if (node.matches("dialog, [role=img], [role=dialog]")) return "";
    return words([...node.childNodes].map(held).join(""));
  };
`;

export const a11yAudit = `
  const said = (node) => node.tagName.toLowerCase() + (node.id ? "#" + node.id : "") + (typeof node.className === "string" && node.className ? "." + node.className.trim().split(/\\s+/).join(".") : "") + " " + JSON.stringify((node.textContent ?? "").replace(/\\s+/g, " ").trim().slice(0, 40));
  const outside = (node) => node.closest(".xterm");
  for (const control of document.querySelectorAll("button, a")) {
    // A hidden control with nothing in it, such as the needs-you chip while nothing needs Daniel, is filled before it shows.
    if (outside(control) || (control.closest("[hidden]") && !control.hasChildNodes())) continue;
    const name = __snapName(control);
    if (!name) log("snap:fail a11y: no accessible name" + (control.title ? " (a title alone)" : "") + " on " + said(control));
    else if (!/[\\p{L}\\p{N}]/u.test(name)) log("snap:fail a11y: a lone glyph for a name (" + JSON.stringify(name) + ") on " + said(control));
  }
  for (const node of document.querySelectorAll("[role=img], dialog, [role=dialog]")) {
    if (!outside(node) && !__snapName(node)) log("snap:fail a11y: no name on " + said(node));
  }
  for (const node of document.querySelectorAll("[aria-pressed][aria-expanded]")) log("snap:fail a11y: both aria-pressed and aria-expanded on " + said(node));
`;
