// Plan usage (danielrispler/plotkeeper#163): the top bar's glance and the popover's tiles. The workbench gives each agent's
// usage in one shape, `AgentUsage`: its windows and when and from where they were read, or one line saying why it has none.
// Each window has `used_percentage` (0 to 100) and `resets_at` (Unix seconds); a window past its reset is dropped. Times here
// are Unix seconds.
import { fullTitle, invoke, text } from "./dom.js";
import { agentNames } from "./capabilities.js";
import { logo, logoKey } from "./logos.js";

type UsageWindow = { used_percentage: number; resets_at: number };
export type AgentUsage = {
  agent: string;
  five_hour: UsageWindow | null;
  seven_day: UsageWindow | null;
  read_at: number | null;
  source: string | null;
  reason: string | null;
};

const nameOf = (agent: string) => agentNames[agent] ?? agent;

// The windows that are still current, in the order they show.
const current = (usage: AgentUsage, now: number) =>
  (
    [
      ["5h", "5-hour", usage.five_hour],
      ["week", "weekly", usage.seven_day],
    ] as const
  ).flatMap(([label, long, w]) => (w && w.resets_at > now ? [{ label, long, ...w }] : []));

// The tightest window across the agents: the one with the largest share used, as that agent's logo and "5h 62%" or "wk 80%".
export function glance(usages: AgentUsage[], now: number): { agent: string; text: string; title: string } | null {
  const all = usages.flatMap((usage) => current(usage, now).map((w) => ({ agent: usage.agent, ...w })));
  const tightest = all.reduce<(typeof all)[number] | null>((best, w) => (!best || w.used_percentage > best.used_percentage ? w : best), null);
  if (!tightest) return null;
  const share = Math.floor(tightest.used_percentage);
  return {
    agent: tightest.agent,
    text: `${tightest.label === "week" ? "wk" : tightest.label} ${share}%`,
    title: `${nameOf(tightest.agent)}'s ${tightest.long} usage is ${share}%. Click for plan usage.`,
  };
}

// Draws the glance in the bar's usage button: the logo and the text, or "Usage" when no agent has data.
export function showGlance(button: HTMLElement, usages: AgentUsage[], now: number) {
  const found = glance(usages, now);
  if (!found) {
    button.replaceChildren(text("span", "Usage"));
    button.title = "Plan usage";
    return;
  }
  button.replaceChildren(logo(logoKey({ agent: found.agent }), nameOf(found.agent)), text("span", found.text));
  button.title = found.title;
}

// "2h 10m", or "3d 4h" past a day.
function span(seconds: number): string {
  const minutes = Math.max(0, Math.floor(seconds / 60));
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  return hours < 24 ? `${hours}h ${minutes % 60}m` : `${Math.floor(hours / 24)}d ${hours % 24}h`;
}

function ago(seconds: number): string {
  const minutes = Math.floor(seconds / 60);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.floor(minutes / 60);
  return hours < 24 ? `${hours} h ago` : `${Math.floor(hours / 24)} d ago`;
}

// One agent's tile: its logo and name, then a bar per current window with the share used and its reset, and the data's age and
// source. A failed read shows only its reason, never an old number. While `reading`, the footer says so instead of an age.
export function tile(usage: AgentUsage, now: number, reading = false): HTMLElement {
  const node = document.createElement("section");
  node.className = "usage-tile";
  const head = document.createElement("div");
  head.className = "tile-head";
  head.append(logo(logoKey({ agent: usage.agent }), nameOf(usage.agent)), text("span", nameOf(usage.agent), "tile-name"));
  node.append(head);
  if (usage.reason !== null) {
    node.append(fullTitle(text("p", usage.reason, "tile-reason"), usage.reason));
    if (reading) node.append(text("p", "Reading…", "tile-foot"));
    return node;
  }
  for (const w of current(usage, now)) {
    const share = Math.floor(w.used_percentage);
    const row = document.createElement("div");
    row.className = "usage-window";
    const bar = document.createElement("div");
    bar.className = "bar";
    const fill = document.createElement("div");
    fill.className = "bar-fill";
    fill.style.width = `${Math.min(100, share)}%`;
    bar.append(fill);
    row.append(text("span", w.label, "window-name"), bar, text("span", `${share}%`, "window-share"), text("span", `resets in ${span(w.resets_at - now)}`, "window-resets"));
    node.append(row);
  }
  if (reading) node.append(text("p", "Reading…", "tile-foot"));
  else if (usage.read_at !== null) {
    const foot = [`as of ${ago(now - usage.read_at)}`, usage.source].filter(Boolean).join(" · ");
    node.append(fullTitle(text("p", foot, "tile-foot"), foot));
  }
  return node;
}

// The newest read of each agent, so a read of some agents redraws the glance without losing the others'. Each agent keeps the
// sequence number of the read it came from: a slower, older read that resolves later never overwrites a newer one.
const latest = new Map<string, AgentUsage>();
const applied = new Map<string, number>();
let sequence = 0;

const drawTiles = (popover: HTMLElement, reading: boolean) => {
  const now = Date.now() / 1000;
  popover.replaceChildren(...[...latest.values()].map((usage) => tile(usage, now, reading)));
};

// Reads usage and draws it. `agents` is the ones to read, or null for every agent: only the popover's open does that. The
// load, the focus and the timer pass only the agents that cost nothing to read (Claude's snapshot): an agent that runs a CLI
// to answer is never read behind the user's back. A read of every agent redraws the popover's tiles, which say "Reading…"
// until it is done. A read that fails keeps the old data.
export async function readUsage(glanceButton: HTMLElement, popover: HTMLElement, agents: string[] | null) {
  const mine = ++sequence;
  if (agents === null) drawTiles(popover, true);
  try {
    for (const usage of await invoke<AgentUsage[]>("usage", { agents })) {
      if ((applied.get(usage.agent) ?? 0) > mine) continue;
      applied.set(usage.agent, mine);
      latest.set(usage.agent, usage);
    }
  } catch {
    // The old data stays.
  }
  showGlance(glanceButton, [...latest.values()], Date.now() / 1000);
  if (agents === null) drawTiles(popover, false);
}

// The timer's refresh of the glance reads only these.
export const timedAgents = ["claude"];
