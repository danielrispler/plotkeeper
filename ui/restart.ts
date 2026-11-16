// Restarts (danielrispler/Falinks#470): the offer to restart into a new build, and the daemon upgrade (danielrispler/Falinks#322 resolution,
// point 4). An older daemon stays in use until Daniel asks for the restart, busy terminals or not;
// the app lists what each terminal loses, and never restarts it on its own. On yes, the
// shell stops the old daemon, and the daemon's loss revives each terminal (`panes.ts`).

import { list } from "./daemon.js";
import { element, invoke, listen, text } from "./dom.js";

// How a terminal comes back, from the core: `claude --resume <session>` for a Claude terminal, a
// shell in `cwd` otherwise; `busy` names what runs in it now.
export interface Revive {
  id: string;
  cwd: string;
  argv: string[];
  session: string | null;
  busy: string | null;
  revives: boolean;
}

const folder = (cwd: string) => cwd.split("/").filter(Boolean).pop() ?? cwd;
export const label = (r: Revive) =>
  r.session ? `Claude in ${folder(r.cwd)} (${r.session.slice(0, 8)})` : `Shell in ${folder(r.cwd)}`;
export const loses = (r: Revive) =>
  r.session
    ? "its screen; the conversation reopens with claude --resume"
    : "the shell's state (history, variables); it restarts in its folder";

// The bar over the terminal: `message`, then a button per action; none hides it.
function say(message: string | null, actions: [string, () => void][] = []) {
  const bar = element("restart");
  bar.hidden = message === null;
  bar.replaceChildren(text("span", message ?? ""));
  for (const [name, act] of actions) {
    const button = text("button", name, "btn");
    button.addEventListener("click", act);
    bar.append(button);
  }
}

// A rebuild, or a second instance of another build: sessions live in the daemon, so a restart keeps
// them. The offer is a button in the top bar, beside New session.
export const offerBuilds = () =>
  listen<string>("new-build", (exe) => {
    const button = element("new-build");
    button.hidden = false;
    button.onclick = () => invoke("restart_into", { exe });
  });

// A profile that does not load the studio plugin (#21): sessions started outside the studio would get
// no row. "Fix it" points its `CLAUDE_CODE_PLUGIN_DIRS` at the data folder's link, after a backup.
// The bar is shared, so an upgrade offer already on it wins; a link that failed has no fix.
export async function offerPluginFix() {
  const problem = await invoke<string | null>("plugin_setup").catch((e) => {
    if (element("restart").hidden) say(`${e}`, [["Dismiss", () => say(null)]]);
    return null;
  });
  if (!problem || !element("restart").hidden) return;
  const fix = () =>
    invoke("fix_plugin_setup").then(
      () => say("Fixed. Shells and claude sessions started from now on load the studio plugin; ~/.zshrc.studio-backup has the old profile."),
      (e) => say(`${e}`),
    );
  say(problem, [["Fix it", fix], ["Not now", () => say(null)]]);
}

// The same for agy's copy of the studio plugin (#49): agy copies it on install, so a changed plugin
// or a moved `studio-hook` needs an install again.
export async function offerAgyPluginFix() {
  const problem = await invoke<string | null>("agy_plugin_setup").catch(() => null);
  if (!problem || !element("restart").hidden) return;
  const fix = () =>
    invoke("fix_agy_plugin_setup").then(
      () => say("Installed. agy sessions started from now on send their events to the studio."),
      (e) => say(`${e}`),
    );
  say(problem, [["Install it", fix], ["Not now", () => say(null)]]);
}

const plan = async () => invoke<Revive[]>("revival", { terms: await list() });
const sleep = (ms: number) => new Promise((done) => setTimeout(done, ms));
const running = (r: Revive) => `${label(r)} is running ${r.busy}`;

// The bar says what a restart does and who is busy, refreshed every 3 s until a button is clicked.
function offer(): Promise<"now" | "later"> {
  let pick!: (choice: "now" | "later") => void;
  const chosen = new Promise<"now" | "later">((done) => (pick = done));
  let open = true;
  const choose = (choice: "now" | "later") => () => {
    open = false;
    pick(choice);
  };
  (async () => {
    let shown = "";
    while (open) {
      // A failed poll is logged and the next one retries; the bar waits for one that succeeds.
      const terms = await plan().catch((e) => (console.warn(`restart offer: ${e}`), null));
      if (!terms) {
        await sleep(3000);
        continue;
      }
      const busy = terms.filter((r) => r.busy);
      const message =
        "Studio was updated, but your terminals still run on the old version. Restarting moves them to the new one: Claude sessions reopen, shells restart in their folder." +
        (busy.length ? ` Busy now: ${busy.map(running).join("; ")}.` : "");
      if (open && message !== shown) {
        shown = message;
        say(message, [["Restart now…", choose("now")], ["Later", choose("later")]]);
      }
      await sleep(3000);
    }
  })();
  return chosen;
}

// Offers the restart, asks, and stops the old daemon on yes; "Later" hides the bar until the next
// app start. A busy terminal does not block it; the question names what it interrupts.
export async function upgrade() {
  for (;;) {
    if ((await offer()) === "later") return say(null);
    say(null);
    if (await ask(await plan())) {
      say("Restarting the terminals on the new version…");
      // The new daemon is this build's, so nothing shows the bar again unless the stop failed.
      await invoke("terminal_upgrade").then(() => say(null), (e) => say(`${e}`));
      return;
    }
  }
}

// The question, listing each terminal, what it loses and what a restart stops in a busy one; true on yes.
function ask(terms: Revive[]): Promise<boolean> {
  const dialog = element("upgrade") as HTMLDialogElement;
  const items = terms.map((r) => {
    const item = text("li", "");
    item.append(text("b", label(r)), ` loses ${loses(r)}.`);
    if (r.busy) item.append(text("b", ` It is running ${r.busy}, which stops.`));
    return item;
  });
  element("upgrade-terms").replaceChildren(...(items.length ? items : [text("li", "No terminal runs.")]));
  dialog.showModal();
  return new Promise((answer) =>
    dialog.addEventListener("close", () => answer(dialog.returnValue === "yes"), { once: true }),
  );
}
