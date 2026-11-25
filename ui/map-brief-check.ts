// The PR brief's check (#340, spec #548 Group 4): one prediction per step, answered in the panel (a pick) or by tapping parts on the
// map into numbered slots (an order). Check grades it, a wrong answer shows the correction, and an order check replays the right path.

import { hint, pressable, text } from "./dom.js";
import { currentMap, type MapView } from "./map.js";
import { orderRun, type OrderRun, pickRun, type PickRun } from "./map-brief-model.js";
import { logStop } from "./stop-rules.js";
import { gateOf, panel, player, prOf, steps } from "./map-brief-player.js";
import { clearOrder, plateOf, reduced, runLight, stopLight, world } from "./map-brief-light.js";
import type { BriefCheck, BriefStep } from "./types.js";

type Run = { kind: "pick"; run: PickRun } | { kind: "order"; run: OrderRun };
// A check keeps its answer while the brief is open, so a redraw of the map or a step back shows it as it was.
const runs = new WeakMap<BriefCheck, Run>();
export function runOf(view: MapView, check: BriefCheck): Run | null {
  let found = runs.get(check);
  if (!found && check.kind === "pick") found = { kind: "pick", run: pickRun(Number(check.key)) };
  if (!found && check.kind === "order") found = { kind: "order", run: orderRun(Array.isArray(check.key) ? (check.key as string[]) : [], view.doc.links) };
  if (found) runs.set(check, found);
  return found ?? null;
}
const nameOf = (view: MapView, id: string) => view.doc.parts.find((p) => p.id === id)?.name ?? id;
const keyOf = (check: BriefCheck | null) => (check && Array.isArray(check.key) ? (check.key as string[]) : []);

// What an order check lets Daniel tap: the parts the step lights, and the ones of the key.
const tappable = (view: MapView, step: BriefStep) => new Set([...step.on, ...keyOf(step.check)].filter((id) => view.doc.parts.some((p) => p.id === id)));

/** The panel's part of a check, drawn from its run: the question (the answer's words once graded), the options or the numbered slots. */
export function checkNode(view: MapView, step: BriefStep, check: BriefCheck) {
  const box = text("div", "", "brief-check");
  const found = runOf(view, check);
  if (!found) {
    box.append(text("p", check.q, "brief-body"));
    return box;
  }
  const done = found.run.answered();
  const right = found.kind === "pick" ? found.run.picked() === Number(check.key) : keyOf(check).every((id, i) => id === found.run.picked()[i]);
  // The question is the step's own heading when the writer put it there; then only the answer's words add to it.
  if (done || check.q !== step.h) box.append(text("p", done ? (right ? check.yes : check.no) : check.q, "brief-body"));
  if (found.kind === "pick") {
    const { run } = found;
    const row = text("div", "", "brief-opts");
    check.opts.forEach((opt, i) => {
      const button = pressable(text("button", opt, "btn brief-opt"), null);
      hint(button, "Pick this answer; press it again to take it back");
      button.setAttribute("aria-pressed", String(run.picked() === i));
      if (done) button.classList.add(i === Number(check.key) ? "right" : "miss");
      button.addEventListener("click", () => {
        run.tap(i);
        redrawCheck(view, step);
      });
      row.append(button);
    });
    box.append(row);
  } else {
    // The slots show the parts as tapped; once graded they show the key, which the map replays.
    const shown = done ? keyOf(check) : found.run.picked();
    const row = text("div", "", "brief-slots");
    keyOf(check).forEach((_, i) => {
      const slot = text("span", "", `brief-slot${shown[i] ? " full" : ""}`);
      slot.append(text("b", String(i + 1)), text("span", shown[i] ? nameOf(view, shown[i]) : "tap a part"));
      row.append(slot);
    });
    box.append(row);
  }
  return box;
}

