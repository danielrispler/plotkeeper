// What sits beside the map: the Ready for review tray, background runs, pre-flight, back a law and micro-worlds.

import { base, type Fixture } from "./base.ts";
import { confirmFirst, mapFile, mapOpened, mapState, runAt } from "./fixtures-shared.ts";

/** The Ready for review tray (#336) with two ready PRs, at the Map view's foot. */
export const mapTray = (): Fixture =>
  mapOpened(`
      const rows = [...document.querySelectorAll("#map-tray .tray-row")];
      if (rows.length !== 2) return log("snap:fail expected 2 tray rows, saw " + rows.length);
      if (rows[0].tagName !== "BUTTON" || rows[1].tagName !== "BUTTON") log("snap:fail a ready PR opens, written or not");
      if (!document.querySelector("#map-tray .side-title")?.textContent.includes("Ready for review")) log("snap:fail the tray has no title");
      if (!rows[0].textContent.includes("#334") || !rows[0].textContent.includes("brief ready")) log("snap:fail row text: " + rows[0].textContent);
      if (!rows[1].textContent.includes("written when you open it")) log("snap:fail row text: " + rows[1].textContent);
      log("tray ok");`, {
    map_state: mapState({ fog: { shell: "thick" }, ready: [{ number: 334, title: "PR briefs: the brief written for each ready PR", head: "b".repeat(40), brief: { stale: false, writing: false, chapters: ["PR briefs on the map"], quiet: false } }, { number: 336, title: "Ready for review tray", head: "a".repeat(40), brief: null }] }),
  });

/**
 * After a push (#477, part B): one PR's read carried over a pure rebase, another rewritten after a partial read; each row says why. A
 * third has a "Since you read" story of two chapters (#497): a row for each, then the full brief's.
 */
export const mapTrayAfterPush = (): Fixture =>
  mapOpened(`
      const notes = [...document.querySelectorAll("#map-tray .tray-row .tray-note")].map((n) => n.textContent);
      if (notes.length !== 5) return log("snap:fail expected 5 tray rows, saw " + notes.length);
      if (notes[0] !== "#334 · Rebased onto main, nothing new in the PR") log("snap:fail the carried row: " + notes[0]);
      if (notes[1] !== "#336 · Rewritten after a new push; you had read 1 of 2") log("snap:fail the rewritten row: " + notes[1]);
      if (notes[3] !== "#337 The push since you read · since you read · 2 of 2") log("snap:fail the story's second row: " + notes[3]);
      log("tray notes ok");`, {
    map_state: mapState({
      ready: [
        { number: 334, title: "PR briefs: the brief written for each ready PR", head: "b".repeat(40), brief: { stale: false, writing: false, chapters: ["PR briefs on the map"], quiet: false, read: true, note: "Rebased onto main, nothing new in the PR" } },
        { number: 336, title: "Ready for review tray", head: "a".repeat(40), brief: { stale: false, writing: false, chapters: ["The tray"], quiet: false, note: "Rewritten after a new push; you had read 1 of 2" } },
        { number: 337, title: "Since you read", head: "c".repeat(40), brief: { stale: false, writing: false, chapters: ["The story"], quiet: false, since: "The push since you read", since_chapters: ["The read carries", "The fog clears"] } },
      ],
    }),
  });

