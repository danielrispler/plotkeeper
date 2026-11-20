// The window (danielrispler/Falinks#463 pick, mockup C): a top bar, then the session list on the right, the terminal in
// the centre and the story rail on the left. Each region is its own module: `list.ts`, `story.ts`
// with `questions.ts` and `cards.ts`, and the terminal pane, `terminal.ts` (danielrispler/Falinks#466). The core
// decides what the window shows; this file wires the shell's events to the regions and lays the
// columns out by the width rule in `layout.ts`.

import { dismissNotice, onLogRecord, setSettingsErrors, showNotice } from "./notice.js";
import { withWriterNotice } from "./writer-notice.js";
import { startWatching } from "./answer.js";
import { openAsk } from "./ask.js";
import { placeBar, watchBar } from "./bar.js";
import { markShown, showCards } from "./cards.js";
import { diffRequest, newestDiff } from "./diff.js";
import { element, failure, invoke, listen, logUncaught } from "./dom.js";
import { type Actions, doctorEntries, newestUnseen, nextNeedsYou, paletteEntries, run, stepLine, stepSession, titled, windowBinding } from "./keys.js";
import { hover, realClock, shownGate } from "./hover.js";
import { arrange, defaultWidths, type LayoutMode, railShown, resized, type Widths } from "./layout.js";
import { listSpace } from "./home.js";
import { showSessions } from "./list.js";
import { goHome, select, show } from "./selection.js";
import { dragPanel, placePanel, togglePanel } from "./panel.js";
import { dragSplit, focusPane, placePanes, runDoctor, showHistory, split, startPanes, toggleSplit } from "./panes.js";
import { showAddProject, showBranches, showPalette, showPicker } from "./picker.js";
import { fetchNow, foundWorkspaces, pullAll, showWorkspaces, sidebarOrder, watchWorkspaces } from "./workspaces.js";
import { reload, reloadedZoom, type SettingsFiles, zoomed } from "./settings.js";
import { knownProjects } from "./projects.js";
import { studio } from "./state.js";
import { closeSettingsPage, settingsPageOpen, showSettingsPage, startSettingsPage } from "./settings-page.js";
import { showSkills } from "./skills.js";
import { showView } from "./story.js";
import { jumpToNeedsYou, showToast } from "./toast.js";
import { finishedNote, showNote, warnContext, watchNotes } from "./notes.js";
import type { Selection, Session, SessionCards } from "./types.js";
import { readUsage, timedAgents } from "./usage.js";

// The terminal's cell, measured from a probe in its font, sets the columns' layout.
function cell(): number {
  const probe = element("probe");
  return probe.getBoundingClientRect().width / (probe.textContent ?? "").length;
}

// The list's and the rail's widths, as their dividers last left them.
const widths: Widths = { ...defaultWidths, ...JSON.parse(localStorage.getItem("widths") ?? "{}") };

