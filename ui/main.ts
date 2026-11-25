// The page (spec #380, ticket #386): Treatment C's frame, a top bar over a project's map, the page's base, with the session list on the
// right. Everything read beside the map opens in the left panel over it (`panel.ts`): a session's story (`story.ts` with `questions.ts` and
// `cards.ts`), History, a turn's diff, a worker's log (`panes.ts`) and Settings. The core decides what the page shows; this file wires the
// server's events to the regions and lays the list out by the width rule in `layout.ts`.

import "./transport.js";
import { dismissNotice, NOT_RECORDING, setSettingsErrors, showError, showNotice } from "./notice.js";
import { withGateNotice, withWriterNotice } from "./writer-notice.js";
import { startWatching } from "./answer.js";
import { forgetAsks, openAsk } from "./ask.js";
import { recheckSeen } from "./seen.js";
import { diffRequest, newestDiff } from "./diff.js";
import { element, failure, hint, invoke, listen, logUncaught } from "./dom.js";
import { type Actions, newestUnseen, nextNeedsYou, paletteEntries, run, stepLine, stepSession, windowBinding } from "./keys.js";
import { hover, realClock, shownGate } from "./hover.js";
import { arrange } from "./layout.js";
import { fitMap, mapEscape, openMap, projectsChanged, startMap } from "./map.js";
import { startMapHistory } from "./map-history.js";
import { startMapSheet } from "./map-sheet.js";
import { startMapTray } from "./map-tray.js";
import { startMapRuns } from "./map-runs.js";
import { startMicroWorld } from "./micro-world.js";
import { openBriefOf } from "./map-brief.js";
import { briefOpen } from "./map-brief-player.js";
import { openPreFlight } from "./map-pre-flight.js";
import { startMapAgents } from "./map-agents.js";
import { drawSidebar, showSessions, staleDots, startList } from "./list.js";
import { listSpace, secondLine, sessionName } from "./list-model.js";
import { deselect, pressSession, select, show } from "./selection.js";
import { diffIn, historyIn, logIn, startPanes } from "./panes.js";
import { besideSheet, closePanel, panelShown, showHistory, showPanel, startPanel } from "./panel.js";
import { showPalette } from "./palette.js";
import { projectOfCwd, selectedProject, sidebarOrder } from "./projects.js";
import { narrowBelow, selectedView } from "./map-model.js";
import { focusSend } from "./questions.js";
import { reload } from "./settings.js";
import { setFocus, setMissed, setProjects, setSettings, studio } from "./state.js";
import { askDelete, drawFeed, forgetStories, lastHeadline, showCards, showTurn, showView, startStory, turnHeadline } from "./story.js";
import { startRailBriefs } from "./rail-brief.js";
import { barKeyDown } from "./story-bar.js";
import { startWriters } from "./writers.js";
import { jumpToNeedsYou, showToast, showWhatsNewToast, watchToasts } from "./toast.js";
import { askOnce, notifyIfAway, type Alert } from "./alerts.js";
import { type Copied, finishedNote, showCopyToast, showNote, watchNotes } from "./notes.js";
import { startWhatsNew } from "./whats-new.js";
import type { SavedProject, Selection, Session, SessionCards, SettingsFiles } from "./types.js";
import { explanationsApplied, startExplanations, toggleExplanations } from "./explanations.js";

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
const list = document.querySelector<HTMLElement>(".list");
if (!list) throw new Error(".list is not on the page");
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
  focusSend(dock);
}