export const mapRuns = (): Fixture =>
  mapOpened(`
      const sent = [];
      const real = window.fetch;
      window.fetch = (url, init) => (sent.push(String(url) + " " + (init?.body ?? "")), real(url, init));
      const rows = [...document.querySelectorAll("#map-runs .run-row")];
      if (rows.length !== 4) return log("snap:fail expected 4 run rows, saw " + rows.length);
      if (!document.querySelector("#map-runs .side-title")?.textContent.includes("Background runs")) log("snap:fail the runs have no title");
      if (rows[0].dataset.state !== "running" || !rows[0].textContent.includes("Running since")) log("snap:fail the running row: " + rows[0].textContent);
      if (!rows[1].textContent.includes("Queued, next") || !rows[2].textContent.includes("Micro-world")) log("snap:fail the queue: " + rows[1].textContent + " / " + rows[2].textContent);
      if (!rows[3].textContent.includes("1 tool call was denied") || !rows[3].textContent.includes("git push")) log("snap:fail the denied run: " + rows[3].textContent);
      const words = (row) => [...row.querySelectorAll("button")].map((button) => button.textContent).join(",");
      if (words(rows[0]) !== "Cancel" || words(rows[1]) !== "Cancel" || words(rows[3]) !== "Copy prompt,Dismiss") log("snap:fail the buttons: " + rows.map(words).join(" / "));
      if (document.querySelector("#map-runs .runs-paused")) log("snap:fail a pause shows with no pause");
      const press = (id, words) => [...document.querySelectorAll(\`#map-runs .run-row[data-run="\${id}"] button\`)].find((b) => b.textContent === words).click();
      press("19a2f-2", "Cancel");
      press("19a2f-0", "Dismiss");
      await new Promise((done) => setTimeout(done, 50));
      if (document.getElementById("confirm-dialog").open) log("snap:fail a queued run's Cancel or an ended run's Dismiss asked first");
      if (!sent.some((call) => call.includes("/api/run_cancel") && call.includes("19a2f-2"))) log("snap:fail Cancel on a queued run did not ask the server at once: " + sent.join(" | "));
      if (!sent.some((call) => call.includes("/api/run_cancel") && call.includes("19a2f-0"))) log("snap:fail Dismiss did not ask the server at once: " + sent.join(" | "));
      ${confirmFirst(`press("19a2f-1", "Cancel")`, "run_cancel", "Cancel run", ["worktree", "changes"])}
      if (!sent.some((call) => call.includes("/api/run_cancel") && call.includes("19a2f-1"))) log("snap:fail confirmed, Cancel did not ask the server for the running run: " + sent.join(" | "));
      log("runs ok");`, {
    map_state: mapState({
      runs: {
        paused: null,
        runs: [
          { id: "19a2f-1", kind: "back-law", state: "running", at: runAt, outcome: null, line: null, denials: [], prompt: null },
          { id: "19a2f-2", kind: "back-law", state: "queued", at: runAt, outcome: null, line: null, denials: [], prompt: null },
          { id: "19a2f-3", kind: "micro-world", state: "queued", at: runAt, outcome: null, line: null, denials: [], prompt: null },
          { id: "19a2f-0", kind: "back-law", state: "ended", at: runAt, outcome: "denied", line: "Finished, but 1 tool call was denied", denials: ["Bash: git push origin HEAD"], prompt: "Write a test tagged @law: core-order." },
        ],
      },
    }),
    snapDone: "runs ok",
  });
/**
 * Pre-flight (#394) of spec #378: Core and Understanding framed on the map as predicted, and the overlay listing each with its laws on trust,
 * its fog and its staleness band, opened from the map's Pre-flight button.
 */
export const mapPreFlight = (): Fixture =>
  mapOpened(`
      const predicted = [...document.querySelectorAll(".plate.predicted")].map((plate) => plate.dataset.id);
      if (predicted.join(",") !== "core,understanding") return log("snap:fail the predicted plates: " + predicted.join(","));
      if (!document.querySelector('.plate[data-id="core"] .plate-predicted')?.textContent.includes("predicted")) log("snap:fail the plate has no predicted chip");
      // Plate text a size larger (#669): the caption at the body size, the predicted word at the small size; the plate keeps its size.
      const core = document.querySelector('.plate[data-id="core"]');
      const sizeOf = (selector) => getComputedStyle(core.querySelector(selector)).fontSize;
      if (sizeOf(".plate-caption") !== "14px" || sizeOf(".plate-predicted") !== "12.5px") log("snap:fail the plate's caption is " + sizeOf(".plate-caption") + " and its predicted word " + sizeOf(".plate-predicted"));
      if (core.offsetWidth !== 264 || core.offsetHeight !== 76) log("snap:fail the plate is " + core.offsetWidth + "x" + core.offsetHeight + ", not 264x76");
      document.getElementById("pre-flight-button").click();
      // Past the glass's rise, so the overlay is drawn whole.
      await wait(600);
      const box = document.getElementById("map-pre-flight");
      if (box.hidden) return log("snap:fail the pre-flight did not open");
      if (!box.textContent.includes("#378 Studio: Spec: the modules")) log("snap:fail the issue: " + box.textContent);
      const rows = [...box.querySelectorAll(".pre-flight-row")];
      if (rows.length !== 2) return log("snap:fail expected 2 rows, saw " + rows.length);
      if (!rows[0].textContent.includes("On trust: Events apply in order.") || rows[0].textContent.includes("The core reads no file")) log("snap:fail the core's laws: " + rows[0].textContent);
      if (!rows[1].textContent.includes("fog: laws, links or flows changed")) log("snap:fail the understanding's fog: " + rows[1].textContent);
      if (!rows[0].textContent.includes("weeks or more ago")) log("snap:fail the core's staleness band: " + rows[0].textContent);
      log("pre-flight ok");`, {
    map_state: mapState({
      fog: { understanding: "thick" },
      stale: { core: 1 },
      pre_flight: { issue: 378, title: "Studio: Spec: the modules", parts: ["core", "understanding"], predicted: true, at: runAt },
    }),
    snapDone: "pre-flight ok",
  });