// The list's open width is the one its divider left; the rail's is the width rule's.
function layout() {
  const { mode, rail, stacked } = arrange(window.innerWidth, cell(), widths, { focus: studio.focus, split: split() });
  document.documentElement.style.setProperty("--list", `${widths.list}px`);
  document.documentElement.style.setProperty("--rail", `${rail || widths.rail}px`);
  document.body.dataset.layout = mode;
  placeBar();
  placePanel();
  placePanes(stacked, cell());
  showToast();
}
logUncaught(window);
window.addEventListener("resize", layout);
// The shell counts a turn seen only while the rail shows its session (danielrispler/Falinks#556), so it is told whether it
// does each time the layout or the body's classes change. A rail that shows as an overlay is told only after it has stayed
// open 2 seconds, so a peek does not clear the unseen mark (danielrispler/plotkeeper#128).
const tellShown = shownGate(realClock, (shown) => invoke("rail_shown", { shown }).catch(failure("rail_shown")));
// The overlays open on a rest of the pointer (hover.ts): the rail's layer from the spine or the session header, the list's strip
// over the terminal. A click, ⌘2 and Esc keep working; they change the body's classes, which the observer below hands to the machines.
const railHover = hover(realClock, (open) => document.body.classList.toggle("rail-open", open));
const listHover = hover(realClock, (open) => document.body.classList.toggle("list-peek", open));
const syncHover = () => {
  const { classList, dataset } = document.body;
  for (const [machine, name] of [[railHover, "rail-open"], [listHover, "list-peek"]] as const) {
    if (classList.contains(name) !== machine.open) classList.contains(name) ? machine.toggle() : machine.dismiss();
  }
  const mode = (dataset.layout ?? "full") as LayoutMode;
  tellShown(railShown(mode, classList.contains("rail-open"), classList.contains("skills-open")), mode === "overlay");
};
new MutationObserver(syncHover).observe(document.body, { attributes: true, attributeFilter: ["class", "data-layout"] });
// A mouse button held down never opens an overlay, so a drag across the edge or a text selection leaves them be.
const held = (event: PointerEvent) => event.buttons !== 0;
window.addEventListener("pointerdown", (event) => {
  railHover.pressed(true);
  listHover.pressed(true);
  // A press outside the rail's layer and its targets closes it.
  const inside = [".rail", "#spine", "#session-header"].some((selector) => (event.target as Element).closest?.(selector));
  if (!inside && document.body.dataset.layout === "overlay") document.body.classList.remove("rail-open");
}, true);
for (const type of ["pointerup", "pointercancel"]) window.addEventListener(type, () => { railHover.pressed(false); listHover.pressed(false); }, true);
const hoverZone = (machine: typeof railHover, target: Element, zone: string, only?: LayoutMode) => {
  target.addEventListener("pointerenter", (event) => {
    if (only && document.body.dataset.layout !== only) return;
    machine.pressed(held(event as PointerEvent));
    machine.enter(zone);
  });
  target.addEventListener("pointerleave", () => machine.leave(zone));
};
hoverZone(railHover, element("spine"), "spine", "overlay");
hoverZone(railHover, element("session-header"), "header", "overlay");
hoverZone(railHover, document.querySelector(".rail")!, "layer");
hoverZone(listHover, document.querySelector(".list")!, "list");
// Leaving focus mode from the toast, and a split opening or closing (`panes.ts`).
window.addEventListener("layout", layout);
layout();

// Dragging a divider resizes its column. The grid runs right to left, so the list's width is
// measured from the window's right edge and the rail's from its left.
function divider(id: string, column: keyof Widths, width: (x: number) => number) {
  const handle = element(id);
  handle.addEventListener("pointerdown", (event) => {
    event.preventDefault(); // no text selection while dragging
    handle.setPointerCapture(event.pointerId);
    handle.classList.add("dragging");
  });
  handle.addEventListener("pointermove", (event) => {
    if (!handle.hasPointerCapture(event.pointerId)) return;
    const mode = (document.body.dataset.layout ?? "full") as LayoutMode;
    Object.assign(widths, resized(column, width(event.clientX), widths, mode, window.innerWidth, cell()));
    layout();
  });
  handle.addEventListener("lostpointercapture", () => {
    handle.classList.remove("dragging");
    localStorage.setItem("widths", JSON.stringify(widths));
  });
}
divider("list-divider", "list", (x) => window.innerWidth - x);
divider("rail-divider", "rail", (x) => x);

