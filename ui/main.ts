// The page (spec #380, ticket #386): Treatment C's frame, a top bar over a project's map, the page's base, with the session list on the
// right. Everything read beside the map opens in the left panel over it (`panel.ts`): a session's story (`story.ts` with `questions.ts` and
// `cards.ts`), History, a turn's diff, a worker's log (`panes.ts`) and Settings. The core decides what the page shows; this file wires the
// server's events to the regions and lays the list out by the width rule in `layout.ts`.

import "./transport.js";
import { dismissNotice, onLogRecord, setSettingsErrors, showNotice } from "./notice.js";
import { withGateNotice, withWriterNotice } from "./writer-notice.js";
import { startWatching } from "./answer.js";
import { openAsk } from "./ask.js";
import { showCards } from "./cards.js";
import { recheckSeen } from "./seen.js";
import { diffRequest, newestDiff } from "./diff.js";
import { element, failure, invoke, listen, logUncaught } from "./dom.js";
import { type Actions, newestUnseen, nextNeedsYou, paletteEntries, run, stepLine, stepSession, windowBinding } from "./keys.js";
import { hover, realClock, shownGate } from "./hover.js";
import { arrange } from "./layout.js";
import { fitMap, mapEscape, openMap, projectsChanged, startMap } from "./map.js";
import "./map-history.js";
import "./map-sheet.js";
import "./map-tray.js";
import "./map-runs.js";
import { openPreFlight } from "./map-pre-flight.js";
import { startMapAgents } from "./map-agents.js";
import { drawSidebar, listSpace, showSessions, staleDots } from "./list.js";
import { deselect, pressSession, select, show } from "./selection.js";
import { diffIn, historyIn, logIn, showHistory, startPanes } from "./panes.js";
import { closePanel, panelShown, showPanel, startPanel } from "./panel.js";
import { showPalette } from "./palette.js";
import { projectOfCwd, selectedProject, sidebarOrder } from "./projects.js";
import { narrowBelow, selectedView } from "./map-model.js";
import { reload, type SettingsFiles } from "./settings.js";
import { studio } from "./state.js";
import { drawFeed, showView } from "./story.js";
import { startRailBriefs } from "./rail-brief.js";
import { startWriters } from "./writers.js";
import { jumpToNeedsYou, showToast } from "./toast.js";
import { askOnce, notifyIfAway, type Alert } from "./alerts.js";
import { finishedNote, showNote, warnContext, watchNotes } from "./notes.js";
import type { SavedProject, Selection, Session, SessionCards } from "./types.js";
import { HOVERS } from "./hovers.js";

// The list's width by the window's (`layout.ts`): in full from 1007 px, else its strip; focus mode folds it. The body is `narrow` below
// 1007 px in every mode, for what the map hides there.
function layout() {
  const { mode, list } = arrange(window.innerWidth, { focus: studio.focus });
  document.documentElement.style.setProperty("--list", `${list}px`);
  document.body.dataset.layout = mode;
  document.body.classList.toggle("narrow", window.innerWidth < narrowBelow);
  showToast();
}
logUncaught(window);
window.addEventListener("resize", layout);
// The server counts a turn seen only while the story shows its session (danielrispler/Falinks#556): in the left panel over the map (#385, #386).
const tellShown = shownGate(realClock, (shown) => invoke("rail_shown", { shown }).catch(failure("rail_shown")));
// The list's strip opens over the map on a rest of the pointer (hover.ts); a click, ⌘1 and Esc keep working. They change the body's classes,
// which the observer below hands to the machine.
const listHover = hover(realClock, (open) => document.body.classList.toggle("list-peek", open));
const syncHover = () => {
  const { classList, dataset } = document.body;
  if (classList.contains("list-peek") !== listHover.open) classList.contains("list-peek") ? listHover.toggle() : listHover.dismiss();
  recheckSeen();
  tellShown(dataset.panel === "story", false);
};
new MutationObserver(syncHover).observe(document.body, { attributes: true, attributeFilter: ["class", "data-layout", "data-panel"] });
// A mouse button held down never opens an overlay, so a drag across the edge or a text selection leaves them be.
const held = (event: PointerEvent) => event.buttons !== 0;
window.addEventListener("pointerdown", () => listHover.pressed(true), true);
for (const type of ["pointerup", "pointercancel"]) window.addEventListener(type, () => listHover.pressed(false), true);
const list = document.querySelector(".list")!;
list.addEventListener("pointerenter", (event) => {
  listHover.pressed(held(event as PointerEvent));
  listHover.enter("list");
});
list.addEventListener("pointerleave", () => listHover.leave("list"));
// Leaving focus mode from the toast.
window.addEventListener("layout", layout);
layout();

