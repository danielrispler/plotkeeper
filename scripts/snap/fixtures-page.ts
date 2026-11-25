// The page's frame: the session list and its projects, a worker's log, toasts, the palette, real keys and the skip link.

import type { LogEntry, Session } from "../../ui/types.ts";
import { base, type Fixture } from "./base.ts";
import { list, session, subagent, withSaved } from "./fixtures-shared.ts";

/** A subagent's log pane: the subagent's line selected, its log from the stubbed `subagent_log`. */
export const logFixture = (status: string): Fixture => {
  const agent = { ...subagent("a1", "Review the diff"), agent_type: "Explore", status, ended_at: status === "running" ? null : Date.now() - 60e3 };
  const sessions = [session("s1", "#56 · Row controls", "running", { agents: [agent] })];
  const entries: LogEntry[] = [
    { kind: "prompt", text: "Review the diff on this branch and list anything that breaks the token rules.\nReport file and line." },
    { kind: "reply", text: "I'll start with the changed files, then check each style rule against the tokens." },
    { kind: "call", id: "c1", tool: "Bash", detail: "git diff --stat" },
    { kind: "result", id: "c1", text: " ui/log.ts | 30 ++++\n ui/style.css | 26 ++++\n 2 files changed", failed: false, cut: false },
    { kind: "call", id: "c2", tool: "Read", detail: "ui/missing.ts" },
    { kind: "result", id: "c2", text: "File does not exist.", failed: true, cut: false },
    { kind: "reply", text: "The diff is small. Checking the stylesheet for raw colours next." },
    { kind: "call", id: "c3", tool: "Grep", detail: "#[0-9a-f]{3,8} ui/style.css" },
    ...(status === "running" ? [] : [
      { kind: "result", id: "c3", text: Array.from({ length: 30 }, (_, i) => `ui/style.css:${i + 1}: no match`).join("\n"), failed: false, cut: false },
      { kind: "reply", text: "No raw colours: every colour in the diff is a theme token." },
    ] satisfies LogEntry[]),
  ];
  return { ...base, sessions, selected: { session: sessions[0], agent: agent.id }, subagent_log: { entries, offset: 1 } };
};

/**
 * #458: the project "⋯" menu, placed through the popover's implicit anchor as the rail's (#447): under its button and inside the window, and
 * right over it when the button is near the window's bottom edge, where the flipped case pads the list until the header sits there.
 */
export const projectMenuScript = (nearBottom: boolean) => `
    const more = named("pokedex").querySelector(".project-more");
    // The list is padded until the button's bottom is 16 px over the window's, too near for the menu under it. In \`vh\`, as headless
    // Chrome's window takes its full height only at its first frame; in a style sheet, as the list's redraws reset its own style.
    ${nearBottom ? 'document.head.appendChild(document.createElement("style")).textContent = "#groups { padding-top: calc(100vh - 16px - " + more.getBoundingClientRect().bottom + "px); }"; await wait(50);' : ""}
    const menu = document.getElementById("project-actions");
    more.click();
    await wait(50);
    if (!menu.matches(":popover-open")) return log("snap:fail the menu did not open");
    const b = more.getBoundingClientRect(), m = menu.getBoundingClientRect();
    if (b.bottom > innerHeight) return log("snap:fail the button is outside the window: " + b.bottom);
    if (m.left < 0 || m.top < 0 || m.right > innerWidth || m.bottom > innerHeight) log("snap:fail the menu leaves the window: " + JSON.stringify(m));
    if (m.right < b.left || m.left > b.right) log("snap:fail the menu is not in line with its button: menu " + m.left + "–" + m.right + ", button " + b.left + "–" + b.right);
    ${nearBottom
      ? 'if (m.bottom > b.top + 0.5 || b.top - m.bottom > 16) log("snap:fail the menu is not right over its button: menu bottom " + m.bottom + ", button top " + b.top);'
      : 'if (m.top < b.bottom - 0.5 || m.top - b.bottom > 16) log("snap:fail the menu is not right under its button: menu top " + m.top + ", button bottom " + b.bottom);'}
    log("project menu at " + [m.left, m.top, m.right, m.bottom].map(Math.round) + "; button at " + [b.left, b.top, b.right, b.bottom].map(Math.round));
    // A real second click on its "⋯" closes it, as the rail's "⋯" does (#452's review).
    await __snapClick(more);
    await wait(50);
    if (menu.matches(":popover-open")) log("snap:fail a second click on its ⋯ left the menu open");
    log("project menu ok");`;

