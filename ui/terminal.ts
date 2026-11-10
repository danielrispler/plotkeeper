// The terminal pane (#466): xterm.js as in the #322 spike (WebGL, Unicode 11 widths, fit) on one
// daemon terminal. Keys go through `keyAction` in `keys.ts`; links and dropped files through
// `paths.ts`. xterm and its addons load as plain scripts from `vendor/` (see `npm run build`).

import type * as Xterm from "@xterm/xterm";
import { connect, decode, encode, onEvent, request } from "./daemon.js";
import { invoke, listen } from "./dom.js";
import { keyAction, type Action } from "./keys.js";
import { openFile, pathLinks, quotePath } from "./paths.js";
import { studio } from "./state.js";
import { noteKeys } from "./typing.js";

declare const Terminal: typeof Xterm.Terminal;
declare const FitAddon: typeof import("@xterm/addon-fit");
declare const Unicode11Addon: typeof import("@xterm/addon-unicode11");
declare const WebglAddon: typeof import("@xterm/addon-webgl");
declare const SearchAddon: typeof import("@xterm/addon-search");
declare const WebLinksAddon: typeof import("@xterm/addon-web-links");

const utf8 = new TextEncoder();
const openLink = (target: string) => invoke("open_link", { target }).catch((e) => console.warn(e));

// A program that ends nonzero within 2 s of starting failed at start (#558): its status and its last
// lines of output, read from the terminal's buffer; else null, and the pane only notes the exit.
type Lines = { length: number; getLine(y: number): { translateToString(trim: boolean): string } | undefined };
export function startFailure(status: string, ms: number, buffer: Lines, count = 5): { status: string; lines: string[] } | null {
  if (status === "code 0" || ms >= 2000) return null;
  const lines = Array.from({ length: buffer.length }, (_, y) => buffer.getLine(y)?.translateToString(true) ?? "");
  while (lines.length && !lines[lines.length - 1]) lines.pop();
  return { status, lines: lines.slice(-count) };
}
export const startNotice = (name: string, failure: { status: string }) => `${name} could not start: ${failure.status}`;