// The page's shortcuts, from `keymap.json` (danielrispler/Falinks#524). By default ⌘1 opens the list from its strip, Esc closes it and
// peels the map's layers, ⌘⇧↩ toggles focus mode. A needs-you only toasts (danielrispler/Falinks#573).
const toggle = (name: string) => document.body.classList.toggle(name);
// The keyboard spine (danielrispler/Falinks#575): ⌘] and ⌘[ step through the sessions, ⌘J goes to the next needs-you, else the next unseen, ↑
// and ↓ move in the list, 1–4 answer the docked card and ⌘⇧P opens the palette of every action here.
const inSidebar = () => sidebarOrder(studio.listed);
const selectedId = () => studio.selection?.session.id ?? null;
const go = (id: string | null) => void (id && select(id));

// Focuses and selects the visible row `by` from the focused one, else from the selected one.
function moveInList(by: 1 | -1) {
  const rows = [...element("groups").querySelectorAll<HTMLButtonElement>("button.row")].filter((row) => row.offsetParent !== null);
  const focused = rows.indexOf(document.activeElement as HTMLButtonElement);
  const current = focused === -1 ? rows.findIndex((row) => row.ariaPressed === "true") : focused;
  const at = stepLine(rows.map((row, i) => ({ classes: [...row.classList], current: i === current })), by);
  const next = at === null ? undefined : rows[at];
  next?.focus();
  next?.click(); // a click without a pointer selects (`list.ts`)
}

// The docked card's open request, if any, on screen: only then are its answer buttons drawn, and
// focus mode (`visibility`) or a closed overlay (`display`) hides the rail.
const answerable = () => {
  const dock = element("dock");
  return !dock.hidden && dock.offsetParent !== null && getComputedStyle(dock).visibility === "visible" && dock.querySelector(".answer-buttons") !== null;
};

// Chooses choice `n` of the docked card's first question not yet chosen, else a plan's or a
// permission's `n`th button. Once every question has a choice, "Send" takes focus, for Enter.
// shortcut: a multi-select takes one choice by key; more by Tab and Enter.
function answer(n: number) {
  const dock = element("dock");
  const open = [...dock.querySelectorAll("ul.choices")].find((list) => list.querySelector("button") && !list.querySelector('[aria-pressed="true"]'));
  const buttons = (open ?? dock.querySelector(".decision .answer-buttons"))?.querySelectorAll<HTMLButtonElement>("button");
  buttons?.[n - 1]?.click();
  dock.querySelector<HTMLButtonElement>(".send:not([disabled])")?.focus();
}

const actions: Actions = {
  "studio::ToggleList": () => toggle("list-open"),
  // The selected session's story, which lives in the left panel over the map (#386): shown, or closed back to the bare map with the
  // session still selected, so ⌘2 opens it again.
  "studio::ToggleRail": () => {
    const id = selectedId();
    if (panelShown()?.kind === "story") showPanel(null, true);
    else if (id) showPanel({ kind: "story", session: id });
  },
  // Ask (danielrispler/plotkeeper#125) on the selected session's newest turn: its Headline's box, in the rail, opened.
  "studio::AskAbout": () => {
    const turn = (studio.selection?.session.turns.length ?? 0) - 1;
    if (turn < 0) return;
    showPanel({ kind: "story", session: selectedId()! });
    openAsk(selectedId()!, { kind: "headline", turn });
  },
  "studio::Dismiss": () => {
    // Esc peels the map's layers (`map.ts`) and on the bare map does nothing (#386); the list or the palette open over it closes first.
    const overlaid = ["list-open", "list-peek"].some((name) => document.body.classList.contains(name)) || !element("palette").hidden;
    if (!overlaid) return mapEscape();
    document.body.classList.remove("list-open", "list-peek");
    element("palette").hidden = true;
  },
  "studio::ToggleFocus": () => {
    studio.focus = !studio.focus;
    document.body.classList.remove("list-open", "list-peek");
    layout();
  },
  "studio::NextSession": () => go(stepSession(inSidebar(), selectedId(), 1)),
  "studio::PreviousSession": () => go(stepSession(inSidebar(), selectedId(), -1)),
  // The toast's session first, which also leaves focus mode (danielrispler/Falinks#573).
  "studio::NextNeedsYou": () => void (jumpToNeedsYou() || go(nextNeedsYou(inSidebar(), selectedId()))),
  "studio::SelectNext": () => moveInList(1),
  "studio::SelectPrevious": () => moveInList(-1),
  "studio::Answer": (n) => answer(Number(n)),
  // The selected session's newest turn's whole diff (danielrispler/plotkeeper#131); none for an agent without trees.
  "studio::OpenFullDiff": () => {
    const session = studio.selection?.session;
    const turn = session ? newestDiff(session.turns, session.capabilities) : null;
    if (session && turn !== null) diffRequest.open(session.id, turn);
  },
  "studio::JumpToUnseen": () => void go(newestUnseen(inSidebar())),
  // Back to the map (#386): the left panel closes. A keymap's `studio::GoHome` is read as this (`settings.ts`).
  "studio::ShowMap": () => closePanel(),
  // The whole map in view (#383).
  "studio::FitMap": () => fitMap(),
  // Pre-flight (#394): the parts a piece of work will likely cross, framed on the map; again, it closes.
  "studio::PreFlight": () => openPreFlight(),
  // History (#94), in the left panel over the map (#385): the ended sessions, searchable.
  "studio::ShowHistory": () => showHistory(),
  // Settings (#385), in the left panel over the map; the gear in the top bar toggles it.
  "studio::ShowSettings": () => void openSettings(),
  // Asks GitHub again which tasks have landed (#53); it is otherwise asked at a turn's end and at the page's focus.
  "studio::RefreshLanded": () => void invoke("refresh_landed_tasks").catch(failure("refresh_landed_tasks")),
  "studio::Palette": () => {
    showPalette(paletteEntries(studio.files.keymap, Object.keys(actions)), (binding) => run(actions, binding));
  },
};
window.addEventListener("keydown", (event) => {
  const target = event.target as HTMLElement;
  const typing = target.isContentEditable || target.matches?.("input, textarea, select") === true;
  const inList = target.closest?.(".list") != null;
  const action = windowBinding(studio.files.keymap, event, typing, inList, !typing && answerable());
  if (!run(actions, action)) return;
  if (action !== "studio::Dismiss") event.preventDefault();
});

