// Home (spec #53, #226): the waiting room, the view in the terminal column while no session is selected. `homeView` is a pure
// function of the sessions, the learning item the card store chose, the projects and the window's Home state: an ordered list of parts, each absent when
// empty. Needs you is the only colour. `drawHome` draws it, patched by key so focus survives updates.

import { age, failure, fullTitle, invoke, patch, pressable, text } from "./dom.js";
import { quizAnswered, quizItem, quizRegion, seedQuiz } from "./quiz.js";
import { childrenOf, deleting, glyph, item, row, secondLine, selecting, sessionName, showsLanded, status } from "./list.js";
import { logo, logoKey } from "./logos.js";
import { foundWorkspaces, lastSegment, sessionProjectName } from "./workspaces.js";
import { lastHeadline } from "./story.js";
import { hunkFiles } from "./ask.js";
import { codeTarget, followToCode } from "./selection.js";
import { startLast } from "./picker.js";
import { counts } from "./times.js";
import { closePane, forgetAsked, homeShown, redrawHome } from "./panes.js";
import { hideClearToast, showClearToast } from "./toast.js";
import { studio } from "./state.js";
import { mapShown, openMap } from "./map.js";
import { watchSeen } from "./seen.js";
import { homeSeen } from "./layout.js";
import { logStop, onHome, reviewNode, reviewOf, type Module } from "./stop-rules.js";
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
// `cleared` is the rows Clear hides, the one set `landedClear` keeps; leaving Home does not reset it.
export type HomeState = { expanded: Set<string>; cleared: Set<string>; quiet: boolean; held: Learning | null };
export const homeState: HomeState = { expanded: new Set(), cleared: new Set(), quiet: false, held: null };
export function resetHome(state: HomeState = homeState) {
  state.expanded.clear();
  state.quiet = false;
  state.held = null;
  // What was marked on screen belongs to the visit, so the sets do not grow for the whole session.
  shownWritten.clear();
}

// Entering Home from another view starts it afresh; drawing it again while it is on screen keeps what Daniel did there.
export function enterHome(already: boolean, state: HomeState = homeState) {
  if (!already) {
    resetHome(state);
    staleMarks();
  }
}

export const MAX_RUNNING = 4;
export const MAX_UNSEEN = 3;
const endOf = (s: Session) => s.turns.at(-1)?.ended_at ?? s.last_event_at ?? 0;
const plural = (n: number, one: string) => `${n} ${one}${n === 1 ? "" : "s"}`;

// What the slot shows: nothing while quiet; an item answered here, with "Next" when `asked`, the store's choice after that answer, is
// another item; else what the store chose.
export function learningSlot(asked: Learning | null, state: HomeState): Slot | null {
  if (state.quiet) return null;
  if (state.held) return { item: state.held, next: asked !== null && asked.kind !== "done" && asked.id !== (state.held as { id: string }).id };
  return asked ? { item: asked, next: false } : null;
}

export function homeView(sessions: Session[], asked: Learning | null, projects: string[], state: HomeState, headline: (s: Session) => string | null = () => null, fogged: Set<string> = new Set()): HomePart[] {
  const allLanded = sessions.filter((s) => showsLanded(s.state, s.landed));
  const landed = allLanded.filter((s) => !state.cleared.has(s.id));
  const needsYou = sessions.filter((s) => s.state === "needs_you");
  const running = sessions.filter((s) => s.state === "running");
  // A landed session that is also unseen is listed as landed, keeping its unseen styling.
  const unseen = sessions.filter((s) => s.unseen && (s.state === "your_turn" || s.state === "idle") && !allLanded.includes(s)).sort((a, b) => endOf(b) - endOf(a));
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
    part("learning", "", { rows: [], more: 0 }, { slot: learningSlot(asked, state) }),
    part("landed", "Landed", plain((s) => ({ session: s, line: "", note: "" }))(landed)),
    part("start", "Start a session in", { rows: [], more: 0 }, { projects: homeProjects(projects, fogged) }),
  ];
  return parts.filter((p) => p.rows.length || p.slot || p.projects.length);
}

// The taps Home sent, in a chain: each waits for the one before it to be stored.
let sent: Promise<unknown> = Promise.resolve();

