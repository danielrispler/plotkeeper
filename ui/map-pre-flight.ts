// Pre-flight (spec danielrispler/plotkeeper#378, ticket #394): before work starts, the parts of the map it will likely cross. From the map's
// Pre-flight button or the palette, Daniel picks an open issue, writes one or two lines, or picks parts by hand. For an issue or text, the
// server asks the no-tools writer (`server/src/pre_flight.rs`); on failure the hand picker shows. The parts are framed on the map as
// *predicted* (or *picked*), and the overlay lists each with its laws on trust, its fog and its staleness band (#392). The overlay is the first
// layer Esc closes (`layerOrder`), and its field is a text field, so single-key shortcuts do not fire while Daniel types (`windowBinding`).
// A brief of a PR that closes the pre-flighted issue, or a ticket under it, carries `expectedLine` (`map-brief.ts`).

import { element, failure, invoke, text } from "./dom.js";
import { currentMap, onDraw, onPlate, registerLayer, reloadMap, shownProject, type LoadedMap } from "./map.js";
import { staleWords } from "./refresher.js";
import { logStop } from "./stop-rules.js";
import type { Expected, Fog, MapState, OpenIssue, PreFlight } from "./types.js";

// A predicted part as the overlay lists it: its name, the texts of its laws on trust, its fog, and its staleness band.
export type PreFlightRow = { id: string; name: string; trust: string[]; fog: string | null; stale: string | null };

// What the overlay shows: the inputs (with the open issues, null while they load), the wait for the writer, the hand picker (with why the
// prediction failed), or the pre-flight framed on the map.
export type Phase =
  | { kind: "start"; issues: OpenIssue[] | null }
  | { kind: "predicting" }
  | { kind: "pick"; why: string | null }
  | { kind: "result"; pre: PreFlight };

// What the overlay's buttons do.
export type PreFlightActs = {
  predict: (issue: number | null, text: string) => void;
  byHand: () => void;
  pick: (parts: string[]) => void;
  clear: () => void;
};

const fogWords: Record<Fog, string> = { thin: "fog: files changed", thick: "fog: laws, links or flows changed" };

// The parts of `pre` on the map `doc`, in the pre-flight's order, each with what Daniel should know before the work starts: a law on trust
// is a blind spot, a law backed by a check is not.
export function preFlightRows(pre: PreFlight, doc: LoadedMap, state: MapState): PreFlightRow[] {
  return pre.parts.flatMap((id) => {
    const part = doc.parts.find((each) => each.id === id);
    if (!part) return [];
    const trust = doc.laws.filter((law) => law.on === id && law.backing === "trust").map((law) => law.text);
    const mist = state.fog?.[id];
    const band = state.stale?.[id];
    return [{ id, name: part.name, trust, fog: mist ? fogWords[mist] : null, stale: band ? staleWords(band) : null }];
  });
}

const reason = (e: unknown) => (e instanceof Error ? e.message : String(e));

// What a prediction left: the pre-flight framed on the map, or the hand picker with why the writer gave nothing.
export async function settle(asked: Promise<PreFlight>): Promise<Phase> {
  try {
    return { kind: "result", pre: await asked };
  } catch (e) {
    return { kind: "pick", why: reason(e) };
  }
}

// A matched brief's line: "You expected Hook, Core; it touched Window, Hook", by the parts' names on `doc`; null for a brief with none.
export function expectedLine(expected: Expected | undefined | null, doc: LoadedMap | null): string | null {
  if (!expected) return null;
  const names = (ids: string[]) => (ids.length ? ids.map((id) => doc?.parts.find((part) => part.id === id)?.name ?? id).join(", ") : "nothing");
  return `You expected ${names(expected.expected)}; it touched ${names(expected.touched)}`;
}

// A button of the component (`btn`), with `kind` beside it, such as "primary" or "quiet".
const button = (words: string, title: string, click: () => void, kind = "") => {
  const node = text("button", words, `btn ${kind}`.trim());
  node.setAttribute("type", "button");
  node.title = title;
  node.addEventListener("click", click);
  return node;
};

// A line that says what the overlay is doing, or why it failed, announced politely (#456).
const status = (words: string, className: string) => {
  const node = text("p", words, className);
  node.setAttribute("role", "status");
  return node;
};