/** #456: the pre-flight's reading, predicting and failed states are announced politely (role="status"). */
export const mapPreFlightStatus = (): Fixture =>
  mapOpened(`
      const box = document.getElementById("map-pre-flight");
      // Each status shows as a line and is said in the overlay's one status region, the same node from the first (#452's review).
      let first = null;
      const status = (words, why) => {
        const node = [...box.querySelectorAll("p:not([role=status])")].find((each) => each.textContent.includes(words));
        if (!node) return log("snap:fail " + why + ": no line with " + JSON.stringify(words) + " in " + JSON.stringify(box.textContent));
        const live = [...box.querySelectorAll("[role=status], [aria-live]")];
        first ??= live[0];
        if (live.length !== 1 || live[0] !== first || !live[0].isConnected) log("snap:fail " + why + ": the overlay's status region is not the one it had");
        if (!live[0]?.textContent.includes(words)) log("snap:fail " + why + " is not said: the region holds " + JSON.stringify(live[0]?.textContent));
      };
      document.getElementById("pre-flight-button").click();
      // A region new to the overlay takes its first words a moment after it is drawn.
      await wait(200);
      status("Reading the open issues", "the issues' reading");
      await wait(400);
      const issue = box.querySelector(".pre-flight-issue");
      if (!issue) return log("snap:fail the open issues never came");
      issue.click();
      await wait(50);
      status("Asking the writer", "the prediction's wait");
      await wait(500);
      status("No prediction", "the failed prediction");
      log("pre-flight status ok");`, {
    map_state: mapState(),
    pre_flight_issues: { __delay: 300, value: base.pre_flight_issues } as never,
    pre_flight_predict: { __delay: 200, value: { __throw: "the writer is not set" } } as never,
    snapDone: "pre-flight status ok",
  });