// `settings.json` and `keymap.json`, at start and each time one is saved. A file that does not parse
// is reported in the notice line and the last good one stays in effect.
let applied = false;
function applySettings(files: SettingsFiles) {
  // A write's returned text and then the watcher's report of the same file are one change: the second does nothing.
  if (applied && files.settings === studio.texts.settings && files.keymap === studio.texts.keymap) return;
  applied = true;
  studio.texts = files;
  studio.files = reload(studio.files, files);
  const { settings, errors } = studio.files;
  setSettingsErrors(errors);
  layout();
  const { model, effort, on_open: onOpen, writer } = settings.summaries;
  invoke("summary_settings", { model, effort, writer, onOpen }).catch(failure("summary_settings"));
  invoke("check_lists", { lists: settings.checks }).catch(failure("check_lists"));
  window.dispatchEvent(new Event("settings"));
}
// The context warning waits for both the settings and the list, so it first arms the sessions at the threshold in the
// file, not the default's (`notes.ts`).
const ready = { settings: false, list: false };
const warn = () => ready.settings && ready.list && warnContext(studio.listed, studio.files.settings.context_warning);
listen<SettingsFiles>("settings-files", (files) => {
  applySettings(files);
  warn();
});
invoke("settings_files").then((files) => {
  applySettings(files);
  ready.settings = true;
  warn();
});
// Every block in the story opens or closes, except a card's "explain more", whose first open asks the
// writer, and the "ask about this" boxes.
const blocks = () => element("story").querySelectorAll<HTMLDetailsElement>("details:not(.more):not(.ask-about)");
element("open-all").addEventListener("click", () => blocks().forEach((block) => (block.open = true)));
element("close-all").addEventListener("click", () => blocks().forEach((block) => (block.open = false)));
// The rail's "⋯" menu (danielrispler/Falinks#574) closes once an action in it is chosen.
element("rail-actions").addEventListener("click", () => element("rail-actions").hidePopover());

document.addEventListener("visibilitychange", recheckSeen);
window.addEventListener("focus", recheckSeen);
window.addEventListener("blur", recheckSeen);
window.addEventListener("focus", () => actions["studio::RefreshLanded"]());