// Redraws the check where it is, the Check button and the map's badges with it.
function redrawCheck(view: MapView, step: BriefStep) {
  if (step.check && runs.get(step.check)?.run.answered()) gateOf()?.answered();
  const old = panel().querySelector(".brief-check");
  if (old && step.check) old.replaceWith(checkNode(view, step, step.check));
  paintOrder(view, step);
  syncNext(step);
}

/** Marks an order check on the map: the parts that can be tapped, and a number on each part tapped. */
export function paintOrder(view: MapView, step: BriefStep) {
  clearOrder(view);
  const found = step.check?.kind === "order" ? runOf(view, step.check) : null;
  if (found?.kind !== "order") return;
  const open = tappable(view, step);
  const answered = found.run.answered();
  const shown = answered ? keyOf(step.check) : found.run.picked();
  open.forEach((id) => plateOf(view, id)?.classList.toggle("pick", !answered));
  shown.forEach((id, i) => {
    const plate = plateOf(view, id);
    plate?.classList.add("picked");
    plate?.append(text("span", String(i + 1), "ord"));
  });
}

/** A tap on a part while an order check is open picks it, instead of opening its sheet. */
export function wireTaps(view: MapView) {
  const canvas = view.slots.canvas;
  if (canvas.dataset.briefTaps) return;
  canvas.dataset.briefTaps = "yes";
  canvas.addEventListener(
    "click",
    (event) => {
      const now = player.current();
      const step = now ? steps()[now.step] : undefined;
      const current = currentMap();
      if (step?.check?.kind !== "order" || !current) return;
      const plate = (event.target as HTMLElement).closest<HTMLElement>(".plate");
      if (!plate) return;
      event.stopPropagation();
      const found = runOf(current, step.check);
      const id = plate.dataset.id;
      if (found?.kind !== "order" || found.run.answered() || id === undefined || !tappable(current, step).has(id)) return;
      found.run.tap(id);
      redrawCheck(current, step);
    },
    true,
  );
}

// A check was graded, right or wrong.
const answeredCheck = () => {
  const now = player.current();
  if (now) logStop("check-answered", now.project, prOf(now.pr));
};

/** Check on a check: graded, its right answer shown, and an order check's right path replayed along the links. */
export function grade(view: MapView, step: BriefStep) {
  const found = step.check ? runOf(view, step.check) : null;
  if (!found?.run.ready()) return;
  if (found.kind === "pick") {
    found.run.grade();
    answeredCheck();
    return redrawCheck(view, step);
  }
  const result = found.run.grade();
  if (!result) return;
  answeredCheck();
  const wrong = result.wrong;
  redrawCheck(view, step);
  // The parts out of place nudge once; then the right path runs.
  if (wrong.length && !reduced()) {
    wrong.forEach((id) => plateOf(view, id)?.classList.add("wrong"));
    setTimeout(() => replay(view, found.run), 420);
  } else {
    replay(view, found.run);
  }
}
function replay(view: MapView, run: OrderRun) {
  if (!player.current()) return;
  view.slots.canvas.querySelectorAll(".wrong").forEach((node) => node.classList.remove("wrong"));
  stopLight();
  world(view)?.querySelector(".map-light")?.remove();
  void runLight(view, run.replay());
}

/** The Next button is Check while the step's check waits for its answer. */
export function syncNext(step: BriefStep) {
  const next = document.getElementById("brief-next") as HTMLButtonElement | null;
  const list = steps();
  if (!next) return;
  const found = step.check ? runs.get(step.check) : undefined;
  const waiting = !!step.check && !found?.run.answered();
  const last = player.current()?.step === list.length - 1;
  next.textContent = waiting ? "Check" : last ? "Done" : "Next";
  next.dataset.check = waiting ? (step.check?.kind ?? "") : "";
  next.disabled = waiting && !found?.run.ready();
}

/** Whether `step`'s check has been tried and waits for its answer: Next grades it first. */
export function checkWaits(step: BriefStep): boolean {
  const found = step.check ? runs.get(step.check) : undefined;
  return !!found && !found.run.answered();
}