// The session list holding what Home held (#382): second lines on needs-you (its question), running (its Intention) and unseen (its last
// Headline) rows only, and "Clear landed" at the list's end while a session has landed.
const listLines = (): Fixture => {
  const now = Date.now();
  const intention = (text: string) => ({ ...session("x", "x", "idle").transcript, intention: text });
  const sessions = [
    session("s1", "Ship PR 286", "needs_you", {
      cwd: "/w/plotkeeper", line: "Bash gh pr merge 286",
      questions: [{ tool_use_id: "q1", agent: null, answer: "open", turn: null, questions: [{ question: "Merge PR 286 now, or wait for the snap job?", options: [] }] }],
    }),
    session("s2", "Correctness pass, #285", "running", { cwd: "/w/plotkeeper", line: "Read ui/list.ts", transcript: intention("Checking each claim in the brief against the code it cites.") }),
    session("s3", "Snap fails in Chrome, #282", "running", { cwd: "/w/plotkeeper", line: "Bash npm run snap", transcript: intention("Printing Chrome's own error to the snap log so a failure says why.") }),
    session("s4", "Stored fixtures, #280", "your_turn", {
      cwd: "/w/plotkeeper", unseen: true,
      turns: [{ started_at: now - 40 * 60e3, ended_at: now - 22 * 60e3, prompt: "Load the older briefs", id: null, message: "Older stored briefs load again; the reader accepts both shapes.", checks: [], listed: [], files: null }],
    }),
    session("s5", "Archive the Tauri app, #332", "your_turn", { cwd: "/w/plotkeeper", landed: true }),
    session("s6", "Strava-style running tracker", "your_turn", { cwd: "/w/timburr", line: "Done." }),
    session("s7", "Night Out Mode", "running", { cwd: "/w/fluffy", line: "Edit probe.ts", transcript: intention("Building the probe for the venue list.") }),
    session("s8", "#48 · Agent deadline", "ended", { cwd: "/w/plotkeeper", landed: true }),
  ];
  return { ...base, sessions, selected: null };
};
const listChecks = `
      const wait = (ms) => new Promise((done) => setTimeout(done, ms));
      const line = (id) => document.querySelector('#groups .session-row[data-key="' + id + ' "] .l2')?.textContent ?? null;
      const want = { s1: "Merge PR 286 now, or wait for the snap job?", s2: "Checking each claim in the brief against the code it cites.", s4: "Older stored briefs load again; the reader accepts both shapes.", s5: null, s6: null };
      for (const [id, text] of Object.entries(want)) if (line(id) !== text) log("snap:fail " + id + "'s second line: " + line(id));
      const second = document.querySelector('#groups .session-row[data-key="s2 "] .l2');
      if (getComputedStyle(second).fontSize !== "11.5px" || getComputedStyle(second.querySelector(".row-step")).textOverflow !== "ellipsis") log("snap:fail a second line is not 11.5px cut with an ellipsis");
      if (getComputedStyle(document.querySelector("#groups .logo")).width !== "12px") log("snap:fail the list's logo is not 12px");
      const clear = () => document.querySelector(".list .clear-landed");`;
export const listSecondLines = (): Fixture => ({
  ...listLines(),
  snapScript: `${listChecks}
      if (!clear() || clear().textContent !== "Clear landed") log("snap:fail no Clear landed at the list's end");
      const probe = document.body.appendChild(document.createElement("span"));
      probe.style.color = "var(--faint)";
      const faint = getComputedStyle(probe).color;
      probe.remove();
      if (clear() && getComputedStyle(clear()).color !== faint) log("snap:fail Clear landed is not faint, " + faint + ": " + getComputedStyle(clear()).color);`,
});
/**
 * Clear landed pressed: the landed row is hidden, the toast offers Undo, and Undo brings it back; pressed again for the screenshot.
 * The strip and the tab's title follow at once too: the cleared session, unseen here, leaves both.
 */
