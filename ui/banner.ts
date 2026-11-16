// "Where we were" (danielrispler/plotkeeper#124): the banner the rail opens on when Daniel returns to a session. Three lines
// built from facts: the last Headline with its age, what happened since he looked, and what is open. The Brief folds inside.

import { age, prose, text } from "./dom.js";
import type { OpenItem, Session, Turn } from "./types.js";

// A session whose last turn ended this long ago, with unseen turns, shows the banner.
export const bannerAfter = 15 * 60_000;

// Whether the banner shows when Daniel selects the session: an idle or ended one with a turn, or one with unseen turns
// whose last turn ended `bannerAfter` ago or more.
export function showsBanner(session: Pick<Session, "state" | "unseen" | "turns">, now: number): boolean {
  const last = session.turns.at(-1);
  if (!last) return false;
  if (session.state === "idle" || session.state === "ended") return true;
  return session.unseen === true && last.ended_at !== null && now - last.ended_at >= bannerAfter;
}

// What happened in the turns after the last Daniel saw (the core's `Session::since`).
export type Since = { turns: number; files: number; passed: number; failed: number; ran: number };
export function sinceOf(turns: Pick<Turn, "files" | "checks">[], seen: number): Since {
  const after = turns.slice(Math.min(seen, turns.length));
  const files = new Set(after.flatMap((turn) => (turn.files?.files ?? []).map((file) => `${file.root}\n${file.path}`)));
  const count = (status: string) => after.reduce((n, turn) => n + turn.checks.filter((check) => check.status === status).length, 0);
  return { turns: after.length, files: files.size, passed: count("passed"), failed: count("failed"), ran: count("ran") };
}

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

// "2 turns · 4 files · checks passed"; a check that failed or only ran is counted by itself.
export function sinceText(since: Since): string {
  if (since.turns === 0) return "Nothing new since you looked";
  const checks = since.failed || since.ran ? [`${since.passed} passed`, `${since.failed} failed`, `${since.ran} ran`].filter((part) => !part.startsWith("0 ")).join(", ") : since.passed ? "passed" : "";
  return [plural(since.turns, "turn"), ...(since.files ? [plural(since.files, "file")] : []), ...(checks ? [`checks ${checks}`] : [])].join(" · ");
}

// The banner is kept for one reading of the session: until Daniel selects another, or a prompt starts a turn.
export type Banner = { session: string; turns: number; since: Since };
export const bannerFor = (session: Pick<Session, "id" | "state" | "unseen" | "turns"> & { seen: number }, now: number): Banner | null =>
  showsBanner(session, now) ? { session: session.id, turns: session.turns.length, since: sinceOf(session.turns, session.seen) } : null;
export const bannerLives = (banner: Banner | null, session: Pick<Session, "id" | "state" | "turns">): boolean =>
  banner !== null && banner.session === session.id && session.turns.length === banner.turns && session.state !== "running" && session.state !== "needs_you";

export type BannerFacts = { headline: string | null; endedAt: number | null; since: Since; open: OpenItem[]; brief: Node[] };
export function bannerElement({ headline, endedAt, since, open, brief }: BannerFacts, now = Date.now()): HTMLElement {
  const node = document.createElement("section");
  node.id = "banner";
  node.className = "banner";
  const row = (label: string, ...content: (HTMLElement | string)[]) => {
    const line = document.createElement("p");
    line.append(text("span", label, "label"), " ", ...content);
    return line;
  };
  node.append(
    text("p", "Where we were", "banner-title"),
    row("Last", prose("span", headline ?? "No Headline"), ...(endedAt === null ? [] : [text("span", ` · ${age(endedAt, now)} ago`, "age")])),
    row("Since you looked", sinceText(since)),
    row("Open", open.length ? open.map((item) => item.text).join(" · ") : "Nothing open"),
  );
  if (brief.length) {
    const fold = document.createElement("details");
    fold.className = "banner-brief";
    const body = document.createElement("section");
    body.append(...brief);
    fold.append(text("summary", "Brief"), body);
    node.append(fold);
  }
  return node;
}