// Back a law (#398): Hook's law on trust offers Back a law; Core's law on trust has a proven patch, and Review opens the patch review over
// the map with the test patch, the output with the test alone and with the plant, and Discard.
const lawPatchDiff = [
  "diff --git a/core/tests/order.rs b/core/tests/order.rs",
  "new file mode 100644",
  "--- /dev/null",
  "+++ b/core/tests/order.rs",
  "@@ -0,0 +1,6 @@",
  "+// @law: core-order",
  "+#[test]",
  "+fn events_apply_in_order() {",
  "+    let state = apply(&[started(), ended()]);",
  "+    assert_eq!(state.last(), Some(Ended));",
  "+}",
  "diff --git a/system-map.json b/system-map.json",
  "--- a/system-map.json",
  "+++ b/system-map.json",
  "@@ -4,1 +4,1 @@",
  '-    { "id": "core-order", "on": "core", "backing": "trust", "text": "Events apply in order." },',
  '+    { "id": "core-order", "on": "core", "backing": "check", "text": "Events apply in order." },',
].join("\n");
export const mapBackLaw = (): Fixture =>
  mapOpened(`
      document.querySelector("#map .plate[data-id=hook]").click();
      await wait(300);
      const held = document.querySelector('#map-sheet .sheet-law[data-law="hook-held"]');
      if (![...(held?.querySelectorAll("button") ?? [])].some((b) => b.textContent === "Back a law")) return log("snap:fail Hook's law on trust has no Back a law");
      document.querySelector("#map .plate[data-id=core]").click();
      await wait(300);
      const pure = document.querySelector('#map-sheet .sheet-law[data-law="core-pure"]');
      if ([...pure.querySelectorAll("button")].some((b) => b.textContent === "Back a law")) log("snap:fail a law a check backs offers Back a law");
      const review = [...document.querySelectorAll('#map-sheet .sheet-law[data-law="core-order"] button')].find((b) => b.textContent === "Review the patch");
      if (!review) return log("snap:fail the proven patch has no Review");
      review.click();
      await wait(300);
      const box = document.getElementById("map-patch-review");
      if (box.hidden) return log("snap:fail the patch review did not open");
      if (!box.textContent.includes("Events apply in order.") || !box.textContent.includes("events_apply_in_order in core/tests/order.rs")) log("snap:fail the law or the test: " + box.textContent);
      if (box.querySelectorAll(".patch-diff .diff-line.add").length !== 7) log("snap:fail the test patch's added lines: " + box.querySelectorAll(".patch-diff .diff-line.add").length);
      const outputs = [...box.querySelectorAll(".patch-output")].map((pre) => pre.textContent);
      if (outputs.length !== 2 || !outputs[0].includes("PASS") || !outputs[1].includes("FAIL")) log("snap:fail the outputs: " + outputs.join(" / "));
      if ([...box.querySelectorAll("button")].map((b) => b.textContent).join(",") !== "Open draft PR,Discard,Close") log("snap:fail the review's buttons");
      // A real Esc in the confirm closes the confirm alone: the window's Esc, which peels the map's layers, never sees it.
      [...box.querySelectorAll("button")].find((b) => b.textContent === "Discard").click();
      await wait(100);
      await __snapKey("Escape");
      await wait(100);
      if (document.getElementById("confirm-dialog").open) log("snap:fail a real Esc did not close the confirm");
      if (box.hidden) log("snap:fail a real Esc in the confirm also closed the patch review under it");
      ${confirmFirst(`[...document.querySelectorAll("#map-patch-review button")].find((b) => b.textContent === "Discard").click()`, "law_patch_discard", "Discard patch", ["patch"])}
      log("back a law ok");`, {
    map_state: mapState({
      law_patches: [
        {
          id: "19a2f-5", project: "/w/plotkeeper", law: "core-order", state: "proven", line: null, at: runAt, diff: lawPatchDiff,
          proof: {
            test: "events_apply_in_order", file: "core/tests/order.rs",
            passed: "$ cargo nextest run -p studio-core\n        PASS [   0.004s] studio-core::it order::events_apply_in_order\n     Summary [   0.912s] 214 tests run: 214 passed, 0 skipped",
            failed: "$ cargo nextest run -p studio-core\n        FAIL [   0.004s] studio-core::it order::events_apply_in_order\n     Summary [   0.917s] 214 tests run: 213 passed, 1 failed, 0 skipped",
          },
        },
      ],
    }),
    realKeys: true,
    snapDone: "back a law ok",
  });
/** The queues paused by a login failure: the one indicator with Resume, over the run that failed and the one that waits. */
export const mapRunsPaused = (): Fixture =>
  mapOpened(`
      const paused = document.querySelectorAll("#map-runs .runs-paused");
      if (paused.length !== 1 || !paused[0].textContent.includes("Background runs are paused")) return log("snap:fail the pause's indicator: " + paused.length);
      if (paused[0].querySelector("button")?.textContent !== "Resume") log("snap:fail the indicator has no Resume");
      const rows = [...document.querySelectorAll("#map-runs .run-row")];
      if (!rows[1]?.textContent.includes("Queued, paused")) log("snap:fail the waiting run: " + rows[1]?.textContent);
      log("paused ok");`, {
    map_state: mapState({
      runs: {
        paused: "Claude is not logged in: run `claude` in a terminal and log in with /login",
        runs: [
          { id: "19a2f-1", kind: "back-law", state: "ended", at: runAt, outcome: "not-logged-in", line: "Claude is not logged in: run `claude` in a terminal and log in with /login", denials: [], prompt: "Write a test tagged @law: core-order." },
          { id: "19a2f-2", kind: "back-law", state: "queued", at: runAt, outcome: null, line: null, denials: [], prompt: null },
        ],
      },
    }),
    snapDone: "paused ok",
  });
/**
 * #472: with a part's sheet open and the side column scrolled to its end, the background runs below it are on the sheet's glass, since the
 * map pans under them.
 */