export const listClearLanded = (): Fixture => ({
  ...listLines(),
  sessions: (listLines().sessions as Session[]).map((s) => (s.id === "s5" ? { ...s, unseen: true } : s)),
  snapScript: `${listChecks}
      const title = document.title;
      if (!document.querySelector('#strip [data-key="strip s5"]')) log("snap:fail the landed session is not in the strip before Clear landed");
      clear().click();
      await wait(100);
      if (line("s5") !== null || document.querySelector('#groups .session-row[data-key="s5 "]')) log("snap:fail the landed row is still shown after Clear landed");
      if (document.querySelector('#strip [data-key="strip s5"]')) log("snap:fail the landed session is still in the strip after Clear landed");
      if (document.title === title) log("snap:fail the tab's title still counts the cleared session: " + title);
      if (clear()) log("snap:fail Clear landed shows with nothing landed left");
      const toast = document.getElementById("clear-toast");
      if (toast.hidden || !/Cleared 2 landed/.test(toast.textContent) || !/Undo/.test(toast.textContent)) log("snap:fail the toast: " + toast.textContent);
      toast.querySelector("button").click();
      await wait(100);
      if (!document.querySelector('#groups .session-row[data-key="s5 "]') || !clear()) log("snap:fail Undo did not bring the landed row back");
      clear().click();
      await wait(100);
      log("cleared ok");`,
});
/**
 * Real keys (#454): \`await __snapKey(key)\` presses a key as Chrome's own input, so Tab moves focus and Esc closes a native popover, which a
 * synthetic KeyboardEvent does neither of. From the page's start, Tab reaches the page's first control and Shift+Tab leaves it; Enter
 * and Space press a focused button; the arrows reach the page as trusted keys; Esc closes an open popover.
 */
export const snapRealKeys = (): Fixture => ({
  ...base,
  snapScript: `
      const wait = (ms) => new Promise((done) => setTimeout(done, ms));
      const press = async (key) => { await __snapKey(key); await wait(50); };
      const named = (node) => node?.id || node?.tagName;
      document.activeElement?.blur();
      // The page's first control is the skip link (#461).
      const first = document.querySelector("a.skip-link");
      if (!first) return log("snap:fail the page has no skip link");
      await press("Tab");
      if (document.activeElement !== first) log("snap:fail Tab from the page's start focused " + named(document.activeElement) + ", not the skip link");
      await press("Shift+Tab");
      if (document.activeElement === first) log("snap:fail Shift+Tab left focus on the skip link");
      // A button made for the check, taken out again before the hover check runs.
      const button = Object.assign(document.createElement("button"), { type: "button", textContent: "Pressed" });
      const clicks = [];
      button.addEventListener("click", (event) => clicks.push(event.isTrusted));
      document.body.append(button);
      button.focus();
      await press("Enter");
      await press("Space");
      button.remove();
      if (clicks.join() !== "true,true") log("snap:fail Enter and Space on a focused button clicked it " + clicks.join() + " (trusted per click)");
      const arrows = [];
      const heard = (event) => event.isTrusted && arrows.push(event.key);
      document.addEventListener("keydown", heard);
      await press("ArrowUp");
      await press("ArrowDown");
      document.removeEventListener("keydown", heard);
      if (arrows.join() !== "ArrowUp,ArrowDown") log("snap:fail the arrows reached the page as " + arrows.join());
      const menu = document.getElementById("project-actions");
      menu.showPopover();
      if (!menu.matches(":popover-open")) return log("snap:fail the popover did not open");
      await press("Escape");
      if (menu.matches(":popover-open")) log("snap:fail Esc left the popover open");
      log("keys ok");`,
  realKeys: true, snapDone: "keys ok",
});

/**
 * What's new (#527): a page that last showed 0.1.2 opens on 0.2.0, so one toast says "Updated to 0.2.0" with a link to that
 * version's GitHub Release, and the new version is stored; its × closes it. The screenshot is the toast open.
 */
export const whatsNewToast = (): Fixture => ({
  ...list(),
  storage: { "shown-version": "0.1.2" },
  snapScript: `
      const wait = (ms) => new Promise((done) => setTimeout(done, ms));
      await wait(300);
      const toast = document.getElementById("whats-new-toast");
      if (!toast.matches(":popover-open") || !document.getElementById("toasts").contains(toast)) return log("snap:fail the what's-new toast is not an open popover in the region");
      if (!toast.textContent.includes("Updated to 0.2.0")) log("snap:fail the toast says " + toast.textContent);
      const link = toast.querySelector("a");
      if (link?.textContent !== "What's new" || link.href !== "https://github.com/danielrispler/plotkeeper/releases/tag/v0.2.0") log("snap:fail the link: " + link?.outerHTML);
      if (!toast.querySelector("[aria-label=Dismiss]")) log("snap:fail the toast has no ×");
      if (localStorage.getItem("shown-version") !== "0.2.0") log("snap:fail the shown version is " + localStorage.getItem("shown-version"));
      log("whats new ok");`,
  snapDone: "whats new ok",
});
/**
 * The toasts from the keyboard (#457): the needs-you, the note and the Clear toasts open at once in the Notifications region, apart.
 * A real press on Undo leaves the focus where it was. From the control before the region, a real Tab reaches each main action and
 * each ×; a real Enter on a × closes its toast and puts the focus on the next toast, and the last one hands it back. A modal dialog
 * holds the toasts back until it closes. A real Enter on the note and on the needs-you toast runs their action.
 */
