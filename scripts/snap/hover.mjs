// The hover rule (#220, rewritten by spec #582), checked in every snap page after its fixture's script. Every control the page holds,
// drawn or not, says something on hover in a hint of its own (`interestfor` or `data-hint`; an ancestor's hover does not count, a toast's
// inner buttons have hints of their own); that hint never just repeats the control's label, its accessible name (its `aria-label`, else
// its text) as `hint()` reads it; no `title` attribute and no SVG `<title>` is left outside the terminal; and no hinted element, a
// disabled control among them, sits where the pointer cannot reach it. A broken rule logs a `snap:fail` line.
export const hoverAudit = `
  const controls = [...document.querySelectorAll("button, summary, a[href], [role=button], #spine")]
    .filter((node) => !node.closest(".xterm"));
  const words = (text) => (text ?? "").replace(/\\s+/g, " ").trim();
  const named = (node) => node.tagName.toLowerCase() + (node.id ? "#" + node.id : "") + (typeof node.className === "string" && node.className ? "." + node.className.trim().split(/\\s+/).join(".") : "") + " " + JSON.stringify(words(node.textContent).slice(0, 40));
  for (const control of controls) {
    const own = control.getAttribute("interestfor") ?? control.getAttribute("data-hint");
    const hinted = own ? document.getElementById(own) : null;
    const hover = hinted ? hinted.textContent : "";
    if (!words(hover)) log("snap:fail hover: no hover on " + named(control));
    else if (words(hover) === words(control.getAttribute("aria-label") ?? control.textContent)) log("snap:fail hover: repeats its label on " + named(control));
  }
  for (const node of document.querySelectorAll("[title]")) {
    if (!node.closest(".xterm")) log("snap:fail hover: a title is left on " + named(node));
  }
  for (const node of document.querySelectorAll("svg title")) {
    if (!node.closest?.(".xterm")) log("snap:fail hover: a title is left on an SVG <title> " + JSON.stringify(words(node.textContent).slice(0, 40)));
  }
  for (const node of document.querySelectorAll("[interestfor], [data-hint]")) {
    if (node.closest(".xterm")) continue;
    if (getComputedStyle(node).pointerEvents === "none") log("snap:fail hover: out of the pointer's reach on " + named(node));
  }
`;
