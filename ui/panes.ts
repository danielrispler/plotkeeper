// The terminal panes (#467): one per daemon terminal, in `#terminal-body`, one shown at a time. The
// login shell `shell` in the configured repository shows while no session is selected. A session
// started from the studio runs `claude` in its own terminal, whose fresh id reaches its hooks as
// `STUDIO_TERMINAL_ID`, so the core links its row to the pane (`Session.terminal`).
// `closePane` ends a terminal and drops its pane (#469).
// #470: at start every running terminal is attached, its screen replayed, and the terminals a dead
// daemon ran come back (`revive`); so after a crash, a reboot or an upgrade, the same way.
// #529: a split pins the pane shown to the column's second half while the selection drives the first;
// while both are the same terminal, one pane shows.

import type * as Xterm from "@xterm/xterm";
import { connect, list, onEvent, request, type Listed } from "./daemon.js";
import { element, invoke, pressable, text } from "./dom.js";
import { type Box, paneBoxes } from "./layout.js";
import { offerBuilds, upgrade, type Revive } from "./restart.js";
import { show as showSelection } from "./selection.js";
import type { DoctorEntry } from "./settings.js";
import { studio } from "./state.js";
import { sessionName } from "./list.js";
import { openPane, startNotice } from "./terminal.js";
import { showStartToast } from "./toast.js";
import type { Selection, Session } from "./types.js";

type Pane = { box: HTMLElement; term: Xterm.Terminal | null; title: string };
const panes = new Map<string, Pane>();
let repo = "";
let shown = "";
let ready: Promise<void> | null = null;
let revived: Promise<void> = Promise.resolve();
// The split's pinned terminal, the first pane's share of the column, and, as the width rule last
// laid it out, whether the panes stack and the terminal's cell width.
let pinned: string | null = null;
let share = 0.5;
let stacked = false;
let cell = 8;
let laidSplit = false;

// Connects, shows the login shell in the configured repository, then brings back every terminal;
// again each time the daemon is lost.
export function startPanes(): Promise<void> {
  offerBuilds();
  ready = connect().then((config) => {
    repo = config.repo;
    show("shell", () => openPane(box("shell"), "shell", repo));
    revived = revive(config.outdated);
  });
  window.addEventListener("start-failed", (e) => failedAtStart((e as CustomEvent).detail));
  onEvent((event) => {
    if (event.ev !== "lost") return;
    revived = connect().then((config) => revive(config.outdated));
  });
  return ready;
}

// Spawns each terminal the dead daemon ran, with `claude --resume` for a Claude terminal, then
// attaches every running terminal: a new pane, hidden, or the pane it had.
async function revive(outdated: boolean) {
  const lost = await invoke<Listed[]>("terminal_lost");
  for (const r of await invoke<Revive[]>("revival", { terms: lost })) {
    // A revived conversation keeps its terminal's id, on purpose: its session stays linked to the
    // pane it had. Only a new conversation gets a fresh id (`startClaude`, #467).
    const argv = r.session ? await invoke<string[]>("claude_argv", { resume: r.session, auto: startedAuto(r.argv) }) : r.argv;
    const term = panes.get(r.id)?.term;
    await request({ op: "spawn", id: r.id, argv, cwd: r.cwd, cols: term?.cols ?? 80, rows: term?.rows ?? 24 });
  }
  for (const t of await list()) {
    if (t.exited !== null) continue;
    if (panes.get(t.id)?.term) await request({ op: "attach", id: t.id });
    else if (!panes.has(t.id)) draw(t.id, () => openPane(box(t.id), t.id, t.cwd, t.argv));
  }
  if (outdated) upgrade();
}

// A pane's box in `#terminal-body`, made the first time.
function box(id: string): HTMLElement {
  let pane = panes.get(id);
  if (!pane) {
    const node = document.createElement("div");
    node.className = "pane";
    node.hidden = true;
    element("terminal-body").append(node);
    pane = { box: node, term: null, title: "terminal" };
    panes.set(id, pane);
  }
  return pane.box;
}

// Shows terminal `id`'s pane, drawing it with `open` the first time, and the header's title and size.
function show(id: string, open: () => Xterm.Terminal | null) {
  shown = id;
  const place = box(id);
  for (const [key, pane] of panes) pane.box.hidden = pane.box !== place && !(split() && key === pinned);
  // A split opening or closing changes the width rule's layout (`main.ts`), which places the panes.
  if (split() !== laidSplit) window.dispatchEvent(new Event("layout"));
  else placePanes(stacked, cell);
  const pane = draw(id, open);
  header(pane);
  pane.term?.focus();
}