export const toastKeys = (): Fixture => {
  const sessions = [...(list().sessions as Session[])];
  const blocked = { ...sessions[1], id: "s5" };
  return {
    ...list(),
    select: { __by: "id", values: Object.fromEntries([...sessions, blocked].map((s) => [s.id, { session: s, agent: null }])) },
    snapScript: `
      const wait = (ms) => new Promise((done) => setTimeout(done, ms));
      const press = async (key) => { await __snapKey(key); await wait(30); };
      const byId = (id) => document.getElementById(id);
      const open = (id) => byId(id).matches(":popover-open");
      const name = (node) => (node?.closest(".toast")?.id ?? node?.id ?? node?.tagName) + " " + (node?.getAttribute?.("aria-label") ?? node?.textContent);
      document.querySelector(".list .clear-landed").click();
      await wait(50);
      document.dispatchEvent(new KeyboardEvent("keydown", { code: "Enter", key: "Enter", metaKey: true, shiftKey: true, bubbles: true }));
      await wait(100);
      window.__snapEmit("finished", ["s4"]);
      await wait(50);
      const region = byId("toasts");
      if (region.tagName !== "SECTION" || region.getAttribute("aria-label") !== "Notifications") log("snap:fail the toasts' region is not named Notifications");
      for (const id of ["toast", "note-toast", "clear-toast"]) if (!open(id) || !region.contains(byId(id))) return log("snap:fail #" + id + " is not an open popover in the region");
      const boxes = ["toast", "note-toast", "clear-toast"].map((id) => byId(id).getBoundingClientRect());
      const apart = (a, b) => a.bottom <= b.top || b.bottom <= a.top || a.right <= b.left || b.right <= a.left;
      if (!apart(boxes[0], boxes[1]) || !apart(boxes[0], boxes[2]) || !apart(boxes[1], boxes[2])) log("snap:fail two toasts overlap: " + boxes.map((b) => [b.left, b.top, b.right, b.bottom].map(Math.round)).join(" | "));
      if (boxes[1].top < boxes[0].bottom) log("snap:fail the note is not under the needs-you toast");
      const before = Object.assign(document.createElement("button"), { type: "button", textContent: "Before", title: "Where the focus was" });
      region.before(before);
      // A real press on Undo acts and leaves the focus where it was; Clear landed again brings the toast back.
      before.focus();
      await __snapClick(byId("clear-toast").querySelector("button"));
      await wait(50);
      if (open("clear-toast")) log("snap:fail the press on Undo did not undo");
      if (document.activeElement !== before) log("snap:fail a press on Undo moved the focus to " + name(document.activeElement));
      document.querySelector(".list .clear-landed").click();
      await wait(50);
      if (!open("clear-toast")) return log("snap:fail the Clear toast did not show again");
      before.focus();
      const buttons = [...region.querySelectorAll("button")];
      const reached = [];
      for (let i = 0; i < 6; i++) {
        await press("Tab");
        reached.push(document.activeElement);
      }
      if (reached.some((node, i) => node !== buttons[i])) log("snap:fail Tab reached " + reached.map(name).join(", ") + ", not " + buttons.map(name).join(", "));
      // The note's × from the keyboard: the focus goes to the next toast, the Clear toast's Undo.
      byId("note-toast").querySelector("[aria-label=Dismiss]").focus();
      await press("Enter");
      if (open("note-toast")) log("snap:fail Enter on the note's × left it open");
      if (document.activeElement !== byId("clear-toast").querySelector("button")) log("snap:fail after the note's × the focus is on " + name(document.activeElement));
      byId("clear-toast").querySelector("[aria-label=Dismiss]").focus();
      await press("Enter");
      if (open("clear-toast")) log("snap:fail Enter on the Clear toast's × left it open");
      if (document.activeElement !== byId("toast").querySelector("button")) log("snap:fail after the Clear toast's × the focus is on " + name(document.activeElement));
      await press("Tab");
      await press("Enter");
      if (open("toast")) log("snap:fail Enter on the needs-you toast's × left it open");
      if (document.activeElement !== before) log("snap:fail after the last toast's × the focus is on " + name(document.activeElement) + ", not where it was");
      // A modal dialog holds a new note back until it closes.
      const dialog = byId("delete-dialog");
      dialog.showModal();
      window.__snapEmit("finished", ["s4"]);
      await wait(50);
      if (open("note-toast")) log("snap:fail the note showed over a modal dialog");
      dialog.close();
      await wait(50);
      if (!open("note-toast")) return log("snap:fail the note did not show once the dialog closed");
      // Enter on the note selects its session.
      before.focus();
      await press("Tab");
      await press("Enter");
      if (open("note-toast") || document.body.dataset.panel !== "story") log("snap:fail Enter on the note did not open its session: panel " + document.body.dataset.panel);
      // A real Esc in Delete's question closes the question alone, and the story under it stays.
      byId("delete").click();
      await wait(50);
      await press("Escape");
      await wait(50);
      if (dialog.open) log("snap:fail a real Esc did not close Delete's question");
      if (document.body.dataset.panel !== "story") log("snap:fail a real Esc in Delete's question also closed the story: panel " + document.body.dataset.panel);
      // Enter on the needs-you toast of a new block jumps to it and leaves focus mode.
      window.__snapEmit("sessions", ${JSON.stringify([...sessions, blocked])});
      await wait(50);
      if (!open("toast")) return log("snap:fail no needs-you toast for the new block");
      before.focus();
      await press("Tab");
      await press("Enter");
      if (document.body.dataset.layout === "focus") log("snap:fail Enter on the needs-you toast did not leave focus mode");
      log("toasts ok");`,
    realKeys: true,
    realPointer: true,
    snapDone: "toasts ok",
  };
};