// Draws terminal `id` of the daemon into `place`, spawning it in `cwd` running `argv` (empty: a login
// shell) unless it already runs. A shell that exited starts again at the next key; an exited Claude
// terminal does not, as its new conversation would keep this id: "Open" starts one in a new terminal,
// so each conversation gets a fresh `STUDIO_TERMINAL_ID` (#467). One whose daemon is gone comes back
// through `panes.ts` (#470). Disposing the returned terminal stops the pane's listeners.
export function openPane(place: HTMLElement, id: string, cwd: string, argv: string[] = []): Xterm.Terminal {
  const css = getComputedStyle(document.documentElement);
  const value = (name: string) => css.getPropertyValue(name).trim();
  const term = new Terminal({
    allowProposedApi: true,
    scrollback: 10000,
    cursorBlink: true,
    fontFamily: value("--mono"),
    fontSize: parseFloat(value("--term-size")),
    lineHeight: parseFloat(value("--term-line")),
    theme: { background: value("--term-bg"), foreground: value("--term-fg") },
    // OSC 8 links: a `file://` one opens in the editor, any other through the shell's `open_link`.
    linkHandler: {
      allowNonHttpProtocols: true,
      activate: (_, uri) => (/^file:/i.test(uri) ? openFile(uri, cwd) : openLink(uri)),
    },
  });
  const fit = new FitAddon.FitAddon();
  const search = new SearchAddon.SearchAddon();
  term.loadAddon(fit);
  term.loadAddon(search);
  term.loadAddon(new Unicode11Addon.Unicode11Addon());
  term.unicode.activeVersion = "11";
  // Invisible until the first replay is written and fitted, so it never shows at the old size (#570).
  place.classList.add("fitting");
  term.open(place);
  try {
    const webgl = new WebglAddon.WebglAddon();
    // xterm falls back to its default renderer once the WebGL one is gone.
    webgl.onContextLoss(() => webgl.dispose());
    term.loadAddon(webgl);
  } catch (e) {
    console.warn(`terminal: no WebGL, the default renderer draws: ${e}`);
  }

  // While a replay is written, xterm answers the queries in the old output; those answers must not
  // reach the program now running.
  let quiet = false;
  // Why the terminal stopped, until the next key starts it again.
  let ended: "exited" | "lost" | null = null;
  // When this pane last started its program; an exit soon after is a failure at start (#558).
  let started = Date.now();

  async function start() {
    await connect();
    started = Date.now();
    // "The id is taken" when it already runs; the attach replays it either way.
    await request({ op: "spawn", id, argv, cwd, cols: term.cols, rows: term.rows });
    await request({ op: "attach", id });
  }
  function restart() {
    const was = ended;
    ended = null;
    term.reset();
    const forget = was === "exited" ? request({ op: "forget", id }) : Promise.resolve();
    forget.then(start).catch((e) => note(`${e}`));
  }
  const note = (text: string) => {
    place.classList.remove("fitting");
    term.write(`\r\n\x1b[2m[${text}]\x1b[0m\r\n`);
  };
  const exited = (status: string) => {
    // Read once the output before it is written, and before the note below. `panes.ts` shows the
    // notice, naming the program until a session names the terminal.
    const ms = Date.now() - started;
    term.write("", () => {
      const failure = startFailure(status, ms, term.buffer.active);
      const name = argv[0]?.split("/").pop() ?? "the shell";
      if (failure) window.dispatchEvent(new CustomEvent("start-failed", { detail: { id, name, ...failure } }));
    });
    note(argv.length ? `claude exited: ${status}; Open in the story starts it again` : `exited: ${status}; press a key for a new shell`);
  };
  const input = (data: string) => {
    if (quiet) return;
    if (ended === "lost" || (ended === "exited" && argv.length > 0)) return;
    if (ended) return restart();
    request({ op: "input", id, data: encode(utf8.encode(data)) });
  };
  // Daniel's own keys, which the studio's typing waits on (#544).
  const keys = (data: string) => {
    noteKeys(id, data);
    input(data);
  };

  const listening = onEvent((event) => {
    if (event.id !== undefined && event.id !== id) return;
    if (event.ev === "replay" && event.exited && argv.length === 0) {
      // A shell that ended while the app was away: a new one starts in its place.
      request({ op: "forget", id }).then(start);
    } else if (event.ev === "replay") {
      // After a lost daemon, `revive` has just started the program again.
      if (ended === "lost") started = Date.now();
      ended = event.exited ? "exited" : null;
      // At the size its bytes were written for, then fitted, which redraws full-screen programs.
      term.reset();
      term.resize(event.cols ?? term.cols, event.rows ?? term.rows);
      quiet = true;
      term.write(decode(event.data ?? ""), () => {
        quiet = false;
        fit.fit();
        place.classList.remove("fitting");
      });
      if (event.exited) exited(event.exited);
    } else if (event.ev === "output") {
      term.write(decode(event.data ?? ""));
    } else if (event.ev === "exit") {
      ended = "exited";
      exited(event.status ?? "");
    } else if (event.ev === "lost") {
      // `panes.ts` reconnects and brings the terminal back (#470).
      ended = "lost";
      note("the terminal daemon is gone; reconnecting");
    } else if (event.ev === "error" && event.message !== "the id is taken") {
      note(event.message ?? "error");
    }
  });
  listening.then(() => start().catch((e) => note(`${e}`)));

  term.onData(keys);
  term.onBinary((data) => !quiet && request({ op: "input", id, data: btoa(data) }));
  term.onResize(({ cols, rows }) => request({ op: "resize", id, cols, rows }));
  const resizes = new ResizeObserver(() => fit.fit());
  resizes.observe(place);

  // Claude Code asks `ESC ] 7501 ; ?` and reports its status (`state=working:app=claude-code:…`) only
  // to a terminal that answers; any reply whose data starts with `?` counts (#466).
  term.parser.registerOscHandler(7501, (data) => {
    if (data.startsWith("?")) input("\x1b]7501;?\x07");
    return true;
  });

  const find = document.createElement("input");
  find.className = "term-find";
  find.type = "search";
  find.placeholder = "Find";
  find.hidden = true;
  place.append(find);
  find.addEventListener("input", () => search.findNext(find.value, { incremental: true }));
  find.addEventListener("keydown", (e) => {
    if (e.key === "Enter") (e.shiftKey ? search.findPrevious : search.findNext).call(search, find.value);
    if (e.key === "Escape") {
      find.hidden = true;
      search.clearDecorations();
      term.focus();
    }
  });

  const run = (action: Exclude<Action, string>) => {
    if ("send" in action) return keys(action.send);
    if (action.command === "selectAll") term.selectAll();
    if (action.command === "clear") term.clear();
    if (action.command === "find") {
      find.hidden = false;
      find.select();
    }
  };
  term.attachCustomKeyEventHandler((e) => {
    const action = keyAction(e, false, studio.files.keymap);
    if (action === "terminal") return true;
    if (typeof action === "object" && e.type === "keydown") {
      e.preventDefault();
      run(action);
    }
    // "native" and "app": xterm skips the key, and the Edit menu or the app's own handlers take it.
    return false;
  });
  // Cmd+V reaches xterm as the webview's paste event, the one place the clipboard's kinds show.
  place.addEventListener(
    "paste",
    (e) => {
      const image = [...(e.clipboardData?.items ?? [])].some((item) => item.type.startsWith("image/"));
      const cmdV = { key: "v", code: "KeyV", metaKey: true, ctrlKey: false, altKey: false, shiftKey: false };
      const action = keyAction(cmdV, image, studio.files.keymap);
      if (typeof action === "object") {
        e.preventDefault();
        e.stopPropagation();
        run(action);
      }
    },
    true,
  );

  term.loadAddon(new WebLinksAddon.WebLinksAddon((_, uri) => openLink(uri)));
  // shortcut: reads one buffer line, so a path wrapped onto the next line or after wide characters
  // links wrongly; join wrapped lines and map cells if that shows up.
  term.registerLinkProvider({
    provideLinks(y, show) {
      const line = term.buffer.active.getLine(y - 1)?.translateToString(true) ?? "";
      const links = pathLinks(line).map(({ text, start }) => ({
        text,
        range: { start: { x: start + 1, y }, end: { x: start + text.length, y } },
        activate: () => openFile(text, cwd),
      }));
      show(links.length ? links : undefined);
    },
  });
  // A file dropped on the window types its path into the visible pane; in a split (#529), the focused one.
  const drops = listen<{ paths: string[] }>("tauri://drag-drop", ({ paths }) => {
    if (place.hidden || place.offsetParent === null) return;
    if (document.querySelectorAll(".pane:not([hidden])").length > 1 && !place.contains(document.activeElement)) return;
    keys(paths.map(quotePath).join(" ") + " ");
    term.focus();
  });
  // A saved `settings.json` (`main.ts`) may change the font size.
  const restyle = () => {
    term.options.fontSize = parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--term-size"));
    fit.fit();
  };
  window.addEventListener("settings", restyle);
  // `term.dispose()` (`closePane`) disposes every addon, so this one stops the pane's listeners.
  term.loadAddon({
    activate() {},
    dispose() {
      resizes.disconnect();
      window.removeEventListener("settings", restyle);
      for (const stop of [listening, drops]) stop.then((off) => off());
    },
  });

  term.focus();
  return term;
}