// Draws terminal `id`'s pane with `open` the first time.
function draw(id: string, open: () => Xterm.Terminal | null): Pane {
  const place = box(id);
  const pane = panes.get(id)!;
  if (pane.term === null && place.childElementCount === 0) {
    pane.term = open();
    pane.term?.onResize(() => shown === id && header(pane));
    pane.term?.onTitleChange((title) => {
      pane.title = title || "terminal";
      if (shown === id) header(pane);
    });
  }
  return pane;
}

// The selected session's name is on its row only (#571), so the header hides while one is selected and
// the terminal takes its height; with nothing selected, the shell or a pane just started keeps the
// title its program sets. The grid size is on the header's hover.
function header({ term, title }: Pane) {
  element("terminal-title").textContent = title;
  element("terminal-head").title = term ? `${term.cols} × ${term.rows}` : "";
}

// Ends terminal `id`'s whole process group (#322, point 3), then forgets it and drops its pane; the
// shell shows in its place.
export async function closePane(id: string) {
  await request({ op: "kill", id });
  await request({ op: "forget", id });
  // Its sessions end now, whether or not their `claude` lived to run its own end hook.
  await invoke("closed", { terminal: id });
  const pane = panes.get(id);
  pane?.term?.dispose();
  pane?.box.remove();
  panes.delete(id);
  const wasPinned = pinned === id;
  if (wasPinned) pinned = null;
  if (shown === id) show("shell", () => openPane(box("shell"), "shell", repo));
  else if (wasPinned) show(shown, () => null);
}

// Whether two panes show: the pinned one and a different one the selection shows.
export const split = () => pinned !== null && pinned !== shown && panes.has(pinned);

// Splits the column, pinning the pane shown; true when it split. Only a terminal pins, not the
// "Open here" message. Again, the split closes.
export function toggleSplit(): boolean {
  if (pinned !== null) {
    pinned = null;
    show(shown, () => null);
    return false;
  }
  if (!panes.get(shown)?.term) return false;
  pinned = shown;
  return true;
}

// Places the shown panes by `paneBoxes`: the selected one first, beside the rail, the pinned one
// second, stacked as the width rule says; a hidden pane keeps the stylesheet's box.
export function placePanes(stack: boolean, width: number) {
  [stacked, cell, laidSplit] = [stack, width, split()];
  const body = element("terminal-body");
  const [first, second] = paneBoxes(body.clientWidth, body.clientHeight, cell, laidSplit ? { stacked, share } : null);
  const inset = ({ top, right, bottom, left }: Box) => `${top}px ${right}px ${bottom}px ${left}px`;
  for (const pane of panes.values()) pane.box.style.inset = "";
  const divider = element("split-divider");
  divider.hidden = !second;
  if (!second) return;
  panes.get(shown)!.box.style.inset = inset(first);
  panes.get(pinned!)!.box.style.inset = inset(second);
  // The divider fills the gap between the panes.
  divider.classList.toggle("stacked", stacked);
  divider.style.inset = stacked ? `${body.clientHeight - first.bottom}px 0 auto` : `0 auto 0 ${body.clientWidth - first.right}px`;
}

// Dragging the split's divider moves the share, and the panes refit as their boxes change.
export function dragSplit() {
  const handle = element("split-divider");
  handle.addEventListener("pointerdown", (event) => {
    event.preventDefault(); // no text selection while dragging
    handle.setPointerCapture(event.pointerId);
    handle.classList.add("dragging");
  });
  handle.addEventListener("pointermove", (event) => {
    if (!handle.hasPointerCapture(event.pointerId)) return;
    const body = element("terminal-body").getBoundingClientRect();
    share = stacked ? (event.clientY - body.top) / body.height : (event.clientX - body.left) / body.width;
    placePanes(stacked, cell);
  });
  handle.addEventListener("lostpointercapture", () => handle.classList.remove("dragging"));
  new ResizeObserver(() => placePanes(stacked, cell)).observe(element("terminal-body"));
}

// Starts `claude` in `cwd` in a new terminal, with `prompt` as its first prompt, or
// `claude --resume <resume>` for an ended session, and shows its pane, in auto mode when `auto`
// (#541). Nothing is selected until the session's first event names its terminal; then the core
// selects it, so the rail follows (#567).
let starting: string | null = null;
export async function startClaude(cwd: string, resume: string | null = null, prompt = "", auto = false) {
  const argv = [...(await invoke<string[]>("claude_argv", { resume, auto })), ...(prompt ? ["--", prompt] : [])]; // `--`: a prompt like "-p x" is no flag
  await startTerminal(`claude-${crypto.randomUUID()}`, cwd, argv);
}

