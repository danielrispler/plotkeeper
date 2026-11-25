// Back a law (spec danielrispler/plotkeeper#378, ticket #398): a law on trust in the part's sheet (`map-sheet.ts`) offers Back a law, which
// starts a background run (`server/src/back_a_law.rs`). When the run ends, plotkeeper proves its patch itself, in a sandbox and with no model,
// and the law's row says where the patch stands: being proven, rejected with why, or proven with Review. The review is an overlay over the
// map, in the glass, the second layer Esc closes (`layerOrder`): the law, the new tagged test, the test patch, and the checks' output with
// the test alone (passing) and with the plant (failing on that test), with Open draft PR and Discard, which deletes the patch. Open draft PR
// (#400) opens a draft PR from a temporary worktree; the row then says its PR, or "Out of date, run again" when the patch no longer applies.
//
// Guard with a law (#401): the brief of a PR that closed a bug-labelled issue and added or changed no tagged test (the server reads both
// from the brief's facts) ends with a row beside Ratify, before and after the PR merges. It offers back a law on each on-trust law of a part
// the PR touched, or a new law: the no-tools writer drafts its sentence from the PR's facts, Daniel confirms or edits it, and only what he
// confirmed reaches the run, which adds the law to the map with its tagged test in one patch, proven and reviewed like back a law.

import { confirmed, failure, invoke, messageOf, promptBox, scrollingPre, text, type Confirm } from "./dom.js";
import { diffLines } from "./diff.js";
import { currentMap, onDraw, registerLayer, reloadMap, shownProject } from "./map.js";
import { copyOverturn, overturnReport } from "./ratify.js";
import { showNotice } from "./notice.js";
import { logStop } from "./stop-rules.js";
import type { MapView } from "./map.js";
import type { Law } from "./map-model.js";
import type { BackLawStarted, Guard, LawPatch, LawPatchOpened, NewLaw } from "./types.js";

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
// What the buttons do: start a run for a law, open a patch's review, open its draft PR, discard a patch, close the review, and ask before
// Discard (#459), resolving true when confirmed.
export type PatchActs = {
  start: (law: string) => void;
  review: (id: string) => void;
  open: (id: string) => void;
  discard: (id: string) => void;
  close: () => void;
  confirm: (ask: Confirm) => Promise<boolean>;
};

// Discard asks first, naming the patch; Dismiss on a rejected run, which has no patch to lose, acts at once.
const discardPatch: Confirm = { heading: "Discard this patch?", lost: "The test patch this run wrote, and its proof, are deleted.", verb: "Discard patch" };
const discardButton = (id: string, act: Pick<PatchActs, "discard" | "confirm">) =>
  button("Discard", "Delete this patch", () => void act.confirm(discardPatch).then((yes) => yes && act.discard(id)));