// The small label at the top of the slot's card: a Retest's age and the session of its first Quiz, a fresh Quiz's session, none for "Done for today".
export function slotLabel(item: Learning, session: Pick<Session, "name" | "identity" | "folder_name"> | undefined, now = Date.now()): string {
  if (item.kind === "done") return "";
  const ref = session ? (session.identity.issue === null ? sessionName(session) : `#${session.identity.issue}`) : null;
  if (item.kind === "fresh") return ref ? `Quiz · ${ref}` : "Quiz";
  if (!item.of_at) return "Retest";
  const days = Math.floor((now - item.of_at) / 864e5);
  return `Retest · ${plural(days, "day")} ${ref ? `after ${ref}` : "ago"}`;
}

// The slot's card: the Quiz of the item the store chose, which records its taps as a Retest's, with "Not now" until it is answered
// and a quiet "Next" after an answer when another item is due. It writes nothing as it is built: `watchLearningShown` writes `shown` once the card is seen.
export function slotNode(slot: Slot, redraw: () => void, reask: () => void): HTMLElement {
  const { item } = slot;
  const wrap = document.createElement("div");
  wrap.className = "slot-card";
  if (item.kind === "done") {
    wrap.className = "";
    wrap.append(text("p", "Done for today", "empty"));
    return wrap;
  }
  const quiz = item.quiz;
  if (!quiz) return wrap;
  seedQuiz(item.id, item.kind === "retest" ? item.taps : []);
  const tap = (tap: Tap) => {
    if (tap.kind === "pick" || tap.kind === "skip") homeState.held = item;
    // One after another, so the confidence tap is stored before its pick.
    sent = sent
      .then(() => invoke("quiz_tap", { id: item.id, tap }))
      .then(reask)
      .catch(failure("quiz_tap"));
    if (homeState.held) redraw();
  };
  const button = (label: string, title: string, run: () => void) => {
    const one = pressable(document.createElement("button"), false, run);
    one.classList.add("quiz-skip");
    one.title = title;
    one.textContent = label;
    return one;
  };
  const when = item.kind === "retest" ? item.of_at : item.kind === "fresh" ? item.at : null;
  const date = when ? new Date(when).toLocaleDateString("en-GB", { day: "numeric", month: "long" }) : null;
  const region = quizRegion(item.id, quiz, [], {
    record: tap,
    explain: () => invoke("ask", { session: item.session_id, item: quizItem(quiz), question: "I did not get this yet. Explain more." }),
    line: () => document.createElement("li"),
    // "Not now" sits in the card, beside "Not sure", until it is answered.
    home: {
      date,
      actions: (answered) =>
        answered
          ? []
          : [button("Not now", "Set this aside until tomorrow", () => {
              homeState.quiet = true;
              tap({ kind: "not_now" });
              redraw();
            })],
    },
  });
  const session = studio.listed.find((s) => s.id === item.session_id);
  wrap.append(text("p", slotLabel(item, session), "label"), region);
  // After an answer: the link to the code and the hunk (#243), then a quiet "Next" when another item is due.
  if (quizAnswered(item.id)) wrap.append(...answerCode(item));
  // An item that arrives answered without being held here (a stored skip, say) would leave no way on: "Next" sets it aside.
  const stuck = quizAnswered(item.id) && homeState.held !== item;
  if (stuck && !slot.next) {
    const actions = text("p", "", "slot-actions");
    actions.append(button("Next", "Set this aside until tomorrow", () => {
      homeState.quiet = true;
      tap({ kind: "not_now" });
      redraw();
    }));
    wrap.append(actions);
  }
  if (slot.next) {
    const actions = text("p", "", "slot-actions");
    actions.append(button("Next", "The next question", () => {
      homeState.held = null;
      redraw();
    }));
    wrap.append(actions);
  }
  wrap.dataset.key = `learning ${item.id}`;
  return wrap;
}

// Under an answer's reveal (#243): the link to the code the Quiz was about, and the Quiz's stored hunk in place, compact: each file as a
// faint caption and its code lines. The hunk is the item's own text, so no git read and no `hunk_opened`: a hunk counts as opened only
// on Daniel's own action. A Retest names the turn of its first Quiz. No link when the session or its turn is gone; no hunk without text.
function answerCode(item: Learning): HTMLElement[] {
  if (item.kind === "done") return [];
  const quizId = item.kind === "retest" ? item.of : item.id;
  const stored = item.hunk ?? "";
  const session = studio.listed.find((s) => s.id === item.session_id);
  const out: HTMLElement[] = [];
  if (session && codeTarget(session.turns, quizId, stored)) {
    const link = pressable(text("button", "See the code", "quiz-code-link"), false, () => void followToCode(session.id, quizId, stored));
    link.title = "Open the turn, and its change, in the session";
    out.push(link);
  }
  const files = hunkFiles(stored);
  if (files.length) {
    const hunk = document.createElement("ul");
    hunk.className = "evidence-rows";
    for (const file of files) {
      const li = document.createElement("li");
      li.append(text("p", file.path, "evidence-caption"), text("pre", file.lines, "evidence-hunk"));
      hunk.append(li);
    }
    out.push(hunk);
  }
  return out;
}