/**
 * The palette (#458): a real Esc, a real click outside it and its close button each close it with focus back on the element that had it
 * before ⌘⇧P; its filter is a combobox whose real ↓ moves `aria-activedescendant` to an option, and no row is a pressed button.
 */
export const paletteFocus = (): Fixture => ({
  ...list(),
  snapScript: `
      const wait = (ms) => new Promise((done) => setTimeout(done, ms));
      const palette = document.getElementById("palette");
      const before = document.getElementById("settings-button");
      const named = (node) => node?.id || node?.className || node?.tagName;
      const open = async () => {
        before.focus();
        document.dispatchEvent(new KeyboardEvent("keydown", { code: "KeyP", key: "P", metaKey: true, shiftKey: true, bubbles: true }));
        await wait(100);
        if (!palette.matches(":popover-open")) log("snap:fail the palette did not open as a popover");
      };
      const back = (how) => {
        if (palette.checkVisibility()) log("snap:fail " + how + " left the palette open");
        if (document.activeElement !== before) log("snap:fail after " + how + " focus is on " + named(document.activeElement) + ", not the settings button");
      };
      await open();
      const field = palette.querySelector("input");
      if (field?.getAttribute("role") !== "combobox") log("snap:fail the filter's role is " + field?.getAttribute("role"));
      const list = document.getElementById(field?.getAttribute("aria-controls"));
      if (list?.getAttribute("role") !== "listbox") log("snap:fail the filter controls " + named(list) + ", not a listbox");
      if (palette.querySelector("[aria-pressed]")) log("snap:fail a palette row carries aria-pressed");
      await __snapKey("ArrowDown");
      await wait(50);
      const active = document.getElementById(field?.getAttribute("aria-activedescendant"));
      if (active?.getAttribute("role") !== "option") log("snap:fail after ↓ the active descendant is " + named(active) + ", not an option");
      if (active && active !== list?.children[1]) log("snap:fail ↓ did not move to the second row");
      await __snapKey("Escape");
      await wait(100);
      back("a real Esc");
      await open();
      await __snapClick(document.querySelector(".bar-project") ?? document.querySelector("header.top"));
      await wait(100);
      back("an outside click");
      // A real click on a focusable control outside closes it and leaves the focus on that control (#452's review).
      await open();
      const outside = Object.assign(document.createElement("input"), { name: "outside", autocomplete: "off" });
      outside.setAttribute("aria-label", "A field outside the palette");
      document.querySelector("header.top").append(outside);
      await __snapClick(outside);
      await wait(100);
      if (palette.checkVisibility()) log("snap:fail a click on a field outside left the palette open");
      if (document.activeElement !== outside) log("snap:fail after a click on a field outside focus is on " + named(document.activeElement) + ", not the field");
      outside.remove();
      await open();
      const close = palette.querySelector('button[aria-label="Close"]');
      if (!close?.checkVisibility()) return log("snap:fail the palette has no visible close button");
      await __snapClick(close);
      await wait(100);
      back("the close button");
      log("palette ok");`,
  realKeys: true, realPointer: true, snapDone: "palette ok",
});

/**
 * The page's structure (#461): from the page's start the first real Tab reaches "Skip to the map", shown, and a real Enter on it puts
 * the focus in the map, the page's one `main`.
 */