// The action of `law` by its newest patch; a law with none, or a payload from before back a law, offers a new run.
export function lawAction(law: string, patches: LawPatch[] | null | undefined): LawAction {
  const newest = (patches ?? []).filter((patch) => patch.law === law).sort((a, b) => b.at - a.at)[0];
  if (!newest) return { law, kind: "start" };
  if (newest.state === "proving") return { law, kind: "proving" };
  if (newest.state === "rejected") return { law, kind: "rejected", id: newest.id, line: (newest.line ?? "").split("\n")[0] ?? "" };
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

// The nodes of a law's action, for its row in the part's sheet. `again` is the words of the button that starts a run; none for a new law,
// which the map does not have to back.
export function lawActionNodes(action: LawAction, act: Pick<PatchActs, "start" | "review" | "discard" | "confirm">, again: string | null = "Back a law"): HTMLElement[] {
  const start = () => (again === null ? [] : [button(again, "Start a background run that writes a test for this law, then prove it here", () => act.start(action.law))]);
  switch (action.kind) {
    case "start":
      return start();
    case "proving":
      return [text("span", "Proving the patch…", "law-patch-note")];
    case "rejected":
      return [
        text("span", `Rejected: ${action.line}`, "law-patch-note"),
        button("Dismiss", "Forget this rejected run", () => act.discard(action.id)),
        ...start(),
      ];
    case "opened": {
      const link = text("a", `Draft PR #${action.pr} opened`, "law-patch-note");
      link.setAttribute("target", "_blank");
      link.setAttribute("rel", "noopener");
      if (action.url.startsWith("https://")) link.href = action.url;
      return [link];
    }
    case "out-of-date":
      return [text("span", "Out of date, run again", "law-patch-note"), discardButton(action.id, act), ...start()];
    case "proven":
      return [button("Review the patch", "See the test this run wrote and the proof plotkeeper ran", () => act.review(action.id), "primary")];
  }
}

// The review of a proven `patch` of the law whose text is `law`.
export function reviewNodes(patch: LawPatch, law: string, act: Pick<PatchActs, "open" | "discard" | "close" | "confirm">): HTMLElement[] {
  const proof = patch.proof;
  const diff = text("div", "", "patch-diff");
  diff.append(...diffLines(patch.diff ?? "").map((line) => text("pre", line.text, `diff-line ${line.kind}`)));
  // An output, named by its heading.
  const output = (heading: string, out: string) => [text("h3", heading, "patch-heading"), scrollingPre(out, "patch-output", heading)];
  const foot = text("div", "", "patch-foot");
  foot.append(
    button("Open draft PR", "Push a branch with this test patch from the default branch and open a draft PR with the proof", () => act.open(patch.id), "primary"),
    discardButton(patch.id, act),
    button("Close", "Close the review", act.close),
  );
  return [
    text("h2", patch.text ? "A new law" : "Back a law", "patch-title"),
    text("p", law, "patch-law"),
    ...(proof ? [text("p", `The new test: ${proof.test} in ${proof.file}`, "patch-test")] : []),
    text("h3", "The test patch", "patch-heading"),
    diff,
    ...(proof ? [...output("With the test alone, the checks pass", proof.passed), ...output("With the plant, the new test fails", proof.failed)] : []),
    foot,
  ];
}

// A run that could not start and whose prompt the clipboard refused: its line, and the prompt in the overlay to copy by hand.
export function refusedNodes(line: string, prompt: string, close: () => void): HTMLElement[] {
  const area = promptBox(prompt, 6);
  const foot = text("div", "", "patch-foot");
  foot.append(button("Close", "Close", close));
  return [text("h2", "The run did not start", "patch-title"), text("p", `${line}. ${overturnReport("manual", prompt).line}`, "patch-law"), area, foot];
}

// A run started: told to the stop-rule log and the map drawn again. A run that cannot start copies its prompt, as a failed run's row does;
// when the clipboard refuses, the overlay shows the prompt.
async function afterStart(started: BackLawStarted, told: () => void) {
  if (started.kind === "run") {
    told();
    await reloadMap();
    return;
  }
  const copied = await copyOverturn(started.prompt, navigator.clipboard?.writeText?.bind(navigator.clipboard));
  if (copied === "copied") return showNotice(`The run did not start: ${started.line}. ${overturnReport(copied, started.prompt).line}`);
  [shown, refused] = [null, { line: started.line, prompt: started.prompt }];
  paint();
}

// The patch under review, by its run's id; null while closed.
let shown: string | null = null;
// The refused run whose prompt the overlay shows; null while closed.
let refused: { line: string; prompt: string } | null = null;
const overlay = () => document.getElementById("map-patch-review");

function paint() {
  const box = overlay();
  if (!box) return;
  if (refused) {
    box.hidden = false;
    box.replaceChildren(...refusedNodes(refused.line, refused.prompt, acts.close));
    return;
  }
  const view = currentMap();
  const patch = view && !view.scrub ? view.state.law_patches?.find((each) => each.id === shown && each.state === "proven") : undefined;
  if (!view || !patch) {
    shown = null;
    box.hidden = true;
    box.replaceChildren();
    return;
  }
  const law = patch.text ?? view.doc.laws.find((each) => each.id === patch.law)?.text ?? patch.law;
  box.hidden = false;
  box.replaceChildren(...reviewNodes(patch, law, acts));
}

const acts: PatchActs = {
  start: (law) => {
    const project = shownProject();
    if (project === null) return;
    invoke("back_a_law", { project, law })
      .then((started) => afterStart(started, () => logStop("back-law-started", project)))
      .catch(failure("back_a_law", "error"));
  },
  review: (id) => {
    [shown, refused] = [id, null];
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
    [shown, refused] = [null, null];
    paint();
  },
  confirm: confirmed,
};

// The sheet's law rows use these acts.
export const patchActs = acts;

onDraw(paint);
registerLayer("patch-review", { open: () => shown !== null || refused !== null, close: acts.close });


// ---- Guard with a law (#401).

// What a bug fix's Guard with a law offers: backing each on-trust law of a part PR `pr` touched, and the new laws its guard's runs add
// (the newest patch of each).
export type GuardOffer = { back: { id: string; text: string }[]; added: LawPatch[] };
export function guardOffer(guard: Guard, pr: number, laws: Law[], patches: LawPatch[] | null | undefined): GuardOffer {
  const added = new Map<string, LawPatch>();
  for (const patch of patches ?? []) {
    if (patch.guards !== pr || !patch.text) continue;
    const seen = added.get(patch.law);
    if (!seen || seen.at < patch.at) added.set(patch.law, patch);
  }
  return {
    back: laws.filter((law) => law.backing === "trust" && guard.parts.includes(law.on)).map((law) => ({ id: law.id, text: law.text })),
    added: [...added.values()],
  };
}

// What the guard's buttons do: back an on-trust law, start a new law's confirm step, and a patch's review and discard.
export type GuardActs = Pick<PatchActs, "review" | "discard" | "confirm"> & { back: (law: string) => void; add: () => void };

// The guard's row: each on-trust law with Back this law (or where its patch stands), each new law with where its patch stands, and Add a
// new law. `patches` are the project's, for the state of each law's newest one.
export function guardNodes(offer: GuardOffer, patches: LawPatch[] | null | undefined, act: GuardActs, adding: HTMLElement[] | null = null): HTMLElement {
  const row = text("div", "", "guard-row");
  row.append(text("p", "Guard with a law", "guard-title"), text("p", "Back a law on trust of a part it touched, or add a new one.", "brief-quiet"));
  const list = text("ul", "", "guard-laws");
  for (const law of offer.back) {
    const item = text("li", "", "guard-law");
    item.dataset.law = law.id;
    item.append(text("span", law.text, "guard-text"), ...lawActionNodes(lawAction(law.id, patches), { ...act, start: act.back }, "Back this law"));
    list.append(item);
  }
  for (const patch of offer.added) {
    const item = text("li", "", "guard-law");
    item.dataset.law = patch.law;
    item.append(text("span", `New law: ${patch.text}`, "guard-text"), ...lawActionNodes(lawAction(patch.law, offer.added), { ...act, start: () => {} }, null));
    list.append(item);
  }
  if (list.children.length) row.append(list);
  if (adding) row.append(...adding);
  else row.append(button("Add a new law", "The writer drafts a law from this PR's facts; you confirm or edit it before a run adds it with its test", act.add));
  return row;
}

// The confirm step of a new law: the writer's draft (null while it drafts) in fields Daniel may edit, the part among the ones the PR
// touched, and Start the run with what the fields hold, which are kept in `draft` as he types. `line` says why the writer gave no draft, so
// he writes the law himself.
export function confirmNodes(draft: NewLaw | null, parts: { id: string; name: string }[], line: string | null, act: { start: (law: NewLaw) => void; cancel: () => void }): HTMLElement[] {
  const cancel = button("Cancel", "Leave the new law", act.cancel);
  if (!draft) return [text("p", "Drafting the law…", "brief-quiet guard-drafting"), cancel];
  const box = text("div", "", "guard-confirm");
  if (line) box.append(text("p", `The writer could not draft one: ${line}. Write the law yourself.`, "brief-quiet"));
  const part = text("select", "", "guard-part");
  for (const each of parts) {
    const option = text("option", each.name, "");
    option.value = each.id;
    part.append(option);
  }
  part.value = draft.on;
  part.name = "law-part";
  part.setAttribute("aria-label", "The part the law is on");
  const id = text("input", "", "guard-id");
  id.value = draft.id;
  id.name = "law-id";
  id.autocomplete = "off";
  id.spellcheck = false;
  id.setAttribute("aria-label", "The law's id");
  const sentence = text("textarea", "", "guard-sentence");
  sentence.value = draft.text;
  sentence.name = "law-sentence";
  sentence.autocomplete = "off";
  sentence.setAttribute("aria-label", "The law's sentence");
  sentence.setAttribute("rows", "2");
  // What Daniel types is kept in the draft, so the step drawn again (a map-landed, a resize) shows it.
  part.addEventListener("change", () => (draft.on = part.value));
  id.addEventListener("input", () => (draft.id = id.value));
  sentence.addEventListener("input", () => (draft.text = sentence.value));
  const foot = text("div", "", "guard-foot");
  foot.append(
    button("Start the run", "Start a background run that adds this law, as written here, with its test", () => act.start({ id: id.value.trim(), on: part.value, text: sentence.value.trim() }), "primary"),
    cancel,
  );
  box.append(text("p", "Confirm or edit the law. Only what you confirm here reaches the run.", "brief-quiet"), part, id, sentence, foot);
  return [box];
}

// The new law being confirmed, by project and PR: its draft (null while the writer drafts) and why there is none.
let confirming: { project: string; pr: number; draft: NewLaw | null; line: string | null } | null = null;

// The guard's row on PR `pr`'s brief in `view`; `redraw` draws the brief again after a change of the confirm step.
export function guardRow(view: MapView, pr: number, guard: Guard, redraw: () => void): HTMLElement {
  const project = view.project;
  logStop("guard-offered", project, pr);
  const mine = () => confirming?.project === project && confirming.pr === pr;
  const started = (started: BackLawStarted) => afterStart(started, () => logStop("guard-started", project, pr));
  const act: GuardActs = {
    review: acts.review,
    discard: acts.discard,
    confirm: acts.confirm,
    back: (law) => {
      invoke("back_a_law", { project, law, guards: pr }).then(started).catch(failure("back_a_law", "error"));
    },
    add: () => {
      const asked: NonNullable<typeof confirming> = { project, pr, draft: null, line: null };
      confirming = asked;
      redraw();
      invoke("guard_draft", { project, number: pr })
        .then((draft) => (asked.draft = draft))
        .catch((e) => Object.assign(asked, { draft: { id: "", on: guard.parts[0] ?? "", text: "" }, line: messageOf(e) }))
        .finally(() => confirming === asked && redraw());
    },
  };
  const parts = guard.parts.map((id) => ({ id, name: view.doc.parts.find((p) => p.id === id)?.name ?? id }));
  const asking = mine() ? confirming : null;
  const adding = asking
    ? confirmNodes(asking.draft, parts, asking.line, {
        start: (law) => {
          confirming = null;
          redraw();
          invoke("new_law", { project, law, guards: pr }).then(started).catch(failure("new_law", "error"));
        },
        cancel: () => {
          confirming = null;
          redraw();
        },
      })
    : null;
  return guardNodes(guardOffer(guard, pr, view.doc.laws, view.state.law_patches), view.state.law_patches, act, adding);
}