// The item the last draw put in the slot, unanswered, whose `shown` is still to be written, and the watch that waits for it to be seen.
type Asked = Exclude<Learning, { kind: "done" }>;
let inSlot: Asked | null = null;
const shownWritten = new Set<string>();
let slotWatch: { id: string; node: Element; stop: () => void } | null = null;

// Whether the item's first showing is still to be written (#240): once per item, and never for a Retest that has it.
export function shownDue(item: Asked | null): boolean {
  if (!item) return false;
  return !shownWritten.has(item.id) && !(item.kind === "retest" && item.taps.some((t) => t.kind === "shown"));
}

// Writes the slot's `shown` once its card is seen (the shared Seen, #306): in the viewport of a visible, focused window with Home
// open (`active`), not merely built. Watches the card as it is on screen; a redraw that keeps the card keeps its watch.
export function watchLearningShown(node: Element | null, item: Asked | null, active: () => boolean) {
  if (slotWatch && (!node || !item || slotWatch.id !== item.id || slotWatch.node !== node)) {
    slotWatch.stop();
    slotWatch = null;
  }
  if (!node || !item || !shownDue(item) || slotWatch) return;
  const id = item.id;
  slotWatch = {
    id,
    node,
    stop: watchSeen(node, {
      active,
      onSeen: () => {
        slotWatch = null;
        if (!shownDue(item)) return;
        shownWritten.add(id);
        void invoke("quiz_tap", { id, tap: { kind: "shown" } }).catch(failure("quiz_tap"));
      },
    }),
  };
}

// Tells that a turn ended: a session has a turn whose end is later than what was seen of that session (for one new to the list, than the
// latest end seen anywhere). Any turn counts, not only the last, so a queued prompt that starts the next turn at once hides no end, and
// a session starting a new turn never moves anything back. The first call only learns.
export function endTracker() {
  let latest: number | null = null;
  const seen = new Map<string, number>();
  return (sessions: Pick<Session, "id" | "turns">[]): boolean => {
    let ended = false;
    let top = latest ?? 0;
    for (const s of sessions) {
      const end = Math.max(0, ...s.turns.map((t) => t.ended_at ?? 0));
      if (latest !== null && end > (seen.get(s.id) ?? latest)) ended = true;
      seen.set(s.id, Math.max(end, seen.get(s.id) ?? 0));
      top = Math.max(top, end);
    }
    latest = top;
    return ended;
  };
}
export const turnEnded = endTracker();

// Clear landed (#244): Clear hides every landed row and offers Undo for CLEAR_WINDOW_MS. When the window closes each hidden session
// that is still landed goes through today's Delete (`commit`); Undo commits nothing. `changed` hears the sessions still pending.
export type Clock = { set(run: () => void, ms: number): unknown; clear(id: unknown): void };
export const CLEAR_WINDOW_MS = 8000;
export function clearing(
  time: Clock,
  deps: { landed(id: string): Session | undefined; commit(session: Session): Promise<unknown> | void; changed(pending: Session[], writing: Session[]): void },
  hidden = new Set<string>(),
) {
  const pending = new Map<string, Session>();
  // The sessions whose delete is in flight: the shell still hears them, so a quit before the write commits them (#244).
  const writing = new Map<string, Session>();
  let timer: unknown = null;
  const stop = () => {
    if (timer !== null) time.clear(timer);
    timer = null;
  };
  const tell = () => deps.changed([...pending.values()], [...writing.values()]);
  return {
    // The rows Clear hides: those pending and those committed, until they leave the list.
    hidden,
    clear(sessions: Session[]) {
      for (const s of sessions) {
        pending.set(s.id, s);
        hidden.add(s.id);
      }
      stop();
      timer = time.set(() => {
        timer = null;
        const due = [...pending.keys()].map((id) => ({ id, session: deps.landed(id) }));
        pending.clear();
        for (const { id, session } of due) if (session) writing.set(id, session);
        tell();
        for (const { id, session } of due) {
          if (!session) hidden.delete(id);
          else
            void Promise.resolve(deps.commit(session)).then(
              () => {
                writing.delete(id);
                tell();
              },
              // A row whose Delete failed comes back.
              () => {
                writing.delete(id);
                hidden.delete(id);
                tell();
              },
            );
        }
      }, CLEAR_WINDOW_MS);
      tell();
    },
    undo() {
      stop();
      for (const id of pending.keys()) hidden.delete(id);
      pending.clear();
      tell();
    },
    // A session that left the list, or stopped being landed, is no longer hidden; one still pending is left alone: it comes back and is
    // not committed.
    sync() {
      let dropped = false;
      for (const id of [...hidden]) {
        if (deps.landed(id)) continue;
        hidden.delete(id);
        dropped = pending.delete(id) || dropped;
        dropped = writing.delete(id) || dropped;
      }
      if (!dropped) return;
      if (pending.size === 0) stop();
      tell();
    },
  };
}