export const mapRunsBesideSheet = (): Fixture => ({
  ...mapRunsPaused(),
  snapScript: mapOpened(`
      document.querySelector("#map .plate[data-id=shell]").click();
      await wait(300);
      if (!document.querySelector("#map-sheet .sheet")) return log("snap:fail the sheet did not open");
      const runs = document.getElementById("map-runs");
      if (!runs.children.length) return log("snap:fail no background runs drawn");
      // The side column scrolls as one (#445), so the runs show at its end: scroll there, and measure the part of them it shows.
      runs.parentElement.scrollTop = runs.parentElement.scrollHeight;
      await wait(100);
      const side = runs.parentElement.getBoundingClientRect();
      const own = runs.getBoundingClientRect();
      const at = { left: own.left, right: own.right, top: Math.max(own.top, side.top), bottom: Math.min(own.bottom, side.bottom) };
      if (at.bottom - at.top < own.height) return log("snap:fail the side column's end shows " + Math.round(at.bottom - at.top) + " of the runs' " + Math.round(own.height) + " px");
      // The map pans under the side column, so the runs can cross a plate, as the sheet can; where they do, they are on the sheet's glass.
      const over = [...document.querySelectorAll("#map .plate")].filter((plate) => {
        const box = plate.getBoundingClientRect();
        return at.left < box.right && box.left < at.right && at.top < box.bottom && box.top < at.bottom;
      });
      // Whether they cross one here depends on the window's size (at 1900x900 they cross Window, at snap:all's 1900x1180 none), so the
      // glass is asserted at any size and the plates crossed are only said.
      const style = getComputedStyle(runs);
      if (!style.backdropFilter.includes("blur") || style.backgroundColor !== getComputedStyle(document.getElementById("map-sheet")).backgroundColor)
        log("snap:fail the background runs are not on the sheet's glass, over " + (over.map((plate) => plate.dataset.id).join(", ") || "no plate here"));
      log("over " + (over.map((plate) => plate.dataset.id).join(", ") || "no plate"));
      log("runs beside sheet ok");`).snapScript,
  snapDone: "runs beside sheet ok",
});
/**
 * #465: the pause is said in the runs area's one status region, which a redraw (Resume reads the map back) keeps in place; a copied
 * prompt is said once in the page's action region, drawn empty before its words, and the row's note itself is not live.
 */
export const mapRunsAnnounced = (): Fixture => ({
  ...mapRunsPaused(),
  snapScript: mapOpened(`
      const runs = document.getElementById("map-runs");
      const live = [...runs.children].filter((node) => node.getAttribute("role") === "status");
      if (live.length !== 1 || !live[0].classList.contains("visually-hidden")) return log("snap:fail the runs area has " + live.length + " status regions, not one hidden");
      if (!live[0].textContent.includes("Background runs are paused. Claude is not logged in")) log("snap:fail the runs' region says " + JSON.stringify(live[0].textContent));
      if (runs.querySelectorAll("[role=status], [aria-live]").length !== 1) log("snap:fail another line of the runs area is live");
      let left = false;
      new MutationObserver((changes) => (left ||= changes.some((change) => [...change.removedNodes].includes(live[0])))).observe(runs, { childList: true });
      const said = [];
      const action = () => [...document.body.children].find((node) => node.getAttribute("role") === "status");
      new MutationObserver(() => {
        const now = action()?.textContent;
        if (now !== undefined && said.at(-1) !== now) said.push(now);
      }).observe(document.body, { childList: true, subtree: true, characterData: true });
      if (action()) log("snap:fail the action region is there before any action");
      Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: () => Promise.resolve() } });
      [...runs.querySelectorAll(".run-row button")].find((b) => b.textContent === "Copy prompt").click();
      await wait(400);
      const line = runs.querySelector(".run-copied");
      if (!line) return log("snap:fail the copy left no note");
      if (line.hasAttribute("role") || line.hasAttribute("aria-live")) log("snap:fail the copy's note is live itself");
      if (said.length !== 2 || said[0] !== "" || said[1] !== "Prompt copied, paste it into your agent") log("snap:fail the action region said " + JSON.stringify(said) + ", not empty then the copy's line");
      const region = action();
      [...runs.querySelectorAll(".runs-paused button")].find((b) => b.textContent === "Resume").click();
      await wait(400);
      const now = [...runs.children].filter((node) => node.getAttribute("role") === "status");
      if (now.length !== 1 || now[0] !== live[0] || left) log("snap:fail the runs' region did not stay in the area across the redraw");
      if (action() !== region || said.length !== 2) log("snap:fail the redraw touched the action region: " + JSON.stringify(said));
      log("announced ok");`).snapScript,
  snapDone: "announced ok",
});

