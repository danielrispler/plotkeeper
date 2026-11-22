// The accept and overturn buttons of an item to ratify (spec danielrispler/plotkeeper#334, ticket #343), shared by the part sheet's law and
// link rows and the brief's ratify step. Ratifying never blocks anything: Accept only records that Daniel has looked. Overturn records that
// too, then hands the quoted item to the agent (`overturnIn`, which `main.ts` gives: the PR's live session's input without Enter, else New session).

import { failure, invoke, text } from "./dom.js";
import { overturnIn } from "./map.js";
import { overturnPrompt } from "./ratify.js";
import { logStop } from "./stop-rules.js";
import type { RatifyRow } from "./types.js";

export type Ratifiable = Pick<RatifyRow, "pr" | "kind" | "id" | "text" | "change">;

// `done` runs once the shell has the decision.
export function ratifyActions(project: string, item: Ratifiable, done: (state: "accepted" | "overturned") => void): HTMLElement {
  const box = text("span", "", "ratify-actions");
  const decide = (state: "accepted" | "overturned") => {
    box.querySelectorAll("button").forEach((button) => ((button as HTMLButtonElement).disabled = true));
    invoke("ratify_set", { project, pr: item.pr, kind: item.kind, id: item.id, state, text: item.text })
      .then(() => {
        logStop("ratify", project, item.pr);
        if (state === "overturned") overturnIn(project, item.pr, overturnPrompt(item));
        done(state);
      })
      .catch((e) => {
        box.querySelectorAll("button").forEach((button) => ((button as HTMLButtonElement).disabled = false));
        failure("ratify_set")(e);
      });
  };
  const accept = text("button", "Accept", "btn quiet ratify-accept");
  accept.title = "Record that you have looked at this and it stands; nothing waits on it";
  accept.addEventListener("click", () => decide("accepted"));
  const overturn = text("button", "Overturn", "btn quiet ratify-overturn");
  overturn.title = "Undo it: starts a session with a prompt quoting it, or types the prompt into the PR's session without pressing Enter";
  overturn.addEventListener("click", () => decide("overturned"));
  for (const button of [accept, overturn]) button.setAttribute("type", "button");
  box.append(accept, overturn);
  return box;
}
