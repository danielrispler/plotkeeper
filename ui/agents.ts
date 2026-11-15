// The picker's agent choice: the agents on Daniel's `PATH` (`agents`), then a plain terminal. New
// session opens on the agent he last chose, New terminal on the terminal, which is never kept as
// his choice, so the next New session starts an agent again.

export type Kind = "session" | "terminal";

// What the picker offers and which it opens on; a last choice no longer installed falls back to
// Claude Code, else the first agent installed.
export function agentChoice(installed: string[], last: string | null, kind: Kind) {
  const offered = [...installed, "terminal"];
  const fallback = installed.includes("claude") ? "claude" : offered[0];
  const chosen = kind === "terminal" ? "terminal" : last && installed.includes(last) ? last : fallback;
  return { offered, chosen };
}

// The choice kept after Daniel picks `picked`, `last` being the kept one before.
export const kept = (picked: string, last: string) => (picked === "terminal" ? last : picked);

// Auto mode is Claude Code's (danielrispler/Falinks#541).
export const showsAuto = (agent: string) => agent === "claude";

export const AGENT_NAMES: Record<string, string> = { claude: "Claude Code", agy: "agy", codex: "Codex", terminal: "Terminal" };
