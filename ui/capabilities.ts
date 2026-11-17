// What a session's view shows follows its agent's capabilities, never its name (danielrispler/plotkeeper#43, decision 4).
import type { Capabilities, Session } from "./types.js";

type WithCapabilities = { capabilities: Capabilities };

// Whether the session's agent reports `what`, so the part of the view that shows it may show.
export const reports = (session: WithCapabilities, what: Exclude<keyof Capabilities, "agent">): boolean => session.capabilities[what];

const marks: Record<string, string> = { claude: "Claude" };

// A model's short name: "claude-opus-5-5" is "Opus 5.5", "gemini-3.8-flash-high" is "Flash high".
function shortModel(agent: string, model: string): string {
  const parts = model.replace(/\[.*\]$/, "").split("-");
  if (agent === "claude") {
    // The family is the word; the version is its numbers, before it ("claude-3-5-sonnet") or after ("claude-opus-5-5").
    const rest = parts.slice(1).filter((p) => !/^\d{8}$/.test(p));
    const family = rest.find((p) => !/^\d+$/.test(p));
    const version = rest.filter((p) => /^\d+$/.test(p)).join(".");
    return [family && family[0].toUpperCase() + family.slice(1), version].filter(Boolean).join(" ");
  }
  const name = parts.filter((p) => !/^\d+(\.\d+)*$/.test(p) && p !== "gemini").join(" ");
  return name[0]?.toUpperCase() + name.slice(1);
}

// The header's model line: the agent's mark and the latest model's short name, "Claude · Opus 5.5" or "agy · Flash high".
// Claude's is in its transcript, agy's on the session; a status-only session shows none.
export function modelLine(session: Pick<Session, "capabilities" | "model" | "transcript">): string {
  const { agent } = session.capabilities;
  const model = agent === "claude" ? session.transcript.model : session.model;
  return agent && model ? `${marks[agent] ?? agent} · ${shortModel(agent, model)}` : "";
}
