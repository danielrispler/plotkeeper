// An agent's logo (danielrispler/plotkeeper#158): a 14 px inline SVG on the current colour, so the muted text colour makes it
// one colour. One pure function maps a session, or just an agent's name, to the logo's key; `logo` draws it.
import type { Session } from "./types.js";

export type LogoKey = "claude" | "agy" | "codex" | "shell" | "generic";

// What a status-only row's name, "codex · folder", says its program is.
const programOf = (name: string): string => name.split(" · ")[0];

// The logo of a session: its adapter's agent, a plain shell, or, for a status-only row, its program, of which only `codex`
// is known. An agent's name alone ("claude", "agy", "codex") works too, for a view with no session.
export function logoKey(of: string | Pick<Session, "capabilities" | "shell" | "status_only" | "identity"> | { agent: string }): LogoKey {
  const agent = typeof of === "string" ? of : "agent" in of ? of.agent : of.capabilities.agent;
  if (typeof of === "object" && "shell" in of) {
    if (of.shell) return "shell";
    if (of.status_only) return programOf(of.identity.name) === "codex" ? "codex" : "generic";
  }
  return agent === "claude" || agent === "agy" || agent === "codex" ? agent : "generic";
}

// Hand-drawn marks in a 14 px box, stroked on the current colour: Claude's spark, Antigravity's arch, Codex's knot, a prompt
// for a shell and a plain ring for any other program.
const marks: Record<LogoKey, string> = {
  claude: '<path d="M7 1.5v11M1.5 7h11M3.1 3.1l7.8 7.8M10.9 3.1l-7.8 7.8"/>',
  agy: '<path d="M2 12.5L7 2l5 10.5M4.6 9.5h4.8"/>',
  codex: '<ellipse cx="7" cy="7" rx="2.2" ry="5.5"/><ellipse cx="7" cy="7" rx="2.2" ry="5.5" transform="rotate(60 7 7)"/><ellipse cx="7" cy="7" rx="2.2" ry="5.5" transform="rotate(120 7 7)"/>',
  shell: '<path d="M2.5 4l3.5 3-3.5 3M7.5 11h4"/>',
  generic: '<circle cx="7" cy="7" r="5"/><circle cx="7" cy="7" r="1.2"/>',
};

const titles: Record<LogoKey, string> = {
  claude: "Claude Code",
  agy: "agy: followed by what it reports, which has no context share",
  codex: "Codex: followed by its reported status only",
  shell: "A plain terminal",
  generic: "Another agent, followed by its reported status only",
};

// The logo's element, named by its title so the shape is never alone.
export function logo(key: LogoKey, title = titles[key]): HTMLElement {
  const node = document.createElement("span");
  node.className = "logo";
  node.setAttribute("data-logo", key);
  node.title = title;
  node.setAttribute("role", "img");
  node.setAttribute("aria-label", title);
  node.innerHTML = `<svg viewBox="0 0 14 14" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">${marks[key]}</svg>`;
  return node;
}