export const skipLink = (): Fixture => ({
  ...list(),
  snapScript: `
      const wait = (ms) => new Promise((done) => setTimeout(done, ms));
      const named = (node) => node ? node.tagName + (node.id ? "#" + node.id : "") + " " + JSON.stringify((node.textContent ?? "").trim().slice(0, 30)) : "nothing";
      await __snapKey("Tab");
      await wait(50);
      const link = document.activeElement;
      if (link?.textContent !== "Skip to the map") return log("snap:fail the first Tab focused " + named(link) + ", not the skip link");
      const box = link.getBoundingClientRect();
      if (box.bottom <= 0 || box.top >= innerHeight || box.width < 24) log("snap:fail the focused skip link is not on screen: " + JSON.stringify(box));
      await __snapKey("Enter");
      await wait(50);
      const main = document.querySelector("main");
      if (!main?.contains(document.activeElement)) return log("snap:fail Enter on the skip link focused " + named(document.activeElement) + ", not the map");
      log("skip link ok");`,
  realKeys: true, snapDone: "skip link ok",
});

/** At 1000x800 (`…Narrow`) with the story panel open, the empty map's prompt sits beside the panel, not under it (#462). */
export const listNarrow = (): Fixture => ({
  ...list(),
  snapScript: `
      const wait = (ms) => new Promise((done) => setTimeout(done, ms));
      await wait(500);
      const panel = document.getElementById("map-panel"), draft = document.querySelector("#map .map-draft");
      if (panel.hidden || !draft) return log("snap:fail the story panel or the empty map's prompt is not shown");
      const a = panel.getBoundingClientRect(), b = draft.getBoundingClientRect();
      if (a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom) log("snap:fail the prompt (" + Math.round(b.left) + "–" + Math.round(b.right) + ") is under the left panel (" + Math.round(a.left) + "–" + Math.round(a.right) + ")");
      log("narrow ok");`,
  snapDone: "narrow ok",
});

/** A call's result scrolls on its own, so it takes focus with a name (#462). */
export const logDone = () => ({
  ...logFixture("completed"),
  snapScript: `
    const wait = (ms) => new Promise((done) => setTimeout(done, ms));
    await wait(300);
    const results = [...document.querySelectorAll(".log-result")];
    if (!results.length) return log("snap:fail the log shows no call's result");
    for (const result of results) if (result.tabIndex !== 0 || !result.getAttribute("aria-label")) log("snap:fail a call's result takes no focus or has no name: " + result.outerHTML.slice(0, 80));
    log("log ok");`,
  snapDone: "log ok",
});
/** The palette open (#220), so the hover check reaches its rows. */
export const paletteOpen = () => {
  const snapScript = `document.dispatchEvent(new KeyboardEvent("keydown", { code: "KeyP", key: "P", metaKey: true, shiftKey: true, bubbles: true }));
    await new Promise((done) => setTimeout(done, 300));
    if (!document.querySelector("#palette .row")) log("snap:fail the palette did not open");`;
  return { ...list(), snapScript };
};
/** The list with each agent's logo in each state (#158): Claude, agy, status-only Codex, another program and a shell. */
export const logos = () => {
  const agy = { capabilities: { agent: "agy", asks: false, background_tasks: false, replies: false, permission_mode: false, durations: false, transcript_version: false, transcript_model: false, context: false, turn_trees: false }, model: "gemini-3.8-flash-high" };
  const only = (program: string, extra: Partial<Session> = {}) => ({ status_only: true, capabilities: { agent: "", asks: false, background_tasks: false, replies: false, permission_mode: false, durations: false, transcript_version: false, transcript_model: false, context: false, turn_trees: false }, ...extra, identity: { issue: null, branch: null, pr: null, worktree: "", name: `${program} · plotkeeper` } });
  const sessions = [
    session("s1", "#56 · Row controls", "running", { transcript: { ...session("x", "x", "idle").transcript, context: { tokens: 90000 } } }),
    session("s2", "agy · Name from the prompt", "needs_you", agy),
    session("s3", "#54 · Subagent terminal", "your_turn", { unseen: true }),
    session("s4", "codex · plotkeeper", "running", only("codex")),
    session("s5", "aider · plotkeeper", "your_turn", only("aider")),
    session("s6", "plotkeeper", "idle", only("zsh", { shell: true })),
    session("s7", "#55 · Delete default", "idle", agy),
  ];
  // The idle group is folded by default: open it, so the idle rows show.
  const snapScript = `[...document.querySelectorAll("summary, button")].find((el) => /^Idle/.test(el.textContent.trim()))?.click();`;
  return { ...base, sessions, selected: { session: sessions[1], agent: null }, snapScript };
};
/** Errors in the notice line (#191): two errors and the dismiss control, beside a settings error. */
export const noticeErrors = () => {
  const snapScript = `
    await new Promise((r) => setTimeout(r, 100));
    const notice = document.getElementById("notice");
    const box = notice.getBoundingClientRect();
    const button = document.getElementById("notice-dismiss").getBoundingClientRect();
    if (notice.hidden || notice.textContent.split("\\n").length !== 3) log("snap:fail the notice line does not hold the settings error and the two errors");
    if (button.right > innerWidth || box.right > button.left) log("snap:fail the notice line or its dismiss control is clipped");`;
  const appErrors = [
    "save_cards: the card store did not save (No space left on device)",
    "delete_session: could not remove the worktree /w/plotkeeper/.claude/worktrees/issue-191-notice-errors-with-a-very-long-name",
  ];
  return { ...list(), settings_files: { settings: "{ bad", keymap: null }, appErrors, snapScript };
};
/**
 * No harness wired (#437): a first start declined, so no sessions and no story open; the base Map view still shows the
 * not-recording line, naming `plotkeeper install`.
 */
