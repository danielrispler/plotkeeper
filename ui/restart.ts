// Restarts (danielrispler/Falinks#470): the offer to restart into a new build, and the daemon upgrade (danielrispler/Falinks#322 resolution,
// point 4). A daemon below this app's protocol stays in use until every terminal is idle; then the
// app asks Daniel, listing what each terminal loses, and never restarts it on its own. On yes, the
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
// them.
export const offerBuilds = () =>
  listen<string>("new-build", (exe) =>
    say("A new build is ready.", [["Restart into it", () => invoke("restart_into", { exe })]]),
  );

const plan = async () => invoke<Revive[]>("revival", { terms: await list() });
const sleep = (ms: number) => new Promise((done) => setTimeout(done, ms));

// Waits for idle, asks, and stops the old daemon on yes; "Not now" waits for Daniel to ask again.
export async function upgrade() {
  const older = "The terminal daemon is older than this app.";
  for (;;) {
    const terms = await plan();
    const busy = terms.filter((r) => r.busy);
    if (busy.length > 0) {
      say(`${older} Waiting for every terminal to be idle: ${busy.map((r) => `${label(r)} runs ${r.busy}`).join("; ")}.`);
      await sleep(3000);
      continue;
    }
    say(null);
    if (await ask(terms)) {
      // Idle may have ended while the question was open.
      if ((await plan()).some((r) => r.busy)) continue;
      say(`${older} Restarting it…`);
      await invoke("terminal_upgrade").catch((e) => say(`${e}`));
      return;
    }
    await new Promise<void>((again) => say(older, [["Restart it…", () => again()]]));
  }
}

// The question, listing each terminal and what it loses; true on yes.
function ask(terms: Revive[]): Promise<boolean> {
  const dialog = element("upgrade") as HTMLDialogElement;
  const items = terms.map((r) => {
    const item = text("li", "");
    item.append(text("b", label(r)), ` loses ${loses(r)}.`);
    return item;
  });
  element("upgrade-terms").replaceChildren(...(items.length ? items : [text("li", "No terminal runs.")]));
  dialog.showModal();
  return new Promise((answer) =>
    dialog.addEventListener("close", () => answer(dialog.returnValue === "yes"), { once: true }),
  );
}
