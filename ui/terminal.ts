// The terminal pane (#466): xterm.js as in the #322 spike (WebGL, Unicode 11 widths, fit) on one
// daemon terminal. Keys go through `keyAction` in `keys.ts`; links and dropped files through
// `paths.ts`. xterm and its addons load as plain scripts from `vendor/` (see `npm run build`).

import type * as Xterm from "@xterm/xterm";
import { connect, decode, encode, onEvent, request } from "./daemon.js";
import { invoke, listen } from "./dom.js";
import { keyAction, type Action } from "./keys.js";
import { pathLinks, quotePath } from "./paths.js";

declare const Terminal: typeof Xterm.Terminal;
declare const FitAddon: typeof import("@xterm/addon-fit");
declare const Unicode11Addon: typeof import("@xterm/addon-unicode11");
declare const WebglAddon: typeof import("@xterm/addon-webgl");
declare const SearchAddon: typeof import("@xterm/addon-search");
declare const WebLinksAddon: typeof import("@xterm/addon-web-links");

const utf8 = new TextEncoder();
const openLink = (target: string) => invoke("open_link", { target }).catch((e) => console.warn(e));

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
    fontSize: parseFloat(value("--size-mono")),
    lineHeight: parseFloat(value("--term-line")),
    theme: { background: value("--term-bg"), foreground: value("--term-fg") },
  });
  const fit = new FitAddon.FitAddon();
  const search = new SearchAddon.SearchAddon();
  term.loadAddon(fit);
  term.loadAddon(search);
  term.loadAddon(new Unicode11Addon.Unicode11Addon());
  term.unicode.activeVersion = "11";
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

  async function start() {
    await connect();
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
  const note = (text: string) => term.write(`\r\n\x1b[2m[${text}]\x1b[0m\r\n`);
  const exited = (status: string) =>
    note(argv.length ? `claude exited: ${status}; Open in the story starts it again` : `exited: ${status}; press a key for a new shell`);
  const input = (data: string) => {
    if (quiet) return;
    if (ended === "lost" || (ended === "exited" && argv.length > 0)) return;
    if (ended) return restart();
    request({ op: "input", id, data: encode(utf8.encode(data)) });
  };

  const listening = onEvent((event) => {
    if (event.id !== undefined && event.id !== id) return;
    if (event.ev === "replay" && event.exited && argv.length === 0) {
      // A shell that ended while the app was away: a new one starts in its place.
      request({ op: "forget", id }).then(start);
    } else if (event.ev === "replay") {
      ended = event.exited ? "exited" : null;
      // At the size its bytes were written for, then fitted, which redraws full-screen programs.
      term.reset();
      term.resize(event.cols ?? term.cols, event.rows ?? term.rows);
      quiet = true;
      term.write(decode(event.data ?? ""), () => {
        quiet = false;
        fit.fit();
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

  term.onData(input);
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
    if ("send" in action) return input(action.send);
    if (action.command === "selectAll") term.selectAll();
    if (action.command === "clear") term.clear();
    if (action.command === "find") {
      find.hidden = false;
      find.select();
    }
  };
  term.attachCustomKeyEventHandler((e) => {
    const action = keyAction(e, false);
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
      const action = keyAction(cmdV, image);
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
        activate: () => openLink(text),
      }));
      show(links.length ? links : undefined);
    },
  });
  // A file dropped on the window types its path into the visible pane.
  const drops = listen<{ paths: string[] }>("tauri://drag-drop", ({ paths }) => {
    if (place.offsetParent === null) return;
    input(paths.map(quotePath).join(" ") + " ");
    term.focus();
  });
  // `term.dispose()` (`closePane`) disposes every addon, so this one stops the pane's listeners.
  term.loadAddon({
    activate() {},
    dispose() {
      resizes.disconnect();
      for (const stop of [listening, drops]) stop.then((off) => off());
    },
  });

  term.focus();
  return term;
}