startPanes();
startRailBriefs(drawFeed);
startMap({ selectedProject, removeProject, shown: drawSidebar });
element("pre-flight-button").addEventListener("click", () => openPreFlight());
startMapAgents({ sessions: () => studio.listed, projectOf: projectOfCwd, select: (id, agent) => void pressSession(id, agent) });
// The left panel over the map (#385): History, a turn's diff and a worker's log; closing a story or a log selects none and leaves the bare map.
startPanel({ history: historyIn, diff: diffIn, log: logIn, deselect: () => void deselect() });
const openSettings = () => showPanel({ kind: "settings" });
element("settings-button").addEventListener("click", () => (panelShown()?.kind === "settings" ? showPanel(null) : void openSettings()));
// The page's own elements whose hover another place shows too (#220); the needs-you toast's adds its key when it shows.
element("toast").title = element("note-toast").title = HOVERS.goToSession;
// A click on the session list's empty space goes back to the bare map, as `studio::ShowMap` does.
list.addEventListener("click", (event) => listSpace(event.target as HTMLElement) && closePanel());
// The needs-you chip (danielrispler/Falinks#572) selects the first session that needs Daniel.
element("needs-you").addEventListener("click", () => {
  const first = studio.listed.find((session) => session.state === "needs_you");
  if (first) select(first.id);
});
// A click outside the palette closes it.
document.addEventListener("pointerdown", (event) => {
  if (!element("palette").contains(event.target as Node)) element("palette").hidden = true;
}, true);

// The saved project list (#381): the list's headers and the map follow it; its first arrival opens the last viewed project's map (#386).
const showProjects = (projects: SavedProject[]) => {
  studio.projects = projects;
  staleDots();
  drawSidebar();
  projectsChanged();
};
function removeProject(path: string) {
  invoke("remove_project", { path }).then(showProjects).catch(failure("remove_project"));
}
listen<SavedProject[]>("projects", showProjects);
invoke("projects").then(showProjects);
element("remove-project").addEventListener("click", () => {
  const menu = element("project-actions");
  menu.hidePopover();
  if (menu.dataset.project) removeProject(menu.dataset.project);
});

const showList = (sessions: Session[]) => {
  showSessions(sessions);
  showToast();
  ready.list = true;
  warn();
};
listen<Session[]>("sessions", showList);
invoke("sessions").then(showList);
// A press on the toast jumps to its session; the press itself keeps the keyboard focus.
element("toast").addEventListener("pointerdown", (event) => event.preventDefault());
element("toast").addEventListener("click", jumpToNeedsYou);
// A turn that ended off screen in the focused window (the server's notices): the note toast names it.
watchNotes();
listen<string[]>("finished", (ids) => {
  const note = finishedNote(ids, studio.listed);
  if (note) showNote(note);
});
// While a press waits for its reply, the reply draws the rail once, with its cards; an event between
// would draw it with the last session's cards for a frame (danielrispler/Falinks#570).
// The press's settling fetches what it held back.
listen<Selection | null>("selected", (selected) => (studio.pending ? (studio.missed = true) : show(selected)));
// A session selected before the page opened opens in the left panel, its story or its worker's log, over its project's map.
invoke("selected").then((selected) => {
  show(selected);
  if (selected) openMap(projectOfCwd(selected.session.cwd));
  if (selected) showPanel(selectedView(selected.session.id, projectOfCwd(selected.session.cwd), selected.agent).panel);
});
listen<SessionCards>("cards", (cards) => (studio.pending ? (studio.missed = true) : showCards(cards)));
// Claude Code's version changed: re-record the subagent fixture.
listen<string>("notice", showNotice);
// A notice the server found due while no tab was focused (#331): the browser notification. The permission is asked once, on the first press.
listen<Alert>("notification", notifyIfAway);
window.addEventListener("pointerdown", () => void askOnce(), { once: true });
window.addEventListener("keydown", () => void askOnce(), { once: true });
// The server's errors, as a `log://log` record (`notice.ts`).
listen<{ level: number; message: string }>("log://log", onLogRecord);
element("notice-dismiss").addEventListener("click", dismissNotice);
// No card writer can run, or one can again (danielrispler/plotkeeper#51).
const editNotice = (edit: (text: string) => string) => {
  const line = element("notice");
  line.textContent = edit(line.textContent ?? "");
  line.hidden = !line.textContent;
};
const showWriterNotice = (notice: string | null) => editNotice((text) => withWriterNotice(text, notice));
listen<string | null>("writer-notice", showWriterNotice);
// A page opened after the writer failed has missed the event (danielrispler/plotkeeper#328).
// The desktop shell has no such command, so a rejection is no error there.
invoke("writer_notice")
  .then((notice) => notice && showWriterNotice(notice))
  .catch(() => {});
invoke("cards").then(showCards);
// The writer section in the Settings panel (spec #320, #387): its gate notice shares the writer's line, and a passed Test's save applies as one.
startWriters({
  summaries: () => studio.files.settings.summaries,
  texts: () => studio.texts,
  saved: (settings) => applySettings({ settings, keymap: studio.texts.keymap }),
  gate: (gated) => editNotice((text) => withGateNotice(text, gated)),
});
// The answer hook waits for the card only while its session shows (danielrispler/Falinks#461, decision 16).
startWatching(() => showView(studio.selection));