function startNodes(issues: OpenIssue[] | null, act: PreFlightActs): HTMLElement[] {
  const list = text("div", "", "pre-flight-issues");
  if (issues === null) list.append(status("Reading the open issues…", "empty"));
  else if (!issues.length) list.append(text("p", "No open issues.", "empty"));
  else list.append(...issues.map((issue) => button(`#${issue.number} ${issue.title}`, "Predict the parts this issue will cross", () => act.predict(issue.number, ""), "quiet pre-flight-issue")));
  const field = document.createElement("textarea");
  field.className = "pre-flight-text";
  field.rows = 2;
  field.maxLength = 400;
  field.name = "about-to-build";
  field.autocomplete = "off";
  field.placeholder = "Or one or two lines of what you are about to build…";
  field.setAttribute("aria-label", "What you are about to build");
  const predict = button("Predict", "Predict the parts these lines will cross", () => {
    const words = field.value.trim();
    if (words) act.predict(null, words);
  }, "primary");
  const foot = text("div", "", "pre-flight-foot");
  foot.append(predict, button("Pick parts by hand", "Choose the parts yourself", act.byHand, "quiet"));
  return [text("h3", "Open issues", "side-title"), list, field, foot];
}

function pickNodes(why: string | null, doc: LoadedMap, act: PreFlightActs): HTMLElement[] {
  const nodes: HTMLElement[] = [];
  if (why) nodes.push(status(`No prediction: ${why}. Pick the parts by hand.`, "pre-flight-why"));
  const boxes = doc.parts.map((part) => {
    const box = document.createElement("input");
    box.type = "checkbox";
    box.name = "part";
    box.value = part.id;
    const label = text("label", "", "pre-flight-part");
    label.append(box, text("span", part.name));
    return { box, label };
  });
  const list = text("div", "", "pre-flight-parts");
  list.append(...boxes.map(({ label }) => label));
  const show = button("Show these parts", "Frame the parts ticked on the map", () => {
    const parts = boxes.filter(({ box }) => box.checked).map(({ box }) => box.value);
    if (parts.length) act.pick(parts);
  }, "primary");
  const foot = text("div", "", "pre-flight-foot");
  foot.append(show);
  return [...nodes, list, foot];
}

function resultNodes(pre: PreFlight, doc: LoadedMap, state: MapState, act: PreFlightActs): HTMLElement[] {
  const what = pre.issue !== null ? `#${pre.issue} ${pre.title}` : pre.title;
  const nodes: HTMLElement[] = [text("p", pre.predicted ? "Predicted" : "Picked by hand", "pre-flight-kind")];
  if (what) nodes.push(text("p", what, "pre-flight-what"));
  const rows = preFlightRows(pre, doc, state).map((row) => {
    const node = text("div", "", "pre-flight-row");
    node.dataset.part = row.id;
    node.append(text("span", row.name, "pre-flight-name"));
    for (const law of row.trust) node.append(text("span", `On trust: ${law}`, "pre-flight-law"));
    if (row.fog) node.append(text("span", row.fog, "pre-flight-fog"));
    if (row.stale) node.append(text("span", row.stale, "pre-flight-stale"));
    return node;
  });
  const foot = text("div", "", "pre-flight-foot");
  foot.append(button("Clear", "Take the pre-flight off the map", act.clear, "quiet"));
  return [...nodes, ...rows, foot];
}

// The overlay's nodes for `phase` on the map `doc`.
export function preFlightNodes(phase: Phase, doc: LoadedMap, state: MapState, act: PreFlightActs): HTMLElement[] {
  const head = text("h2", "Pre-flight", "pre-flight-title");
  switch (phase.kind) {
    case "start":
      return [head, ...startNodes(phase.issues, act)];
    case "predicting":
      return [head, status("Asking the writer which parts this will cross…", "empty")];
    case "pick":
      return [head, ...pickNodes(phase.why, doc, act)];
    case "result":
      return [head, ...resultNodes(phase.pre, doc, state, act)];
  }
}

// The phase each overlay box shows.
const painted = new WeakMap<HTMLElement, Phase>();