export const notRecording = () => ({
  ...base,
  wired: false,
  snapScript: `
    await new Promise((r) => setTimeout(r, 100));
    const shown = [...document.querySelectorAll("[role=status]")].filter((el) => el.checkVisibility() && /^Not recording: .*\`plotkeeper install\`/.test(el.textContent));
    if (shown.length !== 1) log("snap:fail the not-recording line is not visible once on the base Map view: " + shown.length);`,
});
/** A harness wired (#437): no not-recording line anywhere on the page. */
export const recording = () => ({
  ...base,
  wired: true,
  snapScript: `
    await new Promise((r) => setTimeout(r, 100));
    if (document.body.textContent.includes("Not recording")) log("snap:fail a not-recording line is on the page");`,
});
/** Alerts: two sessions' turns ended off screen (the note toast). The contrast pass (#455) runs here too: the toast's buttons. */
export const alerts = () => {
  const context = (k: number) => ({ ...session("x", "x", "idle").transcript, context: { tokens: k * 1000 } });
  const sessions = [
    session("s1", "#56 · Row controls", "running", { transcript: context(34) }),
    session("s2", "#57 · Name from the prompt", "running", { transcript: context(12) }),
    session("s3", "#54 · Subagent terminal", "your_turn", { unseen: true, transcript: { ...context(20), intention: "The subagent's terminal opens beside its parent's" } }),
    session("s4", "#55 · Delete default", "your_turn", { unseen: true }),
  ];
  const snapScript = `
    window.__snapEmit("finished", ["s3", "s4"]);
    await new Promise((done) => setTimeout(done, 50));
    const note = document.getElementById("note-toast");
    if (note.hidden || !note.textContent.includes("2 finished")) log("snap:fail the note toast should say 2 finished");
    // The top bar has no unseen chip (#386): the unseen rows carry their mark in the list.
    if (document.getElementById("unseen")) log("snap:fail the top bar still has an unseen chip");
    log("note: " + note.textContent);`;
  return { ...base, sessions, selected: { session: sessions[1], agent: null }, snapScript, contrast: true };
};
/**
 * The saved project list (#381): a project with no sessions keeps its header, after the one with sessions; a gone one is
 * struck through; a header's "⋯" offers Remove project.
 */
export const projectsSaved = () =>
  withSaved(`
    const order = heads().map((head) => head.querySelector(".name").textContent).join();
    if (order !== "plotkeeper,falinks,pokedex") log("snap:fail the headers' order: " + order);
    if (named("falinks")?.closest(".project").querySelector(".row")) log("snap:fail a project with no sessions shows rows");
    if (!named("pokedex")?.classList.contains("gone")) log("snap:fail the gone project's header is not marked gone");
    // Remove project is offered only for a project with no live session: one with sessions would come back with their next event.
    if (named("plotkeeper").querySelector(".project-more")) log("snap:fail a project with live sessions offers Remove project");
    named("pokedex").querySelector(".project-more").click();
    await wait(100);
    const menu = document.getElementById("project-actions");
    if (!menu.matches(":popover-open") || menu.textContent.trim() !== "Remove project") log("snap:fail the header's menu does not offer Remove project");
    log("headers " + order);`);