// A micro-world (#399) in its overlay: from the refresher on Shell's sheet, the Turn writing flow's world is ready but may be out of date, so its
// row offers Open and Rebuild; Open shows the checked page in a frame sandboxed to scripts alone, in the glass overlay the size of the settings dialog. The page's script is
// an inline handler: a closing script tag would end the snap page's own.
const worldPage = `<!doctype html><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:">
<title>Turn writing</title><style>:root{color-scheme:light dark}body{margin:0;padding:24px;font:15px system-ui;background:#f6f5f2;color:#1a1a1a}
@media (prefers-color-scheme: dark){body{background:#17181b;color:#e8e6e1}}.hops{display:flex;gap:10px;flex-wrap:wrap;margin:16px 0}
.hop{padding:8px 12px;border-radius:8px;border:1px solid #8884}.hop.on{background:#4a7dff;color:#fff;border-color:#4a7dff}</style>
<h1>Turn writing, as a model</h1><p>An illustrative model of how a turn's summary is written. Step through the hops.</p>
<div class="hops"><span class="hop on">Hook</span><span class="hop">Shell</span><span class="hop">Understanding</span><span class="hop">Writer</span><span class="hop">Window</span></div>
<button onclick="const hops=[...document.querySelectorAll('.hop')];const at=hops.findIndex((h)=>h.classList.contains('on'));hops[at].classList.remove('on');hops[(at+1)%hops.length].classList.add('on')">Next hop</button>
<p>The hook's line reaches the Shell, which reads the log.</p>`;
export const mapMicroWorld = (): Fixture => {
  const file = JSON.parse(mapFile);
  file.flows = [{ id: "turn-writing", name: "Turn writing", hops: [
    { link: "hook-shell", text: "The hook's line reaches the Shell, which reads the log." },
    { link: "shell-window", text: "The Shell tells the Window, which shows the summary." },
  ] }];
  return mapOpened(`
      document.querySelector("#map .plate[data-id=shell]").click();
      await wait(300);
      document.querySelector("#map-sheet .sheet-refresher")?.click();
      await wait(400);
      const row = document.querySelector('#map-sheet .micro-world-row[data-world="flow:turn-writing"]');
      if (!row) return log("snap:fail the flow has no micro-world row");
      const words = [...row.querySelectorAll("button")].map((button) => button.textContent).join(",");
      if (words !== "Open,Rebuild") log("snap:fail the row offers " + words);
      if (!row.textContent.includes("may be out of date")) log("snap:fail the row does not say it may be out of date");
      row.querySelector("button").click();
      await wait(600);
      const box = document.getElementById("map-micro-world");
      if (box.hidden) return log("snap:fail the micro-world did not open");
      const frame = box.querySelector("iframe");
      if (!frame) return log("snap:fail no frame");
      if (frame.getAttribute("sandbox") !== "allow-scripts") log("snap:fail the frame's sandbox: " + frame.getAttribute("sandbox"));
      if (frame.hasAttribute("src") || !frame.getAttribute("srcdoc")?.includes("Content-Security-Policy")) log("snap:fail the frame is not the checked page by srcdoc");
      if (!box.querySelector(".micro-world-title")?.textContent.includes("Turn writing")) log("snap:fail the title: " + box.textContent);
      if (box.getBoundingClientRect().width < 600) log("snap:fail the overlay is not wide: " + box.getBoundingClientRect().width);
      log("micro-world ok");`, {
    map_state: mapState({
      source: JSON.stringify(file),
      stale: { shell: 2 },
      micro_worlds: { on: true, worlds: { "flow:turn-writing": { phase: "ready", page: true, stale: true, line: null, prompt: null } } },
    }),
    micro_world_page: worldPage,
    snapDone: "micro-world ok",
  });
};
/**
 * A micro-world that is still building (spec #582, ticket 2): its row's "Building…" is `aria-disabled`, not `disabled`, so it stays
 * focusable; the pointer on it opens its reason as a hint, and a click on it changes nothing.
 */