// A Doctor entry (#527) in a new terminal in the selected session's folder, else the configured
// repository. `shell` runs its command under the login shell, then leaves a shell for reading and
// rerunning; `session` starts `claude` with the command as its first prompt.
export function runDoctor({ command, kind }: DoctorEntry) {
  const cwd = studio.selection?.session.cwd ?? repo;
  if (kind === "session") return startClaude(cwd, null, command);
  return startTerminal(`doctor-${crypto.randomUUID()}`, cwd, ["/bin/sh", "-c", `${command}; exec "$SHELL" -l`]);
}

async function startTerminal(id: string, cwd: string, argv: string[]) {
  starting = id;
  element("terminal-head").hidden = false;
  studio.pending = null; // an earlier press's late reply must not take the pane from this session
  show(id, () => openPane(box(id), id, cwd, argv));
  showSelection(await invoke<Selection | null>("select_terminal", { terminal: id }));
}

// Whether a terminal's spawn `argv` started Claude Code in auto mode (#541), so its revival keeps it.
export const startedAuto = (argv: string[]) => argv[argv.indexOf("--permission-mode") + 1] === "auto";

// What the terminal shows for the selected session (#567): its own terminal while the daemon runs
// it, else a message with "Open here", never the login shell, which may run another session. The
// shell shows only while nothing is selected.
export type PaneView = "shell" | { terminal: string } | { message: string };
export function paneView(session: Pick<Session, "terminal" | "state"> | null, running: boolean): PaneView {
  if (!session) return "shell";
  if (session.terminal && running) return { terminal: session.terminal };
  return { message: session.state === "ended" ? "Ended. Open here resumes it." : "Runs in another terminal. Open here resumes it." };
}

// The selected session's pane, by `paneView`. Daniel's click always shows it; the shell's updates of
// the selection switch the pane only when the session, its terminal or its end changed, so they
// never take away a pane Daniel just started.
let selected: string | null = null;
export async function showSession(session: Session | null, clicked = false) {
  const key = session ? `${session.id} ${session.terminal ?? ""} ${session.state === "ended"}` : null;
  element("terminal-head").hidden = session !== null;
  const pane = panes.get(shown);
  if (pane) header(pane);
  if (key === selected && !clicked) return;
  selected = key;
  if (session) starting = null;
  const connected = await ready?.then(() => true, () => false);
  await revived.catch((e) => console.warn(`terminal: revive: ${e}`));
  const current = () => selected === key;
  if (!connected || !current()) return;
  if (!session) {
    // A pane Daniel just started stays until its session arrives.
    if (starting !== shown) show("shell", () => openPane(box("shell"), "shell", repo));
    return;
  }
  // A terminal of an earlier app run: attach when the daemon still runs it. Opening an exited or
  // unknown id would start a new program under it.
  const id = session.terminal;
  const own = id ? panes.get(id)?.term : null;
  const terminal = id && !own ? (await list()).find((t) => t.id === id && t.exited === null) : undefined;
  if (!current()) return;
  const view = paneView(session, Boolean(own || terminal));
  if (view === "shell") return;
  if ("terminal" in view) return show(view.terminal, () => openPane(box(view.terminal), view.terminal, terminal!.cwd, terminal!.argv));
  const resume = () => startClaude(session.cwd, session.resume_id, "", session.permission_mode === "auto").catch((e) => console.warn(e));
  box(ELSEWHERE).classList.add("elsewhere");
  box(ELSEWHERE).replaceChildren(text("p", view.message, "empty"), pressable(text("button", "Open here"), false, resume));
  show(ELSEWHERE, () => null);
}
const ELSEWHERE = "elsewhere";

// A program that failed at start (#558), a session's, a Doctor entry's or a revived one: a toast names
// the session, else the program, and the code, with "Show output", which shows its pane. When its pane
// is the shown one, the message box also takes its place, with its last lines; a hidden one only toasts.
export const failureTakesPane = (failed: string, current: string) => failed === current;
function failedAtStart({ id, name, status, lines }: { id: string; name: string; status: string; lines: string[] }) {
  const session = studio.listed.find((s) => s.terminal === id);
  const notice = startNotice(session ? sessionName(session) : name, { status });
  const output = () => show(id, () => null);
  showStartToast(notice, output);
  if (!failureTakesPane(id, shown)) return;
  box(ELSEWHERE).classList.add("elsewhere");
  box(ELSEWHERE).replaceChildren(text("p", notice, "empty"), text("pre", lines.join("\n"), "last-lines"), pressable(text("button", "Show output"), false, output));
  show(ELSEWHERE, () => null);
}
