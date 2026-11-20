// Home (spec #53, #226): the waiting room, the view in the terminal column while no session is selected. `homeView` is a pure
// function of the sessions, the learning item the card store chose, the projects and the window's Home state: an ordered list of parts, each absent when
// empty. Needs you is the only colour. `drawHome` draws it, patched by key so focus survives updates.

import { age, failure, fullTitle, invoke, patch, pressable, text } from "./dom.js";
import { quizAnswered, quizItem, quizRegion, seedQuiz } from "./quiz.js";
import { childrenOf, deleting, glyph, item, row, secondLine, selecting, sessionName, showsLanded, status } from "./list.js";
import { logo, logoKey } from "./logos.js";
import { foundWorkspaces, lastSegment, sessionProjectName } from "./workspaces.js";
import { lastHeadline } from "./story.js";
import { startLast } from "./picker.js";
import { closePane, redrawHome } from "./panes.js";
import { hideClearToast, showClearToast } from "./toast.js";
import { studio } from "./state.js";
import type { Learning, Session, Tap } from "./types.js";

export type HomeKey = "needs_you" | "running" | "unseen" | "learning" | "landed" | "start";
// A session's row on Home: its second line, and a muted note beside it (the intention's age).
export type HomeRow = { session: Session; line: string; note: string };
// One part of the page: `rows` are its sessions, `more` the sessions hidden behind its "+N" row.
export type HomePart = { key: HomeKey; title: string; colour: boolean; rows: HomeRow[]; more: number; slot: Slot | null; projects: HomeProject[] };
// The learning slot (#240): the item it serves, and whether a quiet "Next" follows an answer, because another item is due.
export type Slot = { item: Learning; next: boolean };
// The window's Home state: the groups Daniel expanded, kept until Home is left; `quiet`, set by "Not now", which hides the slot
// until Home is entered again or a turn ends; and `held`, the item he answered in this window, which stays with its reveal
// until "Next" or until Home is left.
export type HomeState = { expanded: Set<string>; cleared?: Set<string>; quiet: boolean; held: Learning | null };
export const homeState: HomeState = { expanded: new Set(), quiet: false, held: null };
export function resetHome(state: HomeState = homeState) {
  state.expanded.clear();
  state.quiet = false;
  state.held = null;
}

export const MAX_RUNNING = 4;
export const MAX_UNSEEN = 3;
const endOf = (s: Session) => s.turns.at(-1)?.ended_at ?? s.last_event_at ?? 0;
const plural = (n: number, one: string) => `${n} ${one}${n === 1 ? "" : "s"}`;

// What the slot shows: nothing while quiet; an item answered here, with "Next" when `asked`, the store's choice after that answer, is
// another item; else what the store chose.
export function learningSlot(asked: Learning | null, state: HomeState): Slot | null {
  if (state.quiet) return null;
  if (state.held) return { item: state.held, next: asked !== null && asked.kind !== "done" };
  return asked ? { item: asked, next: false } : null;
}