// Draws `phase` into the overlay `box` over the map `doc` and its `state`. A map redraw (a landed push, window focus) of the same inputs
// or hand picker leaves them as they are, so what Daniel typed or ticked, and the focus, stay; when the open issues arrive, the typed lines
// and the field's focus carry over. Only the predicted parts' rows, which show the map's fog and staleness, are drawn afresh.
export function paintPreFlight(box: HTMLElement, phase: Phase, doc: LoadedMap, state: MapState, act: PreFlightActs) {
  const was = painted.get(box);
  box.hidden = false;
  if (was === phase && phase.kind !== "result") return;
  const field = box.querySelector?.<HTMLTextAreaElement>("textarea.pre-flight-text") ?? null;
  const typed = was?.kind === "start" && phase.kind === "start" && field ? { value: field.value, focused: document.activeElement === field } : null;
  box.replaceChildren(...preFlightNodes(phase, doc, state, act));
  painted.set(box, phase);
  const next = typed && box.querySelector?.<HTMLTextAreaElement>("textarea.pre-flight-text");
  if (typed && next) {
    next.value = typed.value;
    if (typed.focused) next.focus();
  }
}

// The overlay as it is now; null while closed.
let phase: Phase | null = null;
const overlay = () => element("map-pre-flight");

function paint() {
  const view = currentMap();
  const box = overlay();
  // A past commit's map, or none, closes the overlay, so Esc never peels one that is not shown.
  if (!view || view.scrub) phase = null;
  if (phase === null || !view) {
    box.hidden = true;
    box.replaceChildren();
    painted.delete(box);
    return;
  }
  paintPreFlight(box, phase, view.doc, view.state, acts);
}

function show(next: Phase | null) {
  phase = next;
  paint();
}

// A pre-flight framed: logged for the stop rules, and the map read again, so its plates are marked.
function framed(pre: PreFlight, project: string) {
  logStop("pre-flight-run", project);
  void reloadMap();
  return pre;
}

const acts: PreFlightActs = {
  predict: (issue, words) => {
    const project = shownProject();
    if (project === null) return;
    show({ kind: "predicting" });
    void settle(invoke("pre_flight_predict", { project, issue, text: issue === null ? words : null }).then((pre) => framed(pre, project))).then((next) => {
      if (phase?.kind === "predicting" && shownProject() === project) show(next);
    });
  },
  byHand: () => show({ kind: "pick", why: null }),
  pick: (parts) => {
    const project = shownProject();
    if (project === null) return;
    invoke("pre_flight_pick", { project, parts })
      .then((pre) => show({ kind: "result", pre: framed(pre, project) }))
      .catch(failure("pre_flight_pick", "error"));
  },
  clear: () => {
    const project = shownProject();
    if (project === null) return;
    invoke("pre_flight_clear", { project })
      .then(() => (reloadMap(), openPreFlight(true)))
      .catch(failure("pre_flight_clear", "error"));
  },
};

// Opens the overlay on the map shown: its pre-flight when one is framed, else the inputs; again, it closes. `fresh` starts anew.
export function openPreFlight(fresh = false) {
  const view = currentMap();
  const project = shownProject();
  if (!fresh && phase !== null) return show(null);
  if (!view || project === null || view.scrub) return;
  const pre = view.state.pre_flight;
  if (pre && !fresh) return show({ kind: "result", pre });
  show({ kind: "start", issues: null });
  invoke("pre_flight_issues", { project })
    .then((issues) => {
      if (phase?.kind === "start" && shownProject() === project) show({ kind: "start", issues });
    })
    .catch((e) => {
      failure("pre_flight_issues")(e);
      if (phase?.kind === "start" && shownProject() === project) show({ kind: "start", issues: [] });
    });
}

// The parts of the project's pre-flight are framed on the map, marked predicted or picked; a past commit's map shows none.
// The word sits among the law marks, since the live agents' chips replace their slot's children.
onPlate(({ view, part, plate, marks }) => {
  const pre = view.state.pre_flight;
  if (view.scrub || !pre?.parts.includes(part.id)) return;
  plate.classList.add("predicted");
  marks.prepend(text("span", pre.predicted ? "predicted" : "picked", "plate-predicted"));
});
// Another project, or a past commit, closes the overlay; otherwise it is drawn again over the new payload.
let drawnFor: string | null = null;
onDraw((view) => {
  if (view.project !== drawnFor) phase = null;
  drawnFor = view.project;
  paint();
});
registerLayer("pre-flight", { open: () => phase !== null, close: () => show(null) });
