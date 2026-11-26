// The quiet copy icon (#493): the PR brief's prompts for the agent and the "No map yet" view's draft prompt (#669) use it. A press puts a
// prompt that stands alone for any agent on the clipboard through Overturn's path (`copyOverturn`), and the copy toast shows it, or holds
// it to copy by hand when the clipboard refuses: the button says so with a `copy-for-agent` event, which `main.ts` hands to the toast, so
// this module loads none of the page's own modules. A prompt is a string, or waits for the changed files of a brief step's lit parts; those
// are asked for when the step is drawn, so a click usually has them and the clipboard is written inside it.

import { hint, pressable, text } from "./dom.js";
import type { Copied } from "./notes.js";
import { copyOverturn } from "./ratify.js";

export type Prompt = () => string | Promise<string>;
const COPY_ICON = '<svg viewBox="0 0 14 14" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="4.5" y="4.5" width="7" height="7" rx="1.5"/><path d="M9.5 2.5h-6a1 1 0 0 0-1 1v6"/></svg>';

/** A quiet copy icon named `label`, hinted `hover`, that copies what `prompt` makes. */
export function copyButton(label: string, hover: string, prompt: Prompt): HTMLElement {
  const button = pressable(text("button", "", "btn quiet brief-copy"), null);
  button.setAttribute("aria-label", label);
  hint(button, hover);
  button.innerHTML = COPY_ICON;
  button.addEventListener("click", () => {
    const write = navigator.clipboard?.writeText?.bind(navigator.clipboard);
    const copy = (made: string) =>
      void copyOverturn(made, write).then((outcome) => window.dispatchEvent(new CustomEvent<Copied>("copy-for-agent", { detail: { outcome, prompt: made } })));
    const made = prompt();
    if (typeof made === "string") copy(made);
    else void made.then(copy);
  });
  return button;
}