// The window's one Clear: its hidden rows are `homeState.cleared`, the toast offers Undo, and the shell is told which sessions a quit
// would commit (`clearing`). A commit is today's Delete: the terminal ended, the row dropped, the cards kept.
const stillLanded = (id: string) => studio.listed.find((s) => s.id === id && showsLanded(s.state, s.landed));
// Clear's and Delete's one rule (#244, #270): a terminal is in use while a session that is not leaving the list, and not ended, runs in
// it. An ended session keeps the terminal a later one may now use.
export const terminalInUse = (terminal: string, listed: Session[], leaving: (id: string) => boolean) =>
  listed.some((s) => s.terminal === terminal && s.state !== "ended" && !leaving(s.id));
// A terminal is ended once, and never while a session that is not being cleared still runs in it.
const closed = new Set<string>();
const terminalFree = (terminal: string) =>
  !closed.has(terminal) && !terminalInUse(terminal, studio.listed, (id) => landedClear.hidden.has(id)) && (closed.add(terminal), true);
export const landedClear = clearing(
  { set: (run, ms) => setTimeout(run, ms), clear: (id) => clearTimeout(id as number) },
  {
    landed: stillLanded,
    commit: (session) =>
      (async () => {
        if (session.terminal !== null && terminalFree(session.terminal)) await closePane(session.terminal);
        await invoke("delete_session", { id: session.id });
      })().catch((e) => {
        failure("delete_session", "error")(e);
        throw e;
      }),
    changed: (pending, writing) => {
      void invoke("clearing", { sessions: [...pending, ...writing].map((s) => ({ id: s.id, terminal: s.terminal })) }).catch(failure("clearing"));
      if (pending.length) showClearToast(pending.length, () => landedClear.undo());
      else hideClearToast();
      redrawHome();
    },
  },
  homeState.cleared,
);

// `fogged`: a part of its map that Daniel tracks as Core shows fog (#310); Home marks the row quietly, with no count.
export type HomeProject = { name: string; path: string; fogged: boolean };

// The projects Home offers: one per entry of `projects.json`, in its order.
export function homeProjects(paths: string[], fogged: Set<string> = new Set()): HomeProject[] {
  return paths.map((path) => ({ name: lastSegment(path) || path, path, fogged: fogged.has(path) }));
}

// The projects the shell reports with a fogged Core part, read when Home is entered and when the window gains focus, never on a timer.
const marked = new Set<string>();
let marksStale = true;
export const staleMarks = () => void (marksStale = true);
if (typeof window !== "undefined") window.addEventListener?.("focus", staleMarks);
async function readMarks(paths: string[], redraw: () => void) {
  marksStale = false;
  const now = await invoke("map_marks", { projects: paths });
  const review = await readReviews(paths);
  const same = now.length === marked.size && now.every((path) => marked.has(path));
  if (same && !review) return;
  marked.clear();
  now.forEach((path) => marked.add(path));
  redraw();
}