export const mapMicroWorldBuilding = (): Fixture => {
  const file = JSON.parse(mapFile);
  file.flows = [{ id: "turn-writing", name: "Turn writing", hops: [{ link: "hook-shell", text: "The hook's line reaches the Shell, which reads the log." }] }];
  const f = mapOpened(`
      document.querySelector("#map .plate[data-id=shell]").click();
      await wait(300);
      document.querySelector("#map-sheet .sheet-refresher")?.click();
      await wait(300);
      const row = document.querySelector('#map-sheet .micro-world-row[data-world="flow:turn-writing"]');
      const wait1 = row?.querySelector("button");
      if (!wait1) return log("snap:fail the building row has no button");
      if (wait1.disabled || wait1.getAttribute("aria-disabled") !== "true") log("snap:fail Building… is not aria-disabled alone");
      const hint = document.getElementById(wait1.getAttribute("interestfor"));
      wait1.scrollIntoView({ block: "center" });
      await __snapMove(wait1);
      await wait(700);
      if (!hint?.matches(":popover-open") || hint.textContent !== "A background run is building it") log("snap:fail the reason did not show as a hint on hover");
      const before = row.innerHTML;
      await __snapClick(wait1);
      await wait(100);
      if (row.innerHTML !== before || !document.getElementById("map-micro-world").hidden) log("snap:fail a click on Building… changed something");
      log("building ok");`, {
    map_state: mapState({
      source: JSON.stringify(file),
      stale: { shell: 2 },
      micro_worlds: { on: true, worlds: { "flow:turn-writing": { phase: "building", page: false, stale: false, line: null, prompt: null } } },
    }),
  });
  return { ...f, realKeys: true, realPointer: true, snapDone: "building ok" };
};
/**
 * The dark page (#455): the metas, the root's colour scheme, no ::-webkit-scrollbar rule left, and the pre-flight's hand picker,
 * whose native checkbox draws dark, sampled from Chrome's own screenshot.
 */
export const mapPreFlightPick = (): Fixture => ({
  ...mapOpened(`
    const meta = (name) => document.querySelector('meta[name="' + name + '"]')?.content;
    if (meta("color-scheme") !== "dark") log("snap:fail no color-scheme meta of dark: " + meta("color-scheme"));
    const chrome = getComputedStyle(document.documentElement).getPropertyValue("--chrome").trim();
    if (!chrome || meta("theme-color") !== chrome) log("snap:fail the theme-color meta " + meta("theme-color") + " is not --chrome, " + chrome);
    if (getComputedStyle(document.documentElement).colorScheme !== "dark") log("snap:fail the root's color-scheme is " + getComputedStyle(document.documentElement).colorScheme);
    const webkit = [...document.styleSheets].flatMap((sheet) => [...sheet.cssRules]).filter((rule) => rule.selectorText?.includes("::-webkit-scrollbar"));
    if (webkit.length) log("snap:fail ::-webkit-scrollbar rules are left: " + webkit.map((rule) => rule.selectorText).join(", "));
    document.getElementById("pre-flight-button").click();
    await wait(600);
    const byHand = [...document.querySelectorAll("#map-pre-flight button")].find((b) => b.textContent === "Pick parts by hand");
    if (!byHand) return log("snap:fail the pre-flight offers no Pick parts by hand");
    byHand.click();
    await wait(300);
    const box = document.querySelector("#map-pre-flight input[type=checkbox]");
    if (!box) return log("snap:fail the hand picker has no checkbox");
    const shot = await window.__snapShot();
    const at = box.getBoundingClientRect();
    const scale = shot.width / innerWidth;
    const i = (Math.round((at.top + at.height / 2) * scale) * shot.width + Math.round((at.left + at.width / 2) * scale)) * 4;
    const [red, green, blue] = shot.data.slice(i, i + 3);
    const channel = (c) => ((c /= 255) <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
    if (0.2126 * channel(red) + 0.7152 * channel(green) + 0.0722 * channel(blue) > 0.2) log("snap:fail the checkbox draws light: rgb(" + [red, green, blue] + ")");
    log("dark page ok");`, { map_state: mapState() }),
  snapDone: "dark page ok",
  contrast: true,
});