// The window's shortcuts, from `keymap.json` (danielrispler/Falinks#524). By default ⌘1 opens the list from its strip,
// ⌘2 the rail as an overlay, Esc closes both and the picker, ⌘⇧↩ toggles focus mode, and ⌘=, ⌘- and
// ⌘0 zoom. Only focus mode's key changes the terminal's size; a needs-you only toasts (danielrispler/Falinks#573).
const toggle = (name: string) => document.body.classList.toggle(name);
let zoom = 1;
const setZoom = (to: number) => {
  zoom = to;
  invoke("set_zoom", { zoom }).then(layout, failure("set_zoom"));
};
// The keyboard spine (danielrispler/Falinks#575): ⌘] and ⌘[ step through the sessions, ⌘J goes to the next needs-you, else the next unseen, ↑
// and ↓ move in the list, 1–4 answer the docked card, ⌘N starts a session, ⌘, opens the settings page
// and ⌘⇧P opens the palette of every action here.
const inSidebar = () => sidebarOrder(knownProjects(), foundWorkspaces(), studio.listed);
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
  "studio::ToggleRail": () => toggle("rail-open"),
  // Ask (danielrispler/plotkeeper#125) on the selected session's newest turn: its Headline's box, in the rail, opened.
  "studio::AskAbout": () => {
    const turn = (studio.selection?.session.turns.length ?? 0) - 1;
    if (turn < 0) return;
    document.body.classList.add("rail-open");
    openAsk(selectedId()!, { kind: "headline", turn });
  },
  "studio::Dismiss": () => {
    // The settings page, a dialog over the window, closes first and leaves the rest as it was.
    if (settingsPageOpen()) return closeSettingsPage();
    document.body.classList.remove("list-open", "list-peek", "rail-open", "skills-open");
    element("picker").hidden = true;
  },
  "studio::ToggleFocus": () => {
    studio.focus = !studio.focus;
    document.body.classList.remove("list-open", "list-peek", "rail-open");
    layout();
  },
  // A split (danielrispler/Falinks#529) pins the pane shown and shows the next session beside it.
  "studio::ToggleSplit": () => void (toggleSplit() && go(stepSession(inSidebar(), selectedId(), 1))),
  // The terminal panel (danielrispler/Falinks#542): a shell in the shown session's folder, under the terminal column.
  "studio::TogglePanel": () => togglePanel(focusPane),
  "studio::ZoomIn": () => setZoom(zoomed(zoom, 1)),
  "studio::ZoomOut": () => setZoom(zoomed(zoom, -1)),
  "studio::ResetZoom": () => setZoom(zoomed(zoom, 0)),
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
  "studio::NewSession": () => void showPicker(),
  "studio::NewTerminal": () => void showPicker("terminal"),
  // The main checkout's branch picker (#95).
  "studio::SwitchBranch": () => void showBranches(),
  // Add project (#98): the folder dialog and the suggested repositories.
  "studio::AddProject": () => void showAddProject(),
  // Home, in the terminal column: selects no session.
  "studio::JumpToUnseen": () => void go(newestUnseen(inSidebar())),
  "studio::GoHome": () => void goHome(),
  // History (#94), in the terminal column: the ended sessions, searchable.
  "studio::ShowHistory": () => showHistory(),
  // Fetches the repository now (#33); the workspaces' states follow.
  "studio::Fetch": () => void fetchNow(),
  // Pull main (#164): every project's main checkout's default branch fast-forwards to origin's.
  "studio::PullMain": () => void pullAll(),
  // Asks GitHub again which tasks have landed (#53); it is otherwise asked at a turn's end and at the window's focus.
  "studio::RefreshLanded": () => void invoke("refresh_landed_tasks").catch(failure("refresh_landed_tasks")),
  // Finder opens with the Studio log selected.
  "studio::RevealLog": () => void invoke("reveal_log").catch(failure("reveal_log", "error")),
  // The settings page (#195); the file itself is `studio::OpenSettingsFile`.
  "studio::OpenSettings": () => showSettingsPage(),
  "studio::OpenSettingsFile": () => void invoke("open_settings", { editor: studio.files.settings.editor }).catch(failure("open_settings", "error")),
  // The skills browser (danielrispler/Falinks#528), in the rail's place; it leaves focus mode, which hides the rail.
  "studio::Skills": () => {
    if (studio.focus) actions["studio::ToggleFocus"]();
    void showSkills();
  },
  // A Doctor entry from settings, by its command (danielrispler/Falinks#527).
  "studio::Doctor": (command) => {
    const entry = studio.files.settings.doctor.find((e) => e.command === command);
    if (entry) runDoctor(entry).catch(failure("doctor", "error"));
  },
  "studio::Palette": () => {
    const { keymap, settings } = studio.files;
    // The Doctor is listed once per entry, with its warning, not as one bare action.
    const names = Object.keys(actions).filter((name) => name !== "studio::Doctor");
    showPalette([...paletteEntries(keymap, names), ...doctorEntries(settings.doctor)], (binding) => run(actions, binding));
  },
};
window.addEventListener("keydown", (event) => {
  // The terminal types into xterm's hidden textarea, so this covers it too.
  const target = event.target as HTMLElement;
  const typing = target.isContentEditable || target.matches?.("input, textarea, select") === true;
  const inList = target.closest?.(".list") != null;
  const inTerminal = target.closest?.("#terminal") != null;
  const action = windowBinding(studio.files.keymap, event, typing, inList, !typing && !inTerminal && answerable(), settingsPageOpen());
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
  const previous = studio.files.settings;
  studio.texts = files;
  studio.files = reload(studio.files, files);
  const { settings, errors } = studio.files;
  element("settings-gear").title = titled(studio.files.keymap, "Open settings", "studio::OpenSettings");
  element("new-session").title = titled(studio.files.keymap, "Start a new session", "studio::NewSession");
  element("settings-close").title = titled(studio.files.keymap, "Close settings", "studio::Dismiss");
  setSettingsErrors(errors);
  // Only a changed `zoom` resets the zoom the keys set.
  const to = reloadedZoom(zoom, previous, settings);
  if (to !== zoom) setZoom(to);
  invoke("sounds", { on: settings.sounds }).catch(failure("sounds"));
  const size = settings.terminal.font_size;
  if (size === null) document.documentElement.style.removeProperty("--term-size");
  else document.documentElement.style.setProperty("--term-size", `${size}px`);
  layout();
  const { model, effort, on_open: onOpen } = settings.summaries;
  const selfTest = Date.parse(settings.quiz_self_test ?? "");
  invoke("summary_settings", { model, effort, onOpen, selfTest: Number.isNaN(selfTest) ? null : selfTest }).catch(failure("summary_settings"));
  invoke("check_lists", { lists: settings.checks }).catch(failure("check_lists"));
  window.dispatchEvent(new Event("settings"));
}
startSettingsPage(applySettings);
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
// The session's header in the top bar toggles the rail, as ⌘2 does (danielrispler/plotkeeper#127).
// In the overlay layout a click pins the layer open, and a click on a pinned one closes it (hover.ts).
element("session-header").addEventListener("click", () => (document.body.dataset.layout === "overlay" ? railHover.click() : toggle("rail-open")));
element("spine").addEventListener("click", () => railHover.click());
// The top bar's "⋯" menu closes once an item in it is pressed.
element("bar-menu").addEventListener("click", () => element("bar-menu").hidePopover());
watchBar();
// Every block in the story opens or closes, except a card's "explain more", whose first open asks the
// writer, and the "ask about this" boxes.
const blocks = () => element("story").querySelectorAll<HTMLDetailsElement>("details:not(.more):not(.ask-about)");
element("open-all").addEventListener("click", () => blocks().forEach((block) => (block.open = true)));
element("close-all").addEventListener("click", () => blocks().forEach((block) => (block.open = false)));
// The rail's "⋯" menu (danielrispler/Falinks#574) closes once an action in it is chosen.
element("rail-actions").addEventListener("click", () => element("rail-actions").hidePopover());

document.addEventListener("visibilitychange", markShown);
window.addEventListener("focus", () => actions["studio::RefreshLanded"]());

// A login shell in the configured repository, then a pane per session terminal (danielrispler/Falinks#467).
startPanes().catch((e) => {
  failure("terminal", "error")(e);
  element("terminal-body").append(`No terminal: ${e}`);
});
dragSplit();
dragPanel();
element("new-session").addEventListener("click", () => void showPicker());
element("add-project").addEventListener("click", () => void showAddProject());
element("settings-gear").addEventListener("click", () => void run(actions, "studio::OpenSettings"));
// A click on the session list's empty space selects none and shows Home.
document.querySelector(".list")!.addEventListener("click", (event) => listSpace(event.target as HTMLElement) && void goHome());
// The needs-you chip (danielrispler/Falinks#572) selects the first session that needs Daniel.
element("needs-you").addEventListener("click", () => {
  const first = studio.listed.find((session) => session.state === "needs_you");
  if (first) select(first.id);
});
// The unseen chip selects the newest unseen session, as ⌘⇧U does.
element("unseen").addEventListener("click", () => actions["studio::JumpToUnseen"]());
// A click outside the picker, but not on its own buttons, closes it.
document.addEventListener("pointerdown", (event) => {
  const target = event.target as Node;
  const opener = ["new-session", "add-project"].some((id) => element(id).contains(target));
  if (!element("picker").contains(target) && !opener) element("picker").hidden = true;
}, true); // capture: the terminal must not swallow it

const showList = (sessions: Session[]) => {
  showSessions(sessions);
  showWorkspaces();
  showToast();
  ready.list = true;
  warn();
};
watchWorkspaces();
listen<Session[]>("sessions", showList);
invoke("sessions").then(showList);
// A press on the toast jumps to its session; the press itself keeps the terminal's keyboard focus.
element("toast").addEventListener("pointerdown", (event) => event.preventDefault());
element("toast").addEventListener("click", jumpToNeedsYou);
// A turn that ended off screen in the focused window (the shell's notices): the note toast names it.
watchNotes();
listen<string[]>("finished", (ids) => {
  const note = finishedNote(ids, studio.listed);
  if (note) showNote(note);
});
// While a press waits for its reply, the reply draws the rail once, with its cards; an event between
// would draw it with the last session's cards for a frame (danielrispler/Falinks#570).
// The press's settling fetches what it held back.
listen<Selection | null>("selected", (selected) => (studio.pending ? (studio.missed = true) : show(selected)));
invoke("selected").then(show);
listen<SessionCards>("cards", (cards) => (studio.pending ? (studio.missed = true) : showCards(cards)));
// Claude Code's version changed: re-record the subagent fixture.
listen<string>("notice", showNotice);
// The app's errors, from the log plugin's `Webview` target (#191).
listen<{ level: number; message: string }>("log://log", onLogRecord);
element("notice-dismiss").addEventListener("click", dismissNotice);
// No card writer can run, or one can again (danielrispler/plotkeeper#51).
listen<string | null>("writer-notice", (notice) => {
  const line = element("notice");
  line.textContent = withWriterNotice(line.textContent ?? "", notice);
  line.hidden = !line.textContent;
});
invoke("cards").then(showCards);
// The answer hook waits for the card only while its session shows (danielrispler/Falinks#461, decision 16).
startWatching(() => showView(studio.selection));

// Each agent's plan usage (danielrispler/plotkeeper#163): the load, the focus and a 10 s timer read only Claude's snapshot, which
// runs no CLI; every agent is read only when the popover opens, so the studio runs no agent CLI behind the user's back.
// shortcut: polls Claude's snapshot every 10 s; push from the shell if the delay matters.
const readUsageOf = (agents: string[] | null) => readUsage(element("usage"), element("usage-popover"), agents);
readUsageOf(timedAgents);
addEventListener("focus", () => readUsageOf(timedAgents));
setInterval(() => readUsageOf(timedAgents), 10_000);
element("usage-popover").addEventListener("toggle", (event) => {
  if ((event as ToggleEvent).newState === "open") readUsageOf(null);
});