// The one-time review on Home (#314): only for the projects whose Map view fell below its threshold, so an unused map still reaches
// Daniel. It lists that project's unused modules, never a count. Read with the marks, when Home is entered and on focus.
let homeReview: { modules: Module[]; projects: string[] } | null = null;
async function readReviews(paths: string[]): Promise<boolean> {
  const found: Record<string, Module[] | null> = {};
  await Promise.all(paths.map(async (path) => void (found[path] = await reviewOf(path).catch(() => null))));
  const projects = onHome(found);
  const next = projects.length ? { modules: found[projects[0]]!, projects } : null;
  const changed = JSON.stringify(next) !== JSON.stringify(homeReview);
  homeReview = next;
  return changed;
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
  // The clock counts up like the list's (#226), not frozen at the draw.
  const since = part === "running" ? session.turn_span?.start : part === "unseen" ? (session.turns.at(-1)?.ended_at ?? session.last_event_at) : undefined;
  const clock = since === undefined ? [] : [counts(text("span", "", "clock"), (now) => (part === "unseen" ? `${age(since, now)} ago` : age(since, now)))];
  first.append(logo(logoKey(session)), fullTitle(text("span", sessionName(session), "name"), sessionName(session)), text("span", project, "id"), ...clock, ...(part === "needs_you" || part === "unseen" ? [glyph(session.state, session.unseen, session.landed)] : []));
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

// The stop of each project mark's watch, so a redraw replaces the watch of the mark it replaced.
const marksSeen = new Map<string, () => void>();

function projectRow({ name, path, fogged }: HomeProject): HTMLElement {
  const button = pressable(document.createElement("button"), false, () => void startLast(path, "").catch(failure("start", "error")));
  button.dataset.key = `project ${path}`;
  button.classList.add("row");
  button.title = `Start a session in ${path}`;
  const line = text("span", "", "l1");
  line.append(text("span", name, "name"), text("span", "New session", "id"));
  if (fogged) {
    // One quiet mark, no count; pressing it opens the project's Map view and not the new session the row starts.
    const mark = text("span", "", "fog-mark");
    mark.setAttribute("role", "button");
    mark.title = "Something you track has changed, or a law is broken: open the map";
    // The mark counts as seen when it is in view in a focused window with Home open (the shared Seen), once a day is logged.
    marksSeen.get(path)?.();
    marksSeen.set(path, watchSeen(mark, { active: () => homeSeen(homeShown(), mapShown()), onSeen: () => logStop("mark-seen", path) }));
    mark.addEventListener("click", (event) => {
      event.stopPropagation();
      openMap(path);
    });
    line.append(mark);
  }
  button.append(line);
  return button;
}

// Draws Home into `box`, patched, so focus survives the list's updates; `redraw` draws it again after an expansion.
export function drawHome(box: HTMLElement, sessions: Session[], projects: string[], asked: Learning | null = null, redraw: () => void = () => {}, reask: () => void = () => {}) {
  box.classList.add("home");
  // A turn's end brings the slot back after "Not now", and asks the store again.
  if (turnEnded(sessions)) {
    // What was asked while the slot was quiet may be the item set aside or one Daniel never saw: it is not drawn or marked shown before
    // the store answers again.
    if (homeState.quiet) {
      forgetAsked();
      asked = null;
    }
    homeState.quiet = false;
    reask();
  }
  if (marksStale && projects.length) readMarks(projects, redraw).catch(failure("map_marks"));
  const parts = homeView(sessions, asked, projects, homeState, lastHeadline, marked);
  const slot = parts.find((p) => p.key === "learning")?.slot ?? null;
  inSlot = slot && slot.item.kind !== "done" && homeState.held !== slot.item ? slot.item : null;
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
    ...(part.title ? [heading(part.title, `h ${part.key}`)] : []),
    ...part.rows.map((r) => (part.key === "landed" ? sessionRow(r.session, true) : waitingRow(r, part.key, sessionProjectName(projects, found, r.session)))),
    ...(part.more ? [moreRow(part.key, part.more, redraw)] : []),
    ...(part.slot ? [slotNode(part.slot, redraw, reask)] : []),
    ...part.projects.map(projectRow),
  ];
  patch(box, [
    keyed(text("h1", new Date().toLocaleDateString("en-GB", { weekday: "long", day: "numeric", month: "long" }), "title"), "title"),
    ...(parts.some((p) => p.key === "needs_you") ? [] : [keyed(text("p", "Nothing needs you", "empty"), "none")]),
    ...parts.flatMap(drawPart),
    ...(homeReview
      ? [keyed(reviewNode(homeReview.modules, homeReview.projects, () => {
          homeReview = null;
          redraw();
        }), "review")]
      : []),
  ]);
  watchLearningShown(inSlot ? box.querySelector(`[data-key="learning ${inSlot.id}"]`) : null, inSlot, () => homeSeen(homeShown(), mapShown()));
}
