// The hover rule (#220), checked in every snap page after its fixture's script: every control the page holds, drawn or not,
// says something on hover, in its own `title` or an ancestor's; its own title never just repeats its label; and no titled
// element, a disabled control among them, sits where the pointer cannot reach it. A broken rule logs a `snap:fail` line.
export const hoverAudit = `
  const controls = [...document.querySelectorAll("button, summary, a[href], [role=button], #toast, #note-toast, #spine")]
    .filter((node) => !node.closest(".xterm"));
  const words = (text) => (text ?? "").replace(/\\s+/g, " ").trim();
  const named = (node) => node.tagName.toLowerCase() + (node.id ? "#" + node.id : "") + (typeof node.className === "string" && node.className ? "." + node.className.trim().split(/\\s+/).join(".") : "") + " " + JSON.stringify(words(node.textContent).slice(0, 40));
  for (const control of controls) {
    const hover = control.closest("[title]")?.getAttribute("title") ?? "";
    if (!words(hover)) log("snap:fail hover: no hover on " + named(control));
    else if (control.hasAttribute("title") && words(control.title) === words(control.textContent)) log("snap:fail hover: repeats its label on " + named(control));
  }
  for (const node of document.querySelectorAll("[title]")) {
    if (node.closest(".xterm")) continue;
    if (getComputedStyle(node).pointerEvents === "none") log("snap:fail hover: out of the pointer's reach on " + named(node));
  }
`;