export function homeView(sessions: Session[], asked: Learning | null, projects: string[], state: HomeState, headline: (s: Session) => string | null = () => null): HomePart[] {
  const allLanded = sessions.filter((s) => showsLanded(s.state, s.landed));
  const landed = allLanded.filter((s) => !state.cleared?.has(s.id));
  const needsYou = sessions.filter((s) => s.state === "needs_you");
  const running = sessions.filter((s) => s.state === "running");
  // A landed session that is also unseen is listed as landed, keeping its unseen styling.
  const unseen = sessions.filter((s) => s.unseen && s.state !== "needs_you" && !allLanded.includes(s)).sort((a, b) => endOf(b) - endOf(a));
  const capped = (key: HomeKey, all: Session[], max: number, rowOf: (s: Session) => HomeRow) => {
    const shown = state.expanded.has(key) ? all : all.slice(0, max);
    return { rows: shown.map(rowOf), more: all.length - shown.length };
  };
  const plain = (rowOf: (s: Session) => HomeRow) => (all: Session[]) => ({ rows: all.map(rowOf), more: 0 });
  const part = (key: HomeKey, title: string, rows: { rows: HomeRow[]; more: number }, extra: Partial<HomePart> = {}): HomePart => ({ key, title, colour: key === "needs_you", slot: null, projects: [], ...rows, ...extra });
  const parts: HomePart[] = [
    part("needs_you", "Needs you", plain((s) => ({ session: s, line: secondLine(s) ?? "", note: "" }))(needsYou)),
    part("running", "Running", capped("running", running, MAX_RUNNING, (s) => {
      const steps = s.transcript?.steps_ago ?? 0;
      return { session: s, line: s.transcript?.intention ?? s.line ?? "", note: steps === 0 ? "" : `${plural(steps, "step")} ago` };
    })),
    part("unseen", "Unseen", capped("unseen", unseen, MAX_UNSEEN, (s) => ({ session: s, line: headline(s) ?? "", note: "" }))),
    part("learning", "Retest", { rows: [], more: 0 }, { slot: learningSlot(asked, state) }),
    part("landed", "Landed", plain((s) => ({ session: s, line: "", note: "" }))(landed)),
    part("start", "Start a session in", { rows: [], more: 0 }, { projects: homeProjects(projects) }),
  ];
  return parts.filter((p) => p.rows.length || p.slot || p.projects.length);
}

// The slot's card: the Quiz of the item the store chose, which records its taps as a Retest's, with "Not now" until it is answered
// and a quiet "Next" after an answer when another item is due. It writes nothing as it is built: `markLearningShown` writes `shown`.
export function slotNode(slot: Slot, redraw: () => void, reask: () => void): HTMLElement {
  const { item } = slot;
  const wrap = document.createElement("div");
  if (item.kind === "done") {
    wrap.append(text("p", "Done for today", "empty"));
    return wrap;
  }
  const quiz = item.quiz;
  if (!quiz) return wrap;
  seedQuiz(item.id, item.kind === "retest" ? item.taps : []);
  const tap = (tap: Tap) => {
    if (tap.kind === "pick" || tap.kind === "skip") homeState.held = item;
    void invoke("quiz_tap", { id: item.id, tap })
      .then(reask)
      .catch(failure("quiz_tap"));
    if (homeState.held) redraw();
  };
  const region = quizRegion(item.id, quiz, [], {
    record: tap,
    explain: () => invoke("ask", { session: item.session_id, item: quizItem(quiz), question: "I did not get this yet. Explain more." }),
    line: () => document.createElement("li"),
    trial: true,
  });
  wrap.append(region);
  const actions = text("p", "", "slot-actions");
  const button = (label: string, title: string, run: () => void) => {
    const one = pressable(document.createElement("button"), false, run);
    one.classList.add("quiz-skip");
    one.title = title;
    one.textContent = label;
    return one;
  };
  if (!quizAnswered(item.id) && homeState.held !== item) {
    actions.append(button("Not now", "Set this aside until tomorrow", () => {
      homeState.quiet = true;
      tap({ kind: "not_now" });
      redraw();
    }));
  }
  if (slot.next) {
    actions.append(button("Next", "The next question", () => {
      homeState.held = null;
      redraw();
    }));
  }
  if (actions.childNodes.length) wrap.append(actions);
  wrap.dataset.key = `learning ${item.id}`;
  return wrap;
}

// The item the last draw put in the slot, unanswered, whose `shown` is still to be written.
let inSlot: Learning | null = null;
const shownWritten = new Set<string>();

// Whether the item's first showing is written now (#240): the item is a Retest on screen in the slot, Home is the active view and the
// window is visible and focused. Once per item.
export function shownDue(item: Learning | null, home: boolean, visible: boolean, focused: boolean): boolean {
  if (item?.kind !== "retest" || !home || !visible || !focused) return false;
  return !shownWritten.has(item.id) && !item.taps.some((t) => t.kind === "shown");
}

// Writes the slot's `shown` when its conditions hold; called when Home is drawn and when the window becomes visible or focused.
export function markLearningShown(home: boolean) {
  if (!shownDue(inSlot, home, document.visibilityState === "visible", document.hasFocus())) return;
  const id = (inSlot as { id: string }).id;
  shownWritten.add(id);
  void invoke("quiz_tap", { id, tap: { kind: "shown" } }).catch(failure("quiz_tap"));
}

