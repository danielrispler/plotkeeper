// Back a law (spec danielrispler/plotkeeper#378, ticket #398): a law on trust in the part's sheet (`map-sheet.ts`) offers Back a law, which
// starts a background run (`server/src/back_a_law.rs`). When the run ends, plotkeeper proves its patch itself, in a sandbox and with no model,
// and the law's row says where the patch stands: being proven, rejected with why, or proven with Review. The review is an overlay over the
// map, in the glass, the second layer Esc closes (`layerOrder`): the law, the new tagged test, the test patch, and the checks' output with
// the test alone (passing) and with the plant (failing on that test), with Open draft PR and Discard, which deletes the patch. Open draft PR
// (#400) opens a draft PR from a temporary worktree; the row then says its PR, or "Out of date, run again" when the patch no longer applies.

import { failure, invoke, text } from "./dom.js";
import { diffLines } from "./diff.js";
import { currentMap, onDraw, registerLayer, reloadMap, shownProject } from "./map.js";
import { copyOverturn, overturnReport } from "./ratify.js";
import { showNotice } from "./notice.js";
import { logStop } from "./stop-rules.js";
import type { LawPatch, LawPatchOpened } from "./types.js";

// What a law's row offers: a new run, a patch being proven, a rejected one (its first line), a proven one to review, one whose draft PR
// is open, or one out of date.
export type LawAction = { law: string } & (
  | { kind: "start" }
  | { kind: "proving" }
  | { kind: "rejected"; id: string; line: string }
  | { kind: "proven"; id: string }
  | { kind: "opened"; id: string; pr: number; url: string }
  | { kind: "out-of-date"; id: string }
);
// What the buttons do: start a run for a law, open a patch's review, open its draft PR, discard a patch, close the review.
export type PatchActs = { start: (law: string) => void; review: (id: string) => void; open: (id: string) => void; discard: (id: string) => void; close: () => void };

// The action of `law` by its newest patch; a law with none, or a payload from before back a law, offers a new run.
export function lawAction(law: string, patches: LawPatch[] | null | undefined): LawAction {
  const newest = (patches ?? []).filter((patch) => patch.law === law).sort((a, b) => b.at - a.at)[0];
  if (!newest) return { law, kind: "start" };
  if (newest.state === "proving") return { law, kind: "proving" };
  if (newest.state === "rejected") return { law, kind: "rejected", id: newest.id, line: (newest.line ?? "").split("\n")[0] };
  if (newest.state === "opened") return { law, kind: "opened", id: newest.id, pr: newest.pr ?? 0, url: newest.url ?? "" };
  if (newest.state === "out-of-date") return { law, kind: "out-of-date", id: newest.id };
  return { law, kind: "proven", id: newest.id };
}

// The notice line of what Open draft PR did.
export function openedLine(opened: LawPatchOpened): string {
  return opened.kind === "pr" ? `Draft PR #${opened.number} opened: ${opened.url}` : "Out of date, run again: the patch no longer applies to the default branch";
}

const button = (words: string, title: string, click: () => void, kind = "quiet") => {
  const node = text("button", words, `btn ${kind}`.trim());
  node.setAttribute("type", "button");
  node.title = title;
  node.addEventListener("click", click);
  return node;
};

// The nodes of a law's action, for its row in the part's sheet.
export function lawActionNodes(action: LawAction, act: Pick<PatchActs, "start" | "review" | "discard">): HTMLElement[] {
  const start = () => button("Back a law", "Start a background run that writes a test for this law, then prove it here", () => act.start(action.law));
  switch (action.kind) {
    case "start":
      return [start()];
    case "proving":
      return [text("span", "Proving the patch…", "law-patch-note")];
    case "rejected":
      return [
        text("span", `Rejected: ${action.line}`, "law-patch-note"),
        button("Dismiss", "Forget this rejected run", () => act.discard(action.id)),
        start(),
      ];
    case "opened": {
      const link = text("a", `Draft PR #${action.pr} opened`, "law-patch-note");
      link.setAttribute("target", "_blank");
      link.setAttribute("rel", "noopener");
      if (action.url.startsWith("https://")) (link as HTMLAnchorElement).href = action.url;
      return [link];
    }
    case "out-of-date":
      return [text("span", "Out of date, run again", "law-patch-note"), button("Discard", "Delete this patch", () => act.discard(action.id)), start()];
    case "proven":
      return [button("Review the patch", "See the test this run wrote and the proof plotkeeper ran", () => act.review(action.id), "primary")];
  }
}