/**
 * The hints (spec #582, ticket 1), with real input: the pointer on the top bar's gear opens its hint after the delay, beside the gear, and
 * on the hint it stays open; Esc closes it; Tab to the gear opens it too; and sliding from one session row to the next opens the next
 * row's hint at once, closing the first.
 */
export const hints = (): Fixture => ({
  ...list(),
  snapScript: `
      const wait = (ms) => new Promise((done) => setTimeout(done, ms));
      const open = (hint) => hint.matches(":popover-open");
      const hintOf = (node) => document.getElementById(node.getAttribute("interestfor"));
      // The hint sits beside its trigger: clear of it, within a few lines of it, on the side the window leaves room for.
      const beside = (hint, trigger) => {
        const [h, t] = [hint.getBoundingClientRect(), trigger.getBoundingClientRect()];
        const apart = Math.max(h.top - t.bottom, t.top - h.bottom);
        return apart >= 0 && apart <= 24 && h.right > t.left && h.left < t.right;
      };
      const gear = document.getElementById("settings-button");
      const gearHint = hintOf(gear);
      if (!gearHint?.matches('[popover="hint"]')) return log("snap:fail the gear has no hint");
      await __snapMove(gear);
      await wait(100);
      if (open(gearHint)) log("snap:fail the gear's hint was open before its delay");
      await wait(450);
      if (!open(gearHint)) return log("snap:fail the gear's hint did not open after hovering");
      if (!beside(gearHint, gear)) log("snap:fail the gear's hint is not beside the gear " + JSON.stringify([gearHint.getBoundingClientRect(), gear.getBoundingClientRect()]));
      await __snapMove(gearHint);
      await wait(300);
      if (!open(gearHint)) log("snap:fail the gear's hint closed with the pointer on it");
      await __snapKey("Escape");
      await wait(50);
      if (open(gearHint)) log("snap:fail Esc left the gear's hint open");
      // Tab: the skip link first, then the gear (the Needs you chip is a button too, so the Tab may take two more).
      const rows = [...document.querySelectorAll(".list .row")].slice(0, 2);
      await __snapMove(document.body);
      for (let tab = 0; tab < 4 && document.activeElement !== gear; tab++) await __snapKey("Tab");
      if (document.activeElement !== gear) return log("snap:fail Tab did not reach the gear");
      await wait(600);
      if (!open(gearHint)) log("snap:fail Tab to the gear did not open its hint");
      await __snapKey("Escape");
      document.activeElement.blur();
      // Sliding from one row to the next: the second shows at once, the first closes.
      const [first, second] = rows;
      await __snapMove(first);
      await wait(600);
      if (!open(hintOf(first))) return log("snap:fail the first row's hint did not open");
      await __snapMove(second);
      await wait(120);
      if (!open(hintOf(second))) log("snap:fail the neighbouring row's hint did not open at once");
      if (open(hintOf(first))) log("snap:fail the first row's hint stayed open beside the second's");
      log("hints ok");`,
  realKeys: true, realPointer: true, snapDone: "hints ok",
});

/**
 * Cut text scrolls in place (spec #582, ticket 3), with real input: the pointer on a second line the row cuts moves its text inside its
 * own box, and on one that fits it moves nothing. The text stays the element's own, whole.
 */
export const cutText = (): Fixture => ({
  ...listLines(),
  sessions: (listLines().sessions as Session[]).map((s) => (s.id === "s7" ? { ...s, transcript: { ...s.transcript, intention: "Probing." } } : s)),
  snapScript: `
      const wait = (ms) => new Promise((done) => setTimeout(done, ms));
      const steps = [...document.querySelectorAll(".list .row-step.cut")];
      const cuts = steps.filter((node) => node.scrollWidth > node.clientWidth);
      const fits = steps.filter((node) => node.scrollWidth <= node.clientWidth);
      if (!cuts.length || !fits.length) return log("snap:fail the list shows no cut second line or no fitting one: " + cuts.length + ", " + fits.length);
      const moved = (node) => getComputedStyle(node.firstElementChild).translate !== "none";
      if (steps.some((node) => node.hasAttribute("title") || node.textContent !== node.firstElementChild.textContent)) log("snap:fail a second line holds its text outside its box or has a title");
      await __snapMove(cuts[0]);
      await wait(600);
      if (!moved(cuts[0])) log("snap:fail a cut second line did not scroll on hover");
      await __snapMove(fits[0]);
      await wait(600);
      if (moved(fits[0])) log("snap:fail a second line that fits scrolled on hover");
      if (moved(cuts[0])) log("snap:fail a cut second line stayed scrolled after the pointer left");
      log("cut text ok");`,
  realPointer: true, snapDone: "cut text ok",
});