// Tells that a turn ended: its end time is later than the latest one seen. The first call only learns it.
let lastEnd: number | null = null;
export function turnEnded(sessions: Session[]): boolean {
  const end = Math.max(0, ...sessions.map((s) => s.turns.at(-1)?.ended_at ?? 0));
  const ended = lastEnd !== null && end > lastEnd;
  lastEnd = end;
  return ended;
}

// Clear landed (#244): Clear hides every landed row and offers Undo for CLEAR_WINDOW_MS. When the window closes each hidden session
// that is still landed goes through today's Delete (`commit`); Undo commits nothing. `changed` hears the sessions still pending.
export type Clock = { set(run: () => void, ms: number): unknown; clear(id: unknown): void };
export const CLEAR_WINDOW_MS = 8000;
export function clearing(time: Clock, deps: { landed(id: string): Session | undefined; commit(session: Session): void; changed(pending: Session[]): void }) {
  const pending = new Map<string, Session>();
  const hidden = new Set<string>();
  let timer: unknown = null;
  const stop = () => {
    if (timer !== null) time.clear(timer);
    timer = null;
  };
  const tell = () => deps.changed([...pending.values()]);
  const forget = () => {
    stop();
    pending.clear();
    hidden.clear();
    tell();
  };
  return {
    hidden,
    clear(sessions: Session[]) {
      for (const s of sessions) (pending.set(s.id, s), hidden.add(s.id));
      stop();
      timer = time.set(() => {
        timer = null;
        const due = [...pending.keys()].map(deps.landed);
        forget();
        for (const s of due) if (s) deps.commit(s);
      }, CLEAR_WINDOW_MS);
      tell();
    },
    undo: forget,
    // A session that stopped being landed in the window is left alone: it comes back and is not pending.
    sync() {
      let dropped = false;
      for (const id of [...pending.keys()]) if (!deps.landed(id)) (pending.delete(id), hidden.delete(id), (dropped = true));
      if (!dropped) return;
      if (pending.size === 0) stop();
      tell();
    },
  };
}

// The window's one Clear: its hidden rows show in `homeState.cleared`, the toast offers Undo, and the shell is told which sessions a quit
// would commit (`clearing`). A commit is today's Delete: the terminal ended, the row dropped, the cards kept.
const stillLanded = (id: string) => studio.listed.find((s) => s.id === id && showsLanded(s.state, s.landed));
export const landedClear = clearing(
  { set: (run, ms) => setTimeout(run, ms), clear: (id) => clearTimeout(id as number) },
  {
    landed: stillLanded,
    commit: (session) =>
      void (async () => {
        if (session.terminal !== null) await closePane(session.terminal);
        await invoke("delete_session", { id: session.id });
      })().catch(failure("delete_session", "error")),
    changed: (pending) => {
      void invoke("clearing", { sessions: pending.map((s) => ({ id: s.id, terminal: s.terminal })) }).catch(failure("clearing"));
      if (pending.length) showClearToast(pending.length, () => landedClear.undo());
      else hideClearToast();
      redrawHome();
    },
  },
);
homeState.cleared = landedClear.hidden;

export type HomeProject = { name: string; path: string };

// The projects Home offers: one per entry of `projects.json`, in its order.
export function homeProjects(paths: string[]): HomeProject[] {
  return paths.map((path) => ({ name: lastSegment(path) || path, path }));
}

// Whether a click on `target`, inside the session list, is on its empty space: not on a row, a group
// or project heading, a workspace or any button.
export function listSpace(target: { closest(selector: string): unknown }): boolean {
  return ![".item", "summary", ".project-head", ".project-workspaces", "button"].some((selector) => target.closest(selector));
}

// A session's row, the list's, with its Delete beside it on a landed one.
function sessionRow(session: Session, landed: boolean): HTMLElement {
  return item(session.id, 0, null, row(session, childrenOf(session)), landed ? deleting(session) : null);
}

