// The window (#463 pick, mockup C): a top bar, then the session list on the right, the terminal in
// the centre and the story rail on the left. Each region is its own module: `list.ts`, `story.ts`
// with `questions.ts` and `cards.ts`, and the terminal pane, `terminal.ts` (#466). The core
// decides what the window shows; this file wires the shell's events to the regions and lays the
// columns out by the width rule in `layout.ts`.

import { startWatching } from "./answer.js";
import { markShown, showCards } from "./cards.js";
import { element, invoke, listen } from "./dom.js";
import { type Actions, doctorEntries, nextNeedsYou, paletteEntries, run, stepSession, windowBinding } from "./keys.js";
import { arrange, defaultWidths, type LayoutMode, railShown, resized, type Widths } from "./layout.js";
import { showSessions } from "./list.js";
import { select, show } from "./selection.js";
import { dragPanel, placePanel, togglePanel } from "./panel.js";
import { dragSplit, focusPane, placePanes, runDoctor, split, startPanes, toggleSplit } from "./panes.js";
import { showPalette, showPicker } from "./picker.js";
import { readPlaces, showPlaces } from "./places.js";
import { reload, type SettingsFiles, zoomed } from "./settings.js";
import { studio } from "./state.js";
import { showSkills } from "./skills.js";
import { showView } from "./story.js";
import { jumpToNeedsYou, showToast } from "./toast.js";
import type { Selection, Session, SessionCards } from "./types.js";
import { type Limits, usageText } from "./usage.js";

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
  element("rail-toggle").hidden = mode !== "overlay";
  placePanel();
  placePanes(stacked, cell());
  showToast();
}
window.addEventListener("resize", layout);
// The shell counts a turn seen only while the rail shows its session (#556), so it is told whether it
// does each time the layout or the body's classes change.
let railWas: boolean | null = null;
new MutationObserver(() => {
  const { classList, dataset } = document.body;
  const shown = railShown((dataset.layout ?? "full") as LayoutMode, classList.contains("rail-open"), classList.contains("skills-open"));
  if (shown !== railWas) invoke("rail_shown", { shown }).catch((e) => console.warn(e));
  railWas = shown;
}).observe(document.body, { attributes: true, attributeFilter: ["class", "data-layout"] });
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

// The window's shortcuts, from `keymap.json` (#524). By default ⌘1 opens the list from its strip,
// ⌘2 the rail as an overlay, Esc closes both and the picker, ⌘⇧↩ toggles focus mode, and ⌘=, ⌘- and
// ⌘0 zoom. Only focus mode's key changes the terminal's size; a needs-you only toasts (#573).
const toggle = (name: string) => document.body.classList.toggle(name);
let zoom = 1;
const setZoom = (to: number) => {
  zoom = to;
  invoke("set_zoom", { zoom }).then(layout, (e) => console.warn(e));
};
// The keyboard spine (#575): ⌘] and ⌘[ step through the sessions, ⌘J goes to the next needs-you, else the next unseen, ↑
// and ↓ move in the list, 1–4 answer the docked card, ⌘N starts a session, ⌘, opens `settings.json`
// and ⌘⇧P opens the palette of every action here.
const selectedId = () => studio.selection?.session.id ?? null;
const go = (id: string | null) => void (id && select(id));

// Focuses and selects the visible row `by` from the focused one, else from the selected one.
function moveInList(by: 1 | -1) {
  const rows = [...element("groups").querySelectorAll<HTMLButtonElement>("button.row")].filter((row) => row.offsetParent !== null);
  const focused = rows.indexOf(document.activeElement as HTMLButtonElement);
  const next = rows[(focused === -1 ? rows.findIndex((row) => row.ariaPressed === "true") : focused) + by];
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
  "studio::Dismiss": () => {
    document.body.classList.remove("list-open", "rail-open", "skills-open");
    element("picker").hidden = true;
  },
  "studio::ToggleFocus": () => {
    studio.focus = !studio.focus;
    document.body.classList.remove("list-open", "rail-open");
    layout();
  },
  // A split (#529) pins the pane shown and shows the next session beside it.
  "studio::ToggleSplit": () => void (toggleSplit() && go(stepSession(studio.listed, selectedId(), 1))),
  // The terminal panel (#542): a shell in the shown session's folder, under the terminal column.
  "studio::TogglePanel": () => togglePanel(focusPane),
  "studio::ZoomIn": () => setZoom(zoomed(zoom, 1)),
  "studio::ZoomOut": () => setZoom(zoomed(zoom, -1)),
  "studio::ResetZoom": () => setZoom(zoomed(zoom, 0)),
  "studio::NextSession": () => go(stepSession(studio.listed, selectedId(), 1)),
  "studio::PreviousSession": () => go(stepSession(studio.listed, selectedId(), -1)),
  // The toast's session first, which also leaves focus mode (#573).
  "studio::NextNeedsYou": () => void (jumpToNeedsYou() || go(nextNeedsYou(studio.listed, selectedId()))),
  "studio::SelectNext": () => moveInList(1),
  "studio::SelectPrevious": () => moveInList(-1),
  "studio::Answer": (n) => answer(Number(n)),
  "studio::NewSession": showPicker,
  "studio::OpenSettings": () => void invoke("open_settings", { editor: studio.files.settings.editor }).catch((e) => console.warn(e)),
  // The skills browser (#528), in the rail's place; it leaves focus mode, which hides the rail.
  "studio::Skills": () => {
    if (studio.focus) actions["studio::ToggleFocus"]();
    showSkills().catch((e) => console.warn(e));
  },
  // A Doctor entry from settings, by its command (#527).
  "studio::Doctor": (command) => {
    const entry = studio.files.settings.doctor.find((e) => e.command === command);
    if (entry) runDoctor(entry).catch((e) => console.warn(e));
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
  const action = windowBinding(studio.files.keymap, event, typing, inList, !typing && !inTerminal && answerable());
  if (!run(actions, action)) return;
  if (action !== "studio::Dismiss") event.preventDefault();
});