// The review of a proven `patch` of the law whose text is `law`.
export function reviewNodes(patch: LawPatch, law: string, act: Pick<PatchActs, "open" | "discard" | "close">): HTMLElement[] {
  const proof = patch.proof;
  const diff = text("div", "", "patch-diff");
  diff.append(...diffLines(patch.diff ?? "").map((line) => text("pre", line.text, `diff-line ${line.kind}`)));
  const output = (heading: string, out: string) => [text("h3", heading, "patch-heading"), text("pre", out, "patch-output")];
  const foot = text("div", "", "patch-foot");
  foot.append(
    button("Open draft PR", "Push a branch with this test patch from the default branch and open a draft PR with the proof", () => act.open(patch.id), "primary"),
    button("Discard", "Delete this patch", () => act.discard(patch.id)),
    button("Close", "Close the review", act.close),
  );
  return [
    text("h2", "Back a law", "patch-title"),
    text("p", law, "patch-law"),
    ...(proof ? [text("p", `The new test: ${proof.test} in ${proof.file}`, "patch-test")] : []),
    text("h3", "The test patch", "patch-heading"),
    diff,
    ...(proof ? [...output("With the test alone, the checks pass", proof.passed), ...output("With the plant, the new test fails", proof.failed)] : []),
    foot,
  ];
}

// The patch under review, by its run's id; null while closed.
let shown: string | null = null;
const overlay = () => document.getElementById("map-patch-review");

function paint() {
  const box = overlay();
  if (!box) return;
  const view = currentMap();
  const patch = view && !view.scrub ? view.state.law_patches?.find((each) => each.id === shown && each.state === "proven") : undefined;
  if (!view || !patch) {
    shown = null;
    box.hidden = true;
    box.replaceChildren();
    return;
  }
  const law = view.doc.laws.find((each) => each.id === patch.law)?.text ?? patch.law;
  box.hidden = false;
  box.replaceChildren(...reviewNodes(patch, law, acts));
}

const acts: PatchActs = {
  start: (law) => {
    const project = shownProject();
    if (project === null) return;
    invoke("back_a_law", { project, law })
      .then(async (started) => {
        if (started.kind === "run") {
          logStop("back-law-started", project);
          return void reloadMap();
        }
        // A run that cannot start copies its prompt, as a failed run's row does.
        const copied = await copyOverturn(started.prompt, navigator.clipboard?.writeText?.bind(navigator.clipboard));
        showNotice(`Back a law did not start: ${started.line}. ${overturnReport(copied, started.prompt).line}`);
      })
      .catch(failure("back_a_law", "error"));
  },
  review: (id) => {
    shown = id;
    paint();
  },
  open: (id) => {
    const project = shownProject();
    if (project === null) return;
    invoke("law_patch_open", { project, id })
      .then((opened) => {
        showNotice(openedLine(opened));
        if (shown === id) shown = null;
        return reloadMap();
      })
      .catch(failure("law_patch_open", "error"));
  },
  discard: (id) => {
    const project = shownProject();
    if (project === null) return;
    invoke("law_patch_discard", { project, id })
      .then(() => {
        if (shown === id) shown = null;
        return reloadMap();
      })
      .catch(failure("law_patch_discard", "error"));
  },
  close: () => {
    shown = null;
    paint();
  },
};

// The sheet's law rows use these acts.
export const patchActs = acts;

onDraw(paint);
registerLayer("patch-review", { open: () => shown !== null, close: acts.close });