const named: Record<string, (arg?: string) => void> = {
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
    const session = selectedId();
    if (turn < 0 || session === null) return;
    showPanel({ kind: "story", session });
    openAsk(session, { kind: "headline", turn });
  },
  "studio::Dismiss": () => {
    // Esc peels the map's layers (`map.ts`) and on the bare map does nothing (#386); the list open over it closes first. The palette, a
    // popover, and a modal dialog close on Esc themselves (#458).
    const overlaid = ["list-open", "list-peek"].some((name) => document.body.classList.contains(name));
    if (!overlaid) return mapEscape();
    document.body.classList.remove("list-open", "list-peek");
  },
  "studio::ToggleFocus": () => {
    setFocus(!studio.focus);
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
  // The page's explanations on or off (#522), as the `?` beside an explained panel's × does.
  "studio::ToggleExplanations": () => toggleExplanations(),
  "studio::Palette": () => {
    showPalette(paletteEntries(studio.files.keymap, [...actions.keys()]), (binding) => run(actions, binding));
  },
};
// Looked up by the name `keymap.json` gives, so a Map (`keys.ts`, `Actions`).
const actions: Actions = new Map(Object.entries(named));
// Esc in a modal dialog (rename, delete, confirm) is the dialog's, as the palette's is (#458): it closes the dialog, and the window's Esc,
// which peels the map's layers, does not see it (#452's outside review).
for (const dialog of document.querySelectorAll("dialog")) {
  dialog.addEventListener("keydown", (event) => {
    if (event.key === "Escape") event.stopPropagation();
  });
}
window.addEventListener("keydown", (event) => {
  const target = event.target as HTMLElement;
  const typing = target.isContentEditable || target.matches?.("input, textarea, select") === true;
  const inList = target.closest?.(".list") != null;
  // The end-of-story bar on screen takes its digits and its skip key (#393), after a docked card's answer keys. A picked
  // explain-back mark (a radio) takes no letters, so S still skips the bar from it (#443 review).
  const choosing = target.matches?.('input[type="radio"], input[type="checkbox"]') === true;
  if (!answerable() && barKeyDown(event, typing && !choosing)) return void event.preventDefault();
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
  setSettings(files, reload(studio.files, files));
  const { settings, errors } = studio.files;
  setSettingsErrors(errors);
  layout();
  const { model, effort, on_open: onOpen, writer } = settings.summaries;
  invoke("summary_settings", { model, effort, writer, onOpen }).catch(failure("summary_settings"));
  invoke("check_lists", { lists: settings.checks }).catch(failure("check_lists"));
  explanationsApplied();
  window.dispatchEvent(new Event("settings"));
}
listen<SettingsFiles>("settings-files", applySettings);
invoke("settings_files").then(applySettings).catch(failure("settings_files"));
// The rail's "⋯" menu (danielrispler/Falinks#574) closes once an action in it is chosen.
element("rail-actions").addEventListener("click", () => element("rail-actions").hidePopover());

document.addEventListener("visibilitychange", recheckSeen);
window.addEventListener("focus", recheckSeen);
window.addEventListener("blur", recheckSeen);
window.addEventListener("focus", () => actions.get("studio::RefreshLanded")?.());

// The Map view's overlays hook into its draws and Esc's layers, in the order their imports gave them.
startMicroWorld();
startMapHistory({ openBriefOf });
startMapSheet({ besideSheet, panelShown, headline: turnHeadline, show: showTurn });
startMapTray();
startMapRuns();
startPanes(select);
startStory((id, agent) => void select(id, agent));
startList({
  pressSession,
  askDelete,
  lastHeadline,
  forget: (sessions) => {
    forgetStories(sessions);
    forgetAsks(new Set(sessions.map((session) => session.id)));
  },
});
startRailBriefs(drawFeed);
startMap({ selectedProject, removeProject, shown: drawSidebar });
element("pre-flight-button").addEventListener("click", () => openPreFlight());
startMapAgents({
  sessions: () => studio.listed,
  projectOf: projectOfCwd,
  select: (id, agent) => void pressSession(id, agent),
  briefOpen,
  name: sessionName,
  line: (session) => secondLine(session, lastHeadline),
  placed: () => {
    drawFeed();
    drawSidebar();
  },
});
// The left panel over the map (#385): History, a turn's diff and a worker's log; closing a story or a log selects none and leaves the bare map.
startPanel({ history: historyIn, diff: diffIn, log: logIn, deselect: () => void deselect() });
const openSettings = () => showPanel({ kind: "settings" });
hint(element("skip-link"), "Jump past the session list into the map");
hint(element("settings-button"), "Settings");
// The buttons the page's HTML holds, with the words their titles had.
for (const [id, words] of [
  ["remove-project", "Take this project off the list; its sessions stay in History"],
  ["rail-more", "Rename, mark unseen or delete this session"],
  ["rename", "Give this session a name of your own"],
  ["mark-unseen", "Mark this session unseen, to come back to it"],
  ["delete", "Remove this session from the list; its cards are kept"],
  ["notice-dismiss", "Hide this notice"],
  ["pre-flight-button", "Before you start: the parts this work will likely cross"],
  ["panel-close", "Close the panel (Esc)"],
  ["rename-dialog-save", "Save this name (↩)"],
  ["rename-dialog-cancel", "Keep the old name (Esc)"],
  ["delete-dialog-delete", "Remove it from the list"],
  ["delete-dialog-cancel", "Keep it (Esc)"],
  ["confirm-yes", "Go ahead; this cannot be undone"],
  ["confirm-dialog-cancel", "Keep it (Esc)"],
] as const)
  hint(element(id), words);
element("settings-button").addEventListener("click", () => (panelShown()?.kind === "settings" ? showPanel(null) : void openSettings()));
// A click on the session list's empty space goes back to the bare map, as `studio::ShowMap` does.
list.addEventListener("click", (event) => listSpace(event.target as HTMLElement) && closePanel());
// The needs-you chip (danielrispler/Falinks#572) selects the first session that needs Daniel.
element("needs-you").addEventListener("click", () => {
  const first = studio.listed.find((session) => session.state === "needs_you");
  if (first) void select(first.id);
});

// The saved project list (#381): the list's headers and the map follow it; its first arrival opens the last viewed project's map (#386).
const showProjects = (projects: SavedProject[]) => {
  setProjects(projects);
  staleDots();
  drawSidebar();
  projectsChanged();
};
function removeProject(path: string) {
  invoke("remove_project", { path }).then(showProjects).catch(failure("remove_project"));
}
listen<SavedProject[]>("projects", showProjects);
invoke("projects").then(showProjects).catch(failure("projects"));
element("remove-project").addEventListener("click", () => {
  const menu = element("project-actions");
  menu.hidePopover();
  if (menu.dataset.project) removeProject(menu.dataset.project);
});

const showList = (sessions: Session[]) => {
  showSessions(sessions);
  showToast();
};
listen<Session[]>("sessions", showList);
invoke("sessions").then(showList).catch(failure("sessions"));
// The toasts' presses keep the keyboard's focus, and a modal dialog holds them back (#457).
watchToasts(select);
// A turn that ended off screen in the focused window (the server's notices): the note toast names it.
watchNotes();
// Copy for the agent (#493): the brief's copy icons say what they copied, and the copy toast shows it.
window.addEventListener("copy-for-agent", (event) => {
  const { outcome, prompt } = (event as CustomEvent<Copied>).detail;
  showCopyToast(outcome, prompt);
});
// After an update to a new version, once (#527): "Updated to X.Y.Z" and a link to its GitHub Release.
invoke("about")
  .then(({ version }) => startWhatsNew(version, showWhatsNewToast))
  .catch(failure("about"));
listen<string[]>("finished", (ids) => {
  const note = finishedNote(ids, studio.listed);
  if (note) showNote(note);
});
// While a press waits for its reply, the reply draws the rail once, with its cards; an event between
// would draw it with the last session's cards for a frame (danielrispler/Falinks#570).
// The press's settling fetches what it held back.
listen<Selection | null>("selected", (selected) => {
  if (studio.pending) setMissed(true);
  else show(selected);
});
// A session selected before the page opened opens in the left panel, its story or its worker's log, over its project's map.
invoke("selected")
  .then((selected) => {
    show(selected);
    if (selected) openMap(projectOfCwd(selected.session.cwd));
    if (selected) showPanel(selectedView(selected.session.id, projectOfCwd(selected.session.cwd), selected.agent).panel);
  })
  .catch(failure("selected"));
listen<SessionCards>("cards", (cards) => {
  if (studio.pending) setMissed(true);
  else showCards(cards);
});
// Claude Code's version changed: re-record the subagent fixture.
listen<string>("notice", showNotice);
// A notice the server found due while no tab was focused (#331): the browser notification. The permission is asked once, on the first press.
listen<Alert>("notification", notifyIfAway);
window.addEventListener("pointerdown", () => void askOnce(), { once: true });
window.addEventListener("keydown", () => void askOnce(), { once: true });
// An error the server logged, by its message (`notice.ts`).
listen<string>("server-error", showError);
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
invoke("writer_notice")
  .then((notice) => notice && showWriterNotice(notice))
  .catch(failure("writer_notice"));
// No harness wired (#437): "not recording" in the map's head until one is. `plotkeeper install` runs in a terminal, so the page asks again at its focus.
const checkWired = () =>
  invoke("wired")
    .then((wired) => {
      const line = element("map-wiring");
      line.textContent = wired ? "" : NOT_RECORDING;
      line.hidden = wired;
    })
    .catch(failure("wired"));
checkWired();
window.addEventListener("focus", checkWired);
invoke("cards").then(showCards).catch(failure("cards"));
// The writer section in the Settings panel (spec #320, #387): its gate notice shares the writer's line, and a passed Test's save applies as one.
startWriters({
  summaries: () => studio.files.settings.summaries,
  texts: () => studio.texts,
  saved: (settings) => applySettings({ settings, keymap: studio.texts.keymap }),
  gate: (gated) => editNotice((text) => withGateNotice(text, gated)),
});
// The `?` and `studio::ToggleExplanations` write `explanations` (#522); the returned text applies at once, as a passed Test's save does.
startExplanations((settings) => applySettings({ settings, keymap: studio.texts.keymap }));
// The answer hook waits for the card only while its session shows (danielrispler/Falinks#461, decision 16).
startWatching(() => showView(studio.selection));