// A Home row of a session that needs Daniel, runs or is unseen: its logo, name, project and clock, then its line, muted.
function waitingRow({ session, line, note }: HomeRow, part: HomeKey, project: string): HTMLElement {
  const button = selecting(session.id, null, `${part} ${session.id}`);
  button.classList.add("row", "home-row");
  button.classList.toggle("unseen", !!session.unseen);
  button.title = `${sessionName(session)} · ${status(session.state, session.unseen, session.landed)[1]}`;
  const first = text("span", "", "l1");
  const clock = part === "running" && session.turn_span ? age(session.turn_span.start) : part === "unseen" ? `${age(session.turns.at(-1)?.ended_at ?? session.last_event_at)} ago` : "";
  first.append(logo(logoKey(session)), fullTitle(text("span", sessionName(session), "name"), sessionName(session)), text("span", project, "id"), ...(clock ? [text("span", clock, "clock")] : []), ...(part === "needs_you" ? [glyph(session.state)] : []));
  button.append(first);
  if (line) {
    const second = text("span", "", "l2");
    second.append(text("span", line, "row-step"), ...(note ? [text("span", note, "id")] : []));
    fullTitle(second, line);
    button.append(second);
  }
  return button;
}

// The "+N running" row, which expands its group inline until Home is left.
function moreRow(key: HomeKey, count: number, redraw: () => void): HTMLElement {
  const button = pressable(document.createElement("button"), false, () => {
    homeState.expanded.add(key);
    redraw();
  });
  button.dataset.key = `more ${key}`;
  button.classList.add("row", "more");
  button.textContent = `+${count} ${key}`;
  return button;
}

function projectRow({ name, path }: HomeProject): HTMLElement {
  const button = pressable(document.createElement("button"), false, () => void startLast(path, "").catch(failure("start", "error")));
  button.dataset.key = `project ${path}`;
  button.classList.add("row");
  button.title = `Start a session in ${path}`;
  const line = text("span", "", "l1");
  line.append(text("span", name, "name"), text("span", "New session", "id"));
  button.append(line);
  return button;
}

// Draws Home into `box`, patched, so focus survives the list's updates; `redraw` draws it again after an expansion.
export function drawHome(box: HTMLElement, sessions: Session[], projects: string[], asked: Learning | null = null, redraw: () => void = () => {}, reask: () => void = () => {}) {
  box.classList.add("home");
  // A turn's end brings the slot back after "Not now", and asks the store again.
  if (turnEnded(sessions)) {
    homeState.quiet = false;
    reask();
  }
  const parts = homeView(sessions, asked, projects, homeState, lastHeadline);
  const slot = parts.find((p) => p.key === "learning")?.slot ?? null;
  inSlot = slot && slot.item.kind === "retest" && homeState.held !== slot.item ? slot.item : null;
  const found = foundWorkspaces();
  const keyed = (node: HTMLElement, key: string) => {
    node.dataset.key = key;
    return node;
  };
  const heading = (title: string, key: string) => {
    const node = keyed(text("h2", title), key);
    if (key !== "h landed") return node;
    const clear = text("button", "Clear", "btn quiet small clear");
    clear.title = "Clear every landed session; Undo for a few seconds";
    clear.addEventListener("click", () => landedClear.clear(studio.listed.filter((s) => showsLanded(s.state, s.landed) && !landedClear.hidden.has(s.id))));
    node.append(clear);
    return node;
  };
  const drawPart = (part: HomePart): HTMLElement[] => [
    heading(part.title, `h ${part.key}`),
    ...part.rows.map((r) => (part.key === "landed" ? sessionRow(r.session, true) : waitingRow(r, part.key, sessionProjectName(projects, found, r.session)))),
    ...(part.more ? [moreRow(part.key, part.more, redraw)] : []),
    ...(part.slot ? [slotNode(part.slot, redraw, reask)] : []),
    ...part.projects.map(projectRow),
  ];
  patch(box, [
    keyed(text("h1", new Date().toLocaleDateString("en-GB", { weekday: "long", day: "numeric", month: "long" }), "title"), "title"),
    ...(parts.some((p) => p.key === "needs_you") ? [] : [keyed(text("p", "Nothing needs you", "empty"), "none")]),
    ...parts.flatMap(drawPart),
  ]);
  markLearningShown(true);
}