// `settings.json` and `keymap.json`, at start and each time one is saved. A file that does not parse
// is reported in the notice line and the last good one stays in effect.
function applySettings(files: SettingsFiles) {
  const previous = studio.files.settings;
  studio.files = reload(studio.files, files);
  const { settings, errors } = studio.files;
  const notice = element("notice");
  // Other notices, such as the Claude Code version's, stay beside the settings errors.
  const others = (notice.textContent ?? "").split("\n").filter((line) => line && !/^(settings|keymap)\.json: /.test(line));
  notice.textContent = [...others, ...errors].join("\n");
  if (errors.length) notice.hidden = false;
  else if (!others.length) notice.hidden = true;
  // Only a changed `zoom` resets the zoom the keys set.
  if (settings.zoom !== previous.zoom) setZoom(settings.zoom);
  invoke("notify_while_focused", { on: settings.notifications.while_focused }).catch((e) => console.warn(e));
  invoke("sounds", { on: settings.sounds }).catch((e) => console.warn(e));
  const size = settings.terminal.font_size;
  if (size === null) document.documentElement.style.removeProperty("--term-size");
  else document.documentElement.style.setProperty("--term-size", `${size}px`);
  layout();
  const { model, effort, on_open: onOpen } = settings.summaries;
  invoke("summary_settings", { model, effort, onOpen }).catch((e) => console.warn(e));
  window.dispatchEvent(new Event("settings"));
}
listen<SettingsFiles>("settings-files", applySettings);
invoke<SettingsFiles>("settings_files").then(applySettings);
element("rail-toggle").addEventListener("click", () => toggle("rail-open"));
// Every block in the story opens or closes, except a card's "explain more", whose first open asks the
// writer, and the "ask about this" boxes.
const blocks = () => element("story").querySelectorAll<HTMLDetailsElement>("details:not(.more):not(.ask-about)");
element("open-all").addEventListener("click", () => blocks().forEach((block) => (block.open = true)));
element("close-all").addEventListener("click", () => blocks().forEach((block) => (block.open = false)));
// The rail's "⋯" menu (#574) closes once an action in it is chosen.
element("rail-actions").addEventListener("click", () => element("rail-actions").hidePopover());

document.addEventListener("visibilitychange", markShown);

// A login shell in the configured repository, then a pane per session terminal (#467).
startPanes().catch((e) => element("terminal-body").append(`No terminal: ${e}`));
dragSplit();
dragPanel();
element("new-session").addEventListener("click", showPicker);
// The needs-you chip (#572) selects the first session that needs Daniel.
element("needs-you").addEventListener("click", () => {
  const first = studio.listed.find((session) => session.state === "needs_you");
  if (first) select(first.id);
});
// A click outside the picker, but not on its own button, closes it.
document.addEventListener("pointerdown", (event) => {
  const target = event.target as Node;
  if (!element("picker").contains(target) && !element("new-session").contains(target)) element("picker").hidden = true;
}, true); // capture: the terminal must not swallow it

const showList = (sessions: Session[]) => {
  showSessions(sessions);
  showPlaces();
  showToast();
};
// Opening Places reads git's counts at once (#576).
element("places").addEventListener("toggle", () => void ((element("places") as HTMLDetailsElement).open && readPlaces(true)));
listen<Session[]>("sessions", showList);
invoke<Session[]>("sessions").then(showList);
// A press on the toast jumps to its session; the press itself keeps the terminal's keyboard focus.
element("toast").addEventListener("pointerdown", (event) => event.preventDefault());
element("toast").addEventListener("click", jumpToNeedsYou);
// While a press waits for its reply, the reply draws the rail once, with its cards; an event between
// would draw it with the last session's cards for a frame (#570).
// The press's settling fetches what it held back.
listen<Selection | null>("selected", (selected) => (studio.pending ? (studio.missed = true) : show(selected)));
invoke<Selection | null>("selected").then(show);
listen<SessionCards>("cards", (cards) => (studio.pending ? (studio.missed = true) : showCards(cards)));
// Claude Code's version changed: re-record the subagent fixture.
listen<string>("notice", (notice) => {
  element("notice").textContent = notice;
  element("notice").hidden = false;
});
invoke<SessionCards>("cards").then(showCards);
// The answer hook waits for the card only while its session shows (#461, decision 16).
startWatching(() => showView(studio.selection));

// The plan's usage, from the status line's snapshot (#481). The status line runs after each message.
// shortcut: polls every 10 s; push from the shell if the delay matters.
const showUsage = () =>
  invoke<Limits | null>("usage").then((limits) => {
    element("usage").textContent = usageText(limits, Date.now() / 1000);
  });
showUsage();
setInterval(showUsage, 10_000);
