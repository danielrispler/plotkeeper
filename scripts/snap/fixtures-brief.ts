// PR briefs and part catch-ups, played on the map, and the rail's brief row.

import type { BriefChapter, BriefFigure, BriefStep, BriefView, CatchUpStage, PrBrief, RatifyRow, StoredCatchUp } from "../../ui/types.ts";
import type { Fixture } from "./base.ts";
import { railFixtures } from "./rail-fixtures.ts";
import { brief, briefOf, figure, mapBrief, mapOpened, mapState, runAt, step, storySteps } from "./fixtures-shared.ts";

const escBrief = `
      const before = document.querySelector(".brief-dots i.at") && [...document.querySelectorAll(".brief-dots i")].findIndex((d) => d.classList.contains("at"));
      document.dispatchEvent(new KeyboardEvent("keydown", { code: "Escape", key: "Escape", bubbles: true }));
      await wait(200);
      if (!document.getElementById("map-brief").hidden) return log("snap:fail Esc did not close the brief");
      if (document.getElementById("map").hidden) return log("snap:fail Esc left the Map view with a brief open");
      document.querySelector('#map-tray .tray-row[data-pr="334"]').click();
      await wait(700);
      const after = [...document.querySelectorAll(".brief-dots i")].findIndex((d) => d.classList.contains("at"));
      if (after !== before) log("snap:fail the brief did not reopen at its step: " + before + " then " + after);`;
/**
 * #456: a real Next and Back leave only the new step's title in the brief's one live region, the step itself is not live, and the dots
 * say "Step N of M".
 */
export const mapBriefAnnounce = (): Fixture => ({
  ...mapBrief(briefOf(storySteps()), 0, `
      const box = document.getElementById("map-brief");
      const count = ${storySteps().length};
      const heard = (at, why) => {
        const live = [...box.querySelectorAll("[aria-live], [role=status], [role=alert]")];
        if (live.length !== 1 || live[0].getAttribute("aria-live") !== "polite") return log("snap:fail " + why + ": the brief has " + live.length + " live regions, not one polite");
        const title = box.querySelector(".brief-title").textContent;
        if (live[0].textContent !== title) log("snap:fail " + why + ": the live region holds " + JSON.stringify(live[0].textContent) + ", not the title " + JSON.stringify(title));
        if (live[0].checkVisibility() && live[0].offsetWidth > 1) log("snap:fail " + why + ": the live region is drawn");
        const dots = box.querySelector(".brief-dots");
        if (dots.getAttribute("role") !== "img" || __snapName(dots) !== "Step " + at + " of " + count) log("snap:fail " + why + ": the dots read " + JSON.stringify(__snapName(dots)));
      };
      await __snapClick(document.getElementById("brief-next"));
      await wait(500);
      heard(2, "after Next");
      await __snapClick(box.querySelector(".brief-back"));
      await wait(500);
      heard(1, "after Back");`, true),
  realPointer: true,
});
export const mapBriefStart = (): Fixture => mapBrief(briefOf(storySteps()), 0, `if (!document.querySelector("#map-brief .brief-title").textContent.includes("brings its own brief")) log("snap:fail first step");`);
export const mapBriefStory = (): Fixture => mapBrief(briefOf(storySteps(), false), 2, `${escBrief}
      if (!document.querySelector("#map-brief .brief-cites")) log("snap:fail the step shows no citations");
      document.querySelector("#map-brief .brief-code").click();
      await wait(250);
      if (!document.querySelector("#map-drawer .drawer-code")?.textContent.includes("+the brief")) log("snap:fail See the code does not open the PR's diff");`, true);
// #490: a step's own See the code asks for the files of the parts it lights ("Now" lights the Shell and Understanding), not the PR's every file.
const prFile = (path: string) => ({ path, diff: `@@ -1 +1,2 @@\n+${path}` });
export const mapBriefStepCode = (): Fixture => ({
  ...mapBrief(briefOf(storySteps(), false), 2, `
      const button = document.querySelector("#map-brief .brief-txt .brief-code");
      if (!button) return log("snap:fail the step has no See the code of its own");
      if (document.querySelector("#map-brief .brief-foot .brief-code")) log("snap:fail the foot still holds a See the code");
      button.click();
      await wait(250);
      const shown = [...document.querySelectorAll("#map-drawer .drawer-label")].map((label) => label.textContent);
      if (shown.join("|") !== "src-tauri/src/pr_briefs.rs|understanding/src/pr_brief.rs") log("snap:fail the step's drawer shows " + shown.join("|"));`, true),
  pr_diff: {
    __by: "parts",
    values: {
      "shell,understanding": { commit: "b".repeat(40), short: "bbbbbbb", label: "#334 PR briefs on the map", url: null, files: [prFile("src-tauri/src/pr_briefs.rs"), prFile("understanding/src/pr_brief.rs")] },
      undefined: { commit: "b".repeat(40), short: "bbbbbbb", label: "#334 PR briefs on the map", url: null, files: [prFile("src-tauri/src/pr_briefs.rs"), prFile("understanding/src/pr_brief.rs"), prFile("ui/map-brief.ts"), prFile("docs/studio/behaviour/pr-briefs.md")] },
    },
  },
});
// #492: the last step ends with Also touched in every group, by kind; a group's See the code asks for only its files, by path.
const alsoGroups: PrBrief["also_groups"] = [
  { kind: "no-part", files: [{ path: "loose/notes.rs", lines: 12 }], lines: 12 },
  { kind: "tests", files: [{ path: "scripts/snap/snap.test.mjs", lines: 6, ignored: true }, { path: "server/tests/wire.rs", lines: 40, part: "shell", laws: ["wire-types-agree"] }], lines: 46 },
  { kind: "config", files: [{ path: ".github/workflows/studio.yml", lines: 3, ignored: true }, { path: "Cargo.lock", lines: 18, ignored: true }], lines: 21 },
  { kind: "docs", files: [{ path: "docs/studio/behaviour/pr-briefs.md", lines: 9, ignored: true }], lines: 9 },
  { kind: "ignored", files: [{ path: "scripts/release.sh", lines: 2, ignored: true }], lines: 2 },
  { kind: "other", files: [{ path: "hook/src/main.rs", lines: 1, part: "hook" }], lines: 1 },
];
export const mapBriefAlsoGroups = (): Fixture => ({
  ...mapBrief(briefOf(storySteps(), false, { also_groups: alsoGroups }), 3, `
      const groups = [...document.querySelectorAll("#map-brief .brief-also-group")];
      const names = groups.map((group) => group.querySelector(".brief-also-name").textContent + " " + group.querySelector(".brief-also-counts").textContent);
      if (names.join("|") !== "No part 1 file · 12 lines|Tests 2 files · 46 lines|Config/build 2 files · 21 lines|Docs 1 file · 9 lines|Ignored 1 file · 2 lines|Other 1 file · 1 line") log("snap:fail the groups show " + names.join("|"));
      if (groups[1].querySelectorAll(".brief-also-files li")[1]?.querySelector(".brief-also-law")?.textContent !== "@law: wire-types-agree") log("snap:fail the Tests group's tagged file does not name its law");
      if (groups[1].querySelectorAll(".brief-tag").length !== 1 || groups[4].querySelector(".brief-tag")) log("snap:fail the ignored tag shows outside tests, config/build and docs, or not in them");
      groups[1].querySelector(".brief-code").click();
      await wait(250);
      const shown = [...document.querySelectorAll("#map-drawer .drawer-label")].map((label) => label.textContent);
      if (shown.join("|") !== "scripts/snap/snap.test.mjs|server/tests/wire.rs") log("snap:fail the Tests group's drawer shows " + shown.join("|"));
      document.dispatchEvent(new KeyboardEvent("keydown", { code: "Escape", key: "Escape", bubbles: true }));
      await wait(250);
      if (document.querySelector("#map-drawer .drawer-label")?.checkVisibility()) log("snap:fail Esc did not close the drawer");`, true),
  pr_diff: {
    __by: "paths",
    values: {
      "scripts/snap/snap.test.mjs,server/tests/wire.rs": { commit: "b".repeat(40), short: "bbbbbbb", label: "#334 PR briefs on the map", url: null, files: [prFile("scripts/snap/snap.test.mjs"), prFile("server/tests/wire.rs")] },
    },
  },
});
// #493: a Could break step's copy icon, beside its See the code, puts a prompt for the agent on the clipboard with the changed files of
// the parts it lights, and the copy toast shows its first 3 lines; with no clipboard the toast holds the whole prompt to copy by hand.
const copySteps = (): BriefStep[] => [
  storySteps()[0],
  step("Could break", "A push could mark a read brief stale", "A push that lands while you read <b>marks the brief stale</b>, so the next open writes it again.", ["shell", "understanding"], {
    risks: [{ part: "shell", trigger: "A push lands while the brief is open.", check: "Push while reading and open the brief again." }],
    cites: [{ from: "DIFF", quote: "A new head marks it stale" }],
  }),
];
const mapBriefCopy = (clipboard: boolean): Fixture => ({
  ...mapBrief(briefOf(copySteps(), false), 1, `
      Object.defineProperty(navigator, "clipboard", { configurable: true, value: ${clipboard ? "{ writeText: async (words) => void (window.__copied = words) }" : "undefined"} });
      const copy = document.querySelector("#map-brief .brief-step-code .brief-copy");
      if (!copy) return log("snap:fail the Could break step has no copy icon beside its See the code");
      if (!copy.getAttribute("aria-label")) log("snap:fail the copy icon has no name");
      copy.click();
      await wait(150);
      const toast = document.getElementById("copy-toast");
      if (!toast.matches(":popover-open") || !document.getElementById("toasts").contains(toast)) return log("snap:fail the copy toast is not an open popover in the region");
      ${clipboard
        ? `if (!toast.textContent.includes("Prompt copied")) log("snap:fail the toast does not say the prompt was copied");
      if (!(window.__copied ?? "").includes("Changed files of the parts it lights:\\n- src-tauri/src/pr_briefs.rs\\n- understanding/src/pr_brief.rs")) log("snap:fail the prompt lacks the step's files: " + window.__copied);
      if (toast.querySelector(".copy-lines").textContent.split("\\n").length !== 3) log("snap:fail the toast does not show the prompt's first 3 lines");`
        : `const area = toast.querySelector("textarea");
      if (!area || !area.value.startsWith("In PR #334, the brief warns this could break.")) log("snap:fail the toast does not hold the prompt to copy by hand");
      if (!toast.textContent.includes("Copy this prompt")) log("snap:fail the toast does not say to copy it by hand");`}`, true),
  pr_diff: {
    __by: "parts",
    values: {
      "shell,understanding": { commit: "b".repeat(40), short: "bbbbbbb", label: "#334 PR briefs on the map", url: null, files: [prFile("src-tauri/src/pr_briefs.rs"), prFile("understanding/src/pr_brief.rs")] },
    },
  },
});
// #497: a "Since you read" story, which the tray opens first: its head line with the full brief one click away, its own Partial facts, the
// Could break copy and the not-confirmed popover. Full brief opens the whole PR's brief at the same head.
const sinceView = (): BriefView => {
  const story = briefOf([{ ...copySteps()[1], flags: whyFlags }, storySteps()[1]]);
  story.brief.brief.title = "The push since you read";
  story.brief.partial_facts = { claim_error: "the claim check timed out after 120 s", no_extractor: [] };
  return { ...story, since: "a".repeat(40) };
};
export const mapBriefSince = (): Fixture => ({
  ...mapBrief(sinceView(), 0, `
      const line = panel.querySelector(".brief-since");
      if (!line) return log("snap:fail the story has no Since you read line");
      if (!line.textContent.startsWith("Since you read aaaaaaa: only what this push changed.")) log("snap:fail the since line reads " + JSON.stringify(line.textContent));
      if (line.compareDocumentPosition(panel.querySelector(".brief-facts")) !== Node.DOCUMENT_POSITION_FOLLOWING) log("snap:fail the since line is not first under the head");
      if (panel.querySelector(".brief-facts summary")?.textContent !== "Partial facts · 1") log("snap:fail the story's own Partial facts are not shown");
      if (!panel.querySelector(".brief-step-code .brief-copy")) log("snap:fail the story's Could break step has no copy icon");
      ${openWhy("#map-brief .brief-step")}
      why.hidePopover();
      const rows = [...document.querySelectorAll('#map-tray .tray-row[data-pr="334"]')].map((row) => row.querySelector(".tray-title").textContent);
      if (rows.join("|") !== "The push since you read|PR briefs on the map") log("snap:fail the tray does not offer the story first: " + rows.join("|"));
      const full = line.querySelector(".brief-since-full");
      if (!full || full.getAttribute("aria-label") !== "Full brief of PR #334") return log("snap:fail no named Full brief button");
      full.click();
      await wait(400);
      if (panel.querySelector(".brief-since")) return log("snap:fail Full brief still plays the story");
      if (!panel.querySelector(".brief-step")?.textContent.includes("A ready PR now brings its own brief to the map")) log("snap:fail Full brief did not open the full brief");
      panel.querySelector(".brief-leave").click();
      await wait(100);
      document.querySelector('#map-tray .tray-row[data-pr="334"]').click();
      await wait(400);
      if (!panel.querySelector(".brief-since")) log("snap:fail the tray's first row no longer opens the story");`, true),
  map_state: mapState({ fog: { shell: "thick", understanding: "thin" }, ready: [{ number: 334, title: "PR briefs on the map", head: "b".repeat(40), brief: { stale: false, writing: false, chapters: ["PR briefs on the map"], quiet: false, since: "The push since you read" } }] }),
  pr_brief: { __by: "full", values: { false: sinceView(), true: briefOf(storySteps()) } },
  pr_diff: {
    __by: "parts",
    values: {
      "shell,understanding": { commit: "b".repeat(40), short: "bbbbbbb", label: "#334 PR briefs on the map", url: null, files: [prFile("src-tauri/src/pr_briefs.rs"), prFile("understanding/src/pr_brief.rs")] },
    },
  },
});
export const mapBriefCopied = (): Fixture => mapBriefCopy(true);
export const mapBriefCopyByHand = (): Fixture => mapBriefCopy(false);
// Each figure kind filled to the limits `figureShapes` draws (#449): six states, five steps, four lines a side, each label long.
const fullFigure = (kind: string): BriefFigure => {
  const label = (i: number) => `a long label ${i + 1} here`;
  return kind === "states"
    ? { kind, frame: "a terminal with a long name", items: [0, 1, 2, 3, 4, 5].map((i) => ({ label: label(i), tone: i === 1 ? "lost" : i === 2 ? "live" : null, bad: i === 1 ? true : null })), before: [], after: [] }
    : kind === "sequence"
      ? { kind, frame: null, items: [0, 1, 2, 3, 4].map((i) => ({ label: label(i), tone: i === 2 ? "live" : null, bad: null })), before: [], after: [] }
      : { kind, frame: null, items: [], before: [0, 1, 2, 3].map(label), after: [4, 5, 6, 7].map(label) };
};
// Each figure kind's accessible name, from `fullFigure`'s items (#456).
const figureNames: Record<string, string> = {
  states: "States of a terminal with a long name: a long label 1 here, a long label 2 here, a long label 3 here, a long label 4 here, a long label 5 here, a long label 6 here",
  sequence: "Sequence: a long label 1 here, then a long label 2 here, then a long label 3 here, then a long label 4 here, then a long label 5 here",
  compare: "Before: a long label 1 here, a long label 2 here, a long label 3 here, a long label 4 here. After: a long label 5 here, a long label 6 here, a long label 7 here, a long label 8 here",
};
// The card's geometry, read in the page: the card, its scrolling step, the step's words and figure, and Back and Next.
const cardBox = `
      const box = (node) => node.getBoundingClientRect();
      const card = () => document.getElementById("map-brief");`;
/** The figure sits under the step's words, inside the card, never in a column beside them (#449). */
export const mapBriefFigure = (kind: string): Fixture =>
  mapBrief(briefOf([step("Now", "What happens inside one part", "The change is inside Understanding, so a small figure shows it.", ["understanding"], { fig: fullFigure(kind) })]), 0, `${cardBox}
      if (document.querySelectorAll("#map-brief .brief-fig .fig-chip").length < 3) log("snap:fail the figure draws no chips");
      const words = box(document.querySelector("#map-brief .brief-txt"));
      const figure = box(document.querySelector("#map-brief .brief-figure"));
      const cols = document.querySelector("#map-brief .brief-cols");
      const inside = box(cols);
      if (figure.top < words.bottom - 1) log("snap:fail the figure is not under the step's words: " + figure.top + " above " + words.bottom);
      if (figure.left < inside.left - 1 || figure.right > inside.right + 1) log("snap:fail the figure overflows the card: " + figure.left + "-" + figure.right + " in " + inside.left + "-" + inside.right);
      if (cols.scrollWidth > cols.clientWidth) log("snap:fail the step scrolls sideways: " + cols.scrollWidth + " in " + cols.clientWidth);
      if (figure.width < 200) log("snap:fail the figure is cramped: " + figure.width);
      // #456: the figure is named by its kind and items.
      const named = __snapName(document.querySelector("#map-brief .brief-fig"));
      if (named !== ${JSON.stringify(figureNames[kind])}) log("snap:fail the figure is named " + JSON.stringify(named));`);
/** Each step is as tall as its own words, the card keeps one narrow width, and Back and Next keep their place (#449). */
export const mapBriefHeights = (): Fixture =>
  mapBrief(briefOf(storySteps()), 0, `${cardBox}
      const seen = [];
      for (let i = 0; i < 4; i++) {
        if (i) { document.getElementById("brief-next").click(); await wait(50); }
        seen.push({ card: box(card()), back: box(document.querySelector("#map-brief .brief-back")), next: box(document.getElementById("brief-next")) });
      }
      const [where, before] = seen;
      if (!(before.card.height < where.card.height - 20)) log("snap:fail the short step's card is not shorter: " + before.card.height + " against " + where.card.height);
      for (const at of seen) {
        if (Math.abs(at.card.width - where.card.width) > 0.5) log("snap:fail the card's width changes: " + at.card.width + " against " + where.card.width);
        for (const key of ["back", "next"]) if (Math.abs(at[key].top - where[key].top) > 0.5 || Math.abs(at[key].left - where[key].left) > 0.5) log("snap:fail " + key + " moved: " + JSON.stringify(at[key]) + " against " + JSON.stringify(where[key]));
      }
      if (!(where.card.width < 820)) log("snap:fail the card is not narrow: " + where.card.width);
      if (card().style.minHeight) log("snap:fail the card's height is set in script: " + card().style.minHeight);`, true);
// A step taller than the card's maximum scrolls inside the card, with Back and Next outside the scroll and in view (#449).
const tallStep = (): BriefStep =>
  step("Now", "A step with a great deal to say", "Every line below is one more thing the step says, so the step is taller than the card may be.", ["understanding"], {
    list: Array.from({ length: 30 }, (_, i) => `Line ${i + 1} of what changed, long enough to read as a sentence.`),
  });
export const mapBriefTall = (): Fixture =>
  mapBrief(briefOf([tallStep(), storySteps()[1]]), 0, `${cardBox}
      const cols = document.querySelector("#map-brief .brief-cols");
      const map = box(document.querySelector(".map-stage"));
      const next = document.getElementById("brief-next");
      const back = document.querySelector("#map-brief .brief-back");
      if (!(cols.scrollHeight > cols.clientHeight + 20)) log("snap:fail the tall step does not scroll inside the card: " + cols.scrollHeight + " in " + cols.clientHeight);
      if (getComputedStyle(cols).overscrollBehaviorY !== "contain") log("snap:fail the step's scroll chains to the map: " + getComputedStyle(cols).overscrollBehaviorY);
      if (box(card()).top < map.top) log("snap:fail the card runs above the map: " + box(card()).top + " above " + map.top);
      if (cols.contains(next) || cols.contains(back)) log("snap:fail Back and Next scroll with the step");
      for (const button of [back, next]) {
        const at = box(button);
        if (at.bottom > innerHeight || at.top < map.top || document.elementFromPoint(at.left + at.width / 2, at.top + at.height / 2) !== button) log("snap:fail " + button.textContent + " is not in view");
      }
      const place = box(next);
      cols.scrollTop = cols.scrollHeight;
      await wait(100);
      if (box(next).top !== place.top) log("snap:fail Next moved with the scroll");`);
// With motion on, a step's words come in after a short fade: the camera frames once they are in, so a taller step's card never covers the
// parts it lights (#443 review). The brief opens on a short step and Next goes to a tall one.
const motionOn = `
      window.matchMedia = (query) => (query.includes("prefers-reduced-motion") ? { ...media(query), matches: false } : media(query));`;
export const mapBriefTaller = (): Fixture =>
  mapBrief(briefOf([storySteps()[1], { ...tallStep(), on: ["core", "daemon"] }]), 0, `${cardBox}${motionOn}
      document.getElementById("brief-next").click();
      await wait(400);
      const top = box(card()).top;
      const lit = [...document.querySelectorAll(".plate.lit")];
      if (!lit.length) log("snap:fail the tall step lights no part");
      for (const plate of lit) if (box(plate).bottom > top + 0.5) log("snap:fail the card covers " + plate.dataset.id + ": its foot at " + box(plate).bottom + ", the card's top at " + top);`, true);
/** Next from a tall step scrolled to its end opens the next tall step at its top (#443 review). */
export const mapBriefTallScroll = (): Fixture =>
  mapBrief(briefOf([tallStep(), tallStep()]), 0, `
      const cols = document.querySelector("#map-brief .brief-cols");
      cols.scrollTop = cols.scrollHeight;
      await wait(50);
      if (!(cols.scrollTop > 0)) return log("snap:fail the first tall step did not scroll: " + cols.scrollTop);
      document.getElementById("brief-next").click();
      await wait(200);
      if (document.querySelectorAll(".brief-dots i")[1]?.classList.contains("at") !== true) return log("snap:fail Next did not go to the second step");
      if (cols.scrollTop !== 0) log("snap:fail the next step opens scrolled: " + cols.scrollTop);`, true);
export const mapBriefUnconfirmed = (): Fixture =>
  mapBrief(briefOf([step("Now", "Cards are written once per turn", "A turn is written once, whatever the order of events.", ["understanding"], { flags: ["the claim is not in the facts"] })], true), 0, `
      if (!document.querySelector("#map-brief .brief-unconfirmed")?.textContent.includes("not confirmed from the facts")) log("snap:fail no unconfirmed mark");
      // A brief from before its partial facts were stored knows only that its diff was cut (#491).
      const chip = document.querySelector("#map-brief .brief-facts");
      if (chip?.querySelector("summary").textContent !== "Partial facts · 1") return log("snap:fail an older partial brief has no chip of one: " + chip?.textContent);
      if ([...chip.querySelectorAll(".brief-facts-list > li")].map((li) => li.textContent).join("|") !== "The diff was cut") log("snap:fail the older brief's line: " + chip.textContent);`);
// The Partial facts chip (#491): a brief written from a cut diff, without its claim check, over a language no extractor reads. Closed, it says
// how many inputs it lacks; `open` opens it, and a step on it stays open, since it belongs to the panel, not the step.
const partialView = (): BriefView => {
  const view = briefOf(storySteps(), true);
  view.brief.partial_facts = {
    cut: { files_cut: ["ui/map-tray.ts", "ui/map-brief.ts"], files_partial: ["server/src/pr_briefs.rs"], bytes_sent: 65_536, bytes_cut: 20_480 },
    claim_error: "the claim check timed out after 120 s",
    no_extractor: [".py"],
  };
  return view;
};
export const mapBriefPartialFacts = (open: boolean): Fixture =>
  mapBrief(partialView(), 0, `
      const chip = document.querySelector("#map-brief .brief-facts");
      if (!chip) return log("snap:fail no Partial facts chip");
      if (chip.querySelector("summary").textContent !== "Partial facts · 3") log("snap:fail the chip's count: " + chip.querySelector("summary").textContent);
      if (document.getElementById("map-brief").textContent.includes("Written from part of the diff")) log("snap:fail the old partial line is still shown");
      if (chip.compareDocumentPosition(document.querySelector("#map-brief .brief-cols")) !== Node.DOCUMENT_POSITION_FOLLOWING) log("snap:fail the chip is not above the step");
      ${open ? `chip.querySelector("summary").click();
      await wait(50);
      const lines = [...chip.querySelectorAll(".brief-facts-list > li")];
      if (lines.length !== 3) return log("snap:fail the chip lists " + lines.length + " inputs, not 3");
      if (!lines[0].textContent.startsWith("The diff was cut: 64\\u00a0KiB sent, 20\\u00a0KiB not") || lines[0].querySelectorAll(".brief-facts-files li").length !== 3) log("snap:fail the cut line: " + lines[0].textContent);
      if (!lines[0].textContent.includes("server/src/pr_briefs.rs (in part)")) log("snap:fail the file sent in part is not marked");
      if (lines[1].textContent !== "The claim check failed: the claim check timed out after 120 s") log("snap:fail the claim line: " + lines[1].textContent);
      if (!lines[2].textContent.startsWith("No extractor reads .py files")) log("snap:fail the language line: " + lines[2].textContent);
      document.getElementById("brief-next").click();
      await wait(300);
      if (document.querySelector("#map-brief .brief-facts") !== chip || !chip.open) log("snap:fail a step on reset the chip");
      document.querySelector("#map-brief .brief-back").click();
      await wait(300);` : `if (chip.open) log("snap:fail the chip opens closed");`}`, true);
export const mapBriefNoFacts = (): Fixture =>
  mapBrief(briefOf(storySteps()), 0, `if (document.querySelector("#map-brief .brief-facts")) log("snap:fail a chip shows with nothing missing");`, true);
/**
 * The stale-base line (#494): main moved since the PR's merge base on what the brief relies on. It sits under the panel's head, above
 * the step, and names each kind once; past three names the rest are counted. With no `stale_base` there is no line.
 */
export const mapBriefStaleBase = (): Fixture =>
  mapBrief(
    {
      ...briefOf(storySteps()),
      stale_base: { laws: ["wire-types-agree"], links: ["server-window"], flows: [], paths: [], files: ["Cargo.toml", "server/src/map.rs", "server/src/pr_briefs.rs", "ui/types.ts"] },
    },
    0,
    `
      const line = document.querySelector("#map-brief .brief-stale");
      if (!line) return log("snap:fail no stale-base line");
      const want = "Main moved since this PR's base: law wire-types-agree · link server-window · files Cargo.toml, server/src/map.rs, server/src/pr_briefs.rs +1 more";
      if (line.textContent !== want) log("snap:fail the stale-base line reads " + JSON.stringify(line.textContent));
      if (line.compareDocumentPosition(document.querySelector("#map-brief .brief-cols")) !== Node.DOCUMENT_POSITION_FOLLOWING) log("snap:fail the line is not above the step");
      if ([...line.querySelectorAll(".brief-stale-names")].some((names) => names.getAttribute("translate") !== "no")) log("snap:fail a name would be translated");`,
    true,
  );
export const mapBriefNoStaleBase = (): Fixture =>
  mapBrief(briefOf(storySteps()), 0, `if (document.querySelector("#map-brief .brief-stale")) log("snap:fail a stale-base line shows with nothing moved");`, true);
// The not-confirmed popover (#491): the mark on a step, or on a flagged decision in the ratify step, opens a popover under it listing its
// reasons without the mark's own words. \`esc\` checks that Esc closes only the popover, and that a step on closes it.
const whyFlags = ["claim check: no fact says a turn is written once", "claim check: the order of events is not in the diff", "not confirmed from the facts"];
const whyStep = () => step("Now", "Cards are written once per turn", "A turn is written once, whatever the order of events.", ["understanding"], { flags: whyFlags });
const openWhy = (scope: string) => `
      const mark = document.querySelector("${scope} .brief-why-mark");
      if (!mark) return log("snap:fail no not-confirmed mark in ${scope}");
      mark.click();
      await wait(100);
      const why = document.getElementById(mark.getAttribute("aria-controls"));
      if (!why?.matches(":popover-open") || mark.getAttribute("aria-expanded") !== "true") return log("snap:fail the mark did not open its popover");
      const reasons = [...why.querySelectorAll(".brief-why-list li")].map((li) => li.textContent);
      if (reasons.join("|") !== ${JSON.stringify(whyFlags.slice(0, 2).join("|"))}) log("snap:fail the popover's reasons: " + reasons.join("|"));
      const [m, w] = [mark.getBoundingClientRect(), why.getBoundingClientRect()];
      if (!(Math.abs(w.top - m.bottom) < 16 || Math.abs(w.bottom - m.top) < 16) || w.right < m.left || w.left > m.right) log("snap:fail the popover is not anchored to its mark: " + JSON.stringify([m, w]));`;
export const mapBriefWhy = (esc: boolean): Fixture =>
  mapBrief(briefOf([whyStep(), storySteps()[1]]), 0, `${openWhy("#map-brief .brief-step")}
      ${esc ? `document.dispatchEvent(new KeyboardEvent("keydown", { code: "Escape", key: "Escape", bubbles: true }));
      await wait(100);
      if (why.matches(":popover-open")) return log("snap:fail Esc did not close the popover");
      if (document.getElementById("map-brief").hidden) return log("snap:fail Esc closed the brief with the popover");
      document.querySelector("#map-brief .brief-why-mark").click();
      await wait(100);
      if (!why.matches(":popover-open")) return log("snap:fail the mark did not open its popover again");
      document.getElementById("brief-next").click();
      await wait(300);
      if (document.querySelector(".brief-why:popover-open")) log("snap:fail a step on left the popover open");` : `await wait(800);
      if (!why.matches(":popover-open")) log("snap:fail the popover closed by itself");`}`, true);
export const mapBriefWhyDecision = (): Fixture => ({
  ...mapBrief(briefOf(storySteps(), false, { decisions: [{ id: "d1", text: "A brief is stored per project, since PR numbers repeat across repositories.", flags: whyFlags }] }), 4, `${openWhy("#map-brief .ratify-row[data-id=d1]")}
      if (document.querySelectorAll("#map-brief .ratify-row .brief-why-mark").length !== 1) log("snap:fail only the flagged decision carries the mark");`, false, 100),
  pr_ratify: ratifyRows,
});
// The prediction check (#340). A pick on a case inside one part: pick, take back, Check, then the right option and the way on.
const pickCheck: NonNullable<BriefStep["check"]> = {
  kind: "pick", q: "A worker's card comes from a turn that ended in Needs you. Is its Quiz stored?", opts: ["Yes: workers skip the rule", "No: the card is stored without its Quiz"], key: 1,
  yes: "Right: worker and check cards follow the same rule as turns.", no: "Not any more: review made worker and check cards lose it too.", flow: null, test: "a_workers_card_is_stored_without_a_quiz",
};
const pickStep = () => step("Your turn", "A worker's card comes from a turn that asked you something", "", ["understanding"], { check: pickCheck, cites: [{ from: "the PR body", quote: "worker cards lose their Quiz" }] });
export const mapBriefPick = (option: number): Fixture =>
  mapBrief(briefOf([pickStep()]), 0, `
      const next = document.getElementById("brief-next");
      const opts = [...document.querySelectorAll("#map-brief .brief-opt")];
      if (opts.length !== 2 || !next.disabled || next.textContent !== "Check") return log("snap:fail a pick check starts with Check off");
      opts[${option}].click();
      if (next.disabled) log("snap:fail a pick did not turn Check on");
      opts[${option}].click();
      if (!next.disabled || opts[${option}].getAttribute("aria-pressed") !== "false") log("snap:fail a pick was not taken back");
      opts[${option}].click();
      next.click();
      await wait(100);
      const body = document.querySelector("#map-brief .brief-check .brief-body").textContent;
      const done = [...document.querySelectorAll("#map-brief .brief-opt")];
      if (!done[1].classList.contains("right")) log("snap:fail the right option is not shown");
      if (${option} === 0 && !done[0].classList.contains("miss")) log("snap:fail the wrong pick is not marked");
      if (done[0].classList.contains("right")) log("snap:fail the wrong option is shown as right");
      if (body !== ${JSON.stringify(option === 1 ? pickCheck.yes : pickCheck.no)}) log("snap:fail the answer's words: " + body);
      if (next.textContent !== "Done" || next.disabled) log("snap:fail the way on after Check: " + next.textContent);`, true);

// An order check on a flow change, answered by tapping parts on the map: the writer-ticket's synthetic flow change as the UI plays it, an
// overview of two steps that runs the flow's hops, closed by the order check, then the chapter. The key is Core, Shell, Understanding.
const orderCheck: NonNullable<BriefStep["check"]> = {
  kind: "order", q: "A turn just ended, and Core has noticed. Tap, in order, the three parts the work passes through before anything is stored.", opts: [], key: ["core", "shell", "understanding"],
  yes: "Right: Core, then the Shell, then Understanding.", no: "Not quite. It goes Core, then the Shell, then Understanding. Follow the light.", flow: "turn-flow", test: null,
};
const flowHops: [string, boolean][] = [["shell-core", true], ["shell-understanding", false]];
const flowSteps = (): BriefStep[] => [
  step("Where", "A turn's end now runs through three parts", "The Core notices the end of a turn, the Shell hands it on, and Understanding stores it.", ["core", "shell", "understanding"], { hops: flowHops }),
  step("Your turn", "A turn just ended, and Core has noticed. Tap, in order, the three parts the work passes through before anything is stored.", "", ["core", "shell", "understanding", "window"], { check: orderCheck }),
];
const flowBrief = (): BriefView =>
  briefOf(storySteps().slice(0, 2), false, { overview: flowSteps(), chapters: [{ behaviour: "story", parts: ["shell", "understanding"], title: "Turn writing", steps: storySteps().slice(0, 2), flags: [] }] });
const tap = (ids: string[]) => ids.map((id) => `document.querySelector('.plate[data-id="${id}"]').click(); await wait(50);`).join("\n      ");
// The same taps from the keyboard (#460): real Enter on each part's open button.
const keyTap = (ids: string[]) =>
  ids.map((id) => `if (!document.querySelector('.plate[data-id="${id}"] .plate-open')) return log("snap:fail ${id} has no open button");
      document.querySelector('.plate[data-id="${id}"] .plate-open').focus(); await __snapKey("Enter"); await wait(50);`).join("\n      ");
const mapBriefOrder = (ids: string[], grade: boolean, check: string, keys = false): Fixture => ({
  ...mapBrief(flowBrief(), 1, `
      const next = document.getElementById("brief-next");
      if (next.textContent !== "Check" || !next.disabled) return log("snap:fail an order check starts with Check off");
      if (document.querySelectorAll(".plate.pick").length !== 4) log("snap:fail the parts the step lights cannot be tapped: " + document.querySelectorAll(".plate.pick").length);
      if (document.querySelectorAll("#map-brief .brief-slot").length !== 3) log("snap:fail the check has no three slots");
      ${(keys ? keyTap : tap)(ids)}
      if (document.querySelector(".plate.open")) log("snap:fail a tap opened the part's sheet");
      if (document.querySelectorAll(".plate .ord").length !== ${ids.length}) log("snap:fail the taps carry no numbers on the map");
      if (document.querySelectorAll("#map-brief .brief-slot.full").length !== ${ids.length}) log("snap:fail the slots do not hold the taps");
      ${grade ? `if (next.disabled) return log("snap:fail Check is off with every slot full");
      next.click();
      await wait(250);` : `if (!next.disabled) log("snap:fail Check is on before every slot is full");`}
      ${check}`, true),
  ...(keys && { realKeys: true }),
});
const orderTakeBack = `
      document.querySelector('.plate[data-id="shell"]').click();
      await wait(50);
      if (document.querySelectorAll(".plate .ord").length !== 1 || document.querySelector(".plate[data-id='core'] .ord")?.textContent !== "1") log("snap:fail a tap on a part did not take it back");
      document.querySelector('.plate[data-id="shell"]').click();
      await wait(50);`;
/** The parts are tapped with real Enter on their open buttons (#460). */
export const mapBriefOrderSlots = (): Fixture => mapBriefOrder(["core", "shell"], false, orderTakeBack, true);
export const mapBriefOrderWrong = (): Fixture =>
  mapBriefOrder(["understanding", "shell", "core"], true, `
      if (document.querySelector("#map-brief .brief-check .brief-body").textContent !== ${JSON.stringify(orderCheck.no)}) log("snap:fail the correction's words");
      const slots = [...document.querySelectorAll("#map-brief .brief-slot span:last-child")].map((n) => n.textContent);
      if (slots.join() !== "Core,Shell,Understanding") log("snap:fail the slots do not show the right order: " + slots);
      if (document.querySelectorAll(".map-light .light-trail").length !== 2) log("snap:fail the right path was not replayed: " + document.querySelectorAll(".map-light .light-trail").length);
      if (next.textContent !== "Next") log("snap:fail the way on after Check: " + next.textContent);`);
/** Right, and on through the chapter: the brief plays end to end, its parts tapped with real Enter (#460). */
export const mapBriefOrderRight = (): Fixture =>
  mapBriefOrder(["core", "shell", "understanding"], true, `
      if (document.querySelector("#map-brief .brief-check .brief-body").textContent !== ${JSON.stringify(orderCheck.yes)}) log("snap:fail the right answer's words");
      next.click();
      await wait(300);
      if (!document.querySelector("#map-brief .brief-title").textContent.includes("A ready PR now brings")) log("snap:fail the chapter did not follow the overview");`, true);
// The brief's ratify step (#343): a changed law and a link, and the agent's two decisions, one already accepted.
const ratifyRows: RatifyRow[] = [
  { pr: 334, kind: "law", id: "window-quiet", text: "Only needs-you has colour.", change: "changed", status: "not-yet", detail: null },
  { pr: 334, kind: "link", id: "shell-understanding", text: "Shell to Understanding", change: "added", status: "not-yet", detail: null },
  { pr: 334, kind: "decision", id: "d1", text: "A brief is stored per project, since PR numbers repeat across repositories.", change: "", status: "not-yet", detail: null },
  { pr: 334, kind: "decision", id: "d2", text: "A merged PR's brief is read as stored, never rewritten.", change: "", status: "accepted", detail: null },
];
export const mapBriefRatify = (): Fixture => ({
  ...mapBrief(briefOf(storySteps()), 4, `
      const rows = [...document.querySelectorAll("#map-brief .ratify-row")];
      if (rows.length !== 4) return log("snap:fail expected 4 ratify rows, saw " + rows.length);
      if (document.querySelectorAll("#map-brief .ratify-accept").length !== 3) log("snap:fail Accept belongs on the three undecided rows");
      if (!rows[3].textContent.includes("Accepted")) log("snap:fail the decided row does not say so");
      document.querySelector("#map-brief .ratify-row .ratify-accept").click();
      await wait(300);
      if (!document.querySelector("#map-brief .ratify-row").textContent.includes("Accepted")) log("snap:fail Accept did not mark the row");
      log("ratify ok");`, false, 100),
  pr_ratify: ratifyRows,
});
// The head's map check (#641): the PR's head fails `system-map check`, so Ratify's head says so above the items and opens to the problems.
// `alone`: nothing to ratify, so the line alone brings the step, titled "The map at this head".
const mapProblems = [
  "ui/map-diff.ts is tracked but no part owns it; add it to a part's paths or to ignored",
  "import link \"shell-understanding\" runs from \"shell\" to \"understanding\", but no import runs that way",
];
export const mapBriefMapCheck = (alone: boolean): Fixture => {
  const view = briefOf(storySteps());
  return {
    ...mapBrief({ ...view, brief: { ...view.brief, map_problems: mapProblems } }, 4, `
      const head = document.querySelector("#map-brief .ratify-head");
      if (!head) return log("snap:fail Ratify has no head");
      if (head.nextElementSibling !== document.querySelector("#map-brief .brief-ratify")) log("snap:fail the head is not above the items");
      const check = head.querySelector("details.ratify-check");
      if (check?.querySelector("summary")?.textContent !== "The map check fails at this head: 2 problems") return log("snap:fail the line says " + check?.textContent);
      if (check.open) log("snap:fail the list opens closed");
      check.querySelector("summary").click();
      await wait(100);
      const problems = [...check.querySelectorAll("li")].map((li) => li.textContent);
      if (problems.join("|") !== ${JSON.stringify(mapProblems.join("|"))}) log("snap:fail the problems: " + problems.join("|"));
      const title = document.querySelector("#map-brief .brief-title")?.textContent;
      if (title !== (${alone} ? "The map at this head" : "Accept or overturn")) log("snap:fail the step's title: " + title);
      log("map check ok");`, false, 100),
    pr_ratify: alone ? [] : ratifyRows,
  };
};
/**
 * The brief's reader explains itself (#524), with Explanations on (the default): the ? beside Leave, pressed; the step's title described by
 * its kind's line, directly under it. On the Ratify step (`ratify`), the title's line, and "Not yet ratified"'s under the first undecided
 * item only. `off` then presses the ?: the ? stays, keeps the focus and is no longer pressed, and no line is drawn or pointed at.
 */
export const mapBriefExplained = (at: "where" | "ratify" | "off"): Fixture => ({
  ...mapBrief(briefOf(storySteps()), at === "ratify" ? 4 : 0, `
      const box = document.getElementById("map-brief");
      const toggle = box.querySelector(".brief-header .explain-toggle");
      if (!toggle || toggle.nextElementSibling !== box.querySelector(".brief-header .brief-leave")) return log("snap:fail the ? is not beside Leave");
      if (toggle.getAttribute("aria-pressed") !== "true" || document.getElementById(toggle.getAttribute("interestfor"))?.textContent !== "Hide explanations") log("snap:fail the ?: " + toggle.getAttribute("aria-pressed") + " " + document.getElementById(toggle.getAttribute("interestfor"))?.textContent);
      const title = box.querySelector(".brief-title");
      const line = document.getElementById(title.getAttribute("aria-describedby"));
      if (!line?.classList.contains("explain") || title.nextElementSibling !== line) log("snap:fail the title's line");
      const want = ${JSON.stringify(at === "ratify" ? "Changes this PR made to your map; say whether each stands." : "Where this change sits on your map.")};
      if (line?.textContent !== want) log("snap:fail the title's line says " + JSON.stringify(line?.textContent));
      if (${at === "ratify"}) {
        const notes = [...box.querySelectorAll(".ratify-row .explain-link")].map((note) => note.closest(".ratify-row").dataset.id + "=" + note.textContent.split(":")[0]);
        if (notes.join("|") !== "window-quiet=Not yet ratified") log("snap:fail not yet ratified's line: " + notes.join("|"));
      }
      if (${at === "off"}) {
        toggle.focus();
        toggle.click();
        await wait(300);
        const kept = box.querySelector(".brief-header .explain-toggle");
        if (kept !== toggle || document.activeElement !== toggle) log("snap:fail the ? did not keep the focus");
        if (toggle.getAttribute("aria-pressed") !== "false" || document.getElementById(toggle.getAttribute("interestfor"))?.textContent !== "Show explanations") log("snap:fail the ? after a press: " + toggle.getAttribute("aria-pressed") + " " + document.getElementById(toggle.getAttribute("interestfor"))?.textContent);
        const lines = box.querySelectorAll(".explain").length;
        const described = [...box.querySelectorAll("[aria-describedby]")].filter((node) => document.getElementById(node.getAttribute("aria-describedby"))?.classList.contains("explain")).length;
        if (lines || described) log("snap:fail explanations drawn while off: " + lines + " lines, " + described + " described");
      }`, false, 100),
  ...(at === "ratify" ? { pr_ratify: ratifyRows } : {}),
  ...(at === "off" ? { set_explanations: '{ "explanations": false }' } : {}),
});
// Old beside new in the brief's ratify step (#485): a changed part (its purpose, a path, a held item reworded, one added, one removed), an
// added part with nothing before, and a link whose kind and reason changed.
const ratifyDetailRows: RatifyRow[] = [
  {
    pr: 334, kind: "part", id: "server", text: "Server", change: "changed", status: "not-yet",
    detail: {
      of: "part",
      name: { before: "Server", after: "Server" },
      purpose: { before: "Serves the page, keeps the state, installs the hook and runs the agents.", after: "Serves the page and keeps the state it shows." },
      caption: { before: "Serves the page", after: "Serves the page" },
      holds: [
        { name: "Fog", text: { before: "Greys a part.", after: "Greys a part a PR changed until its brief is read." }, paths_added: ["server/src/fog.rs"], paths_removed: [] },
        { name: "Settings", text: { before: "Kept per user.", after: "Kept per user." }, paths_added: [], paths_removed: [] },
        { name: "Locks", text: { before: null, after: "One writer per file at a time." }, paths_added: [], paths_removed: [] },
        { name: "Install", text: { before: "Puts the hook in place.", after: null }, paths_added: [], paths_removed: [] },
      ],
      paths_added: [],
      paths_removed: ["server/src/install.rs"],
    },
  },
  {
    pr: 334, kind: "part", id: "install", text: "Install", change: "added", status: "not-yet",
    detail: {
      of: "part",
      name: { before: null, after: "Install" },
      purpose: { before: null, after: "Puts plotkeeper's hook into each agent's settings, and takes it out again." },
      caption: { before: null, after: "Wires the hook" },
      holds: [{ name: "Hook copies", text: { before: null, after: "The hook file each agent runs." }, paths_added: ["server/src/hook_copies.rs"], paths_removed: [] }],
      paths_added: ["server/src/install.rs", "server/src/hook_copies.rs"],
      paths_removed: [],
    },
  },
  {
    pr: 334, kind: "link", id: "shell-server", text: "Shell to Server", change: "changed", status: "not-yet",
    detail: { of: "link", kind: { before: "import", after: "http" }, reason: { before: "", after: "The page calls the server over HTTP on 127.0.0.1." } },
  },
];
export const mapBriefRatifyDetail = (): Fixture => ({
  ...mapBrief(briefOf(storySteps()), 4, `
      const rows = [...document.querySelectorAll("#map-brief .ratify-row")];
      if (rows.length !== 3) return log("snap:fail expected 3 ratify rows, saw " + rows.length);
      const [changed, added, link] = rows.map((row) => row.querySelector(".ratify-detail"));
      if (!changed || !added || !link) return log("snap:fail a part or link row has no detail");
      const terms = (detail) => [...detail.querySelectorAll("dt")].map((dt) => dt.textContent);
      if (terms(changed).join("|") !== "Purpose|Paths|Holds Fog|Fog: paths|Holds Locks|Holds Install") log("snap:fail the changed part's lines: " + terms(changed).join("|"));
      if (!changed.querySelector("del")?.textContent.includes("installs the hook") || !changed.querySelector("ins")?.textContent.includes("keeps the state")) log("snap:fail the purpose is not old beside new");
      if (added.querySelector("del")) log("snap:fail an added part has something before");
      if (terms(added)[0] !== "Name" || !added.textContent.includes("Hook copies")) log("snap:fail the added part does not show its name and held items");
      if (terms(link).join("|") !== "Kind|Reason" || link.querySelector("del").textContent !== "import" || !link.textContent.includes("none")) log("snap:fail the link's kind and reason: " + link.textContent);
      log("ratify detail ok");`, false, 100),
  pr_ratify: ratifyDetailRows,
});
// Link evidence in the brief's ratify step (#499): an added link's second line, one import site then "(+N imports across M files)"; a
// removed link with none. `line` shows the second line, `list` presses the count (every site in the drawer), `site` presses a site of the
// list (its code from the PR's head, numbered, its line marked).
const evidenceRows: RatifyRow[] = [
  {
    pr: 334, kind: "link", id: "shell-understanding", text: "Shell to Understanding", change: "added", status: "not-yet",
    detail: { of: "link", kind: { before: null, after: "import" }, reason: { before: null, after: "" } },
    evidence: {
      head: "b".repeat(40),
      sites: [
        { file: "ui/map-brief.ts", line: 17 },
        { file: "ui/map-brief.ts", line: 24 },
        { file: "ui/map-ratify.ts", line: 9 },
        { file: "ui/map-tray.ts", line: 12 },
      ],
    },
  },
  { pr: 334, kind: "link", id: "hook-shell", text: "Hook to Shell", change: "removed", status: "not-yet", detail: null },
];
const evidenceSnippet = {
  file: "ui/map-ratify.ts",
  line: 9,
  first: 5,
  lines: [
    "// too, then copies a prompt that quotes the item to the clipboard for him to paste into his agent (port decision Q2); when the clipboard",
    "// refuses, the prompt shows in a small box with a Copy button.",
    "",
    "import { failure, invoke, promptBox, sayAction, text } from \"./dom.js\";",
    "import { currentMap } from \"./map.js\";",
    "import { openDrawer } from \"./map-drawer.js\";",
    "import { copyOverturn, decideWith, detailLines, evidenceLine, noteFor } from \"./ratify.js\";",
    "import { logStop } from \"./stop-rules.js\";",
    "import type { ImportSite, RatifyRow } from \"./types.js\";",
  ],
};
export const mapBriefRatifyEvidence = (at: "line" | "list" | "site"): Fixture => ({
  ...mapBrief(briefOf(storySteps()), 4, `
      const rows = [...document.querySelectorAll("#map-brief .ratify-row")];
      if (rows.length !== 2) return log("snap:fail expected 2 ratify rows, saw " + rows.length);
      const line = rows[0].querySelector(".ratify-evidence");
      if (!line) return log("snap:fail the added link has no second line");
      const site = line.querySelector(".ratify-site"), more = line.querySelector(".ratify-more");
      if (site?.textContent !== "ui/map-brief.ts:17" || more?.textContent !== "(+3 imports across 3 files)") log("snap:fail the second line says " + line.textContent);
      if (rows[1].querySelector(".ratify-evidence")) log("snap:fail a removed link shows evidence");
      if (${at !== "line"}) {
        more.click();
        await wait(300);
        const picks = [...document.querySelectorAll("#map-drawer .drawer-pick")].map((pick) => pick.textContent);
        if (picks.join("|") !== "ui/map-brief.ts:17|ui/map-brief.ts:24|ui/map-ratify.ts:9|ui/map-tray.ts:12") log("snap:fail the drawer lists " + picks.join("|"));
        if (document.querySelector("#map-drawer .drawer-title")?.textContent !== "Imports behind Shell to Understanding") log("snap:fail the list's title");
      }
      if (${at === "site"}) {
        document.querySelectorAll("#map-drawer .drawer-pick")[2].click();
        await wait(300);
        const marked = document.querySelector("#map-drawer mark.drawer-line");
        if (marked?.querySelector(".drawer-n")?.textContent !== "9") log("snap:fail the site's line is not marked: " + marked?.textContent);
        if (document.querySelector("#map-drawer .drawer-line .drawer-n")?.textContent !== "5") log("snap:fail the snippet is not numbered from its first line");
        if (document.querySelector("#map-drawer .drawer-title")?.textContent !== "ui/map-ratify.ts:9") log("snap:fail the snippet's title");
      }
      log("ratify evidence ok");`, false, 100),
  pr_ratify: evidenceRows,
  ratify_site: evidenceSnippet,
});
// Guard with a law (#401): a bug fix's brief, with nothing to ratify, ends with the guard's row: the on-trust laws of Core and Shell, the
// parts it touched, with Back this law, a new law of an earlier run being proven, and Add a new law. `confirm` presses Add a new law and
// shows the confirm step with the writer's draft.
const guardedBrief = (): BriefView => {
  const view = briefOf(storySteps());
  return { ...view, brief: { ...view.brief, guard: { parts: ["core", "shell"] } } };
};
export const mapBriefGuard = (confirm: boolean): Fixture => ({
  ...mapBrief(guardedBrief(), 4, `
      const row = document.querySelector("#map-brief .guard-row");
      if (!row) return log("snap:fail the brief has no Guard with a law row");
      if (document.querySelector("#map-brief .brief-title").textContent !== "Guard with a law") log("snap:fail the step's heading: " + document.querySelector("#map-brief .brief-title").textContent);
      const laws = [...row.querySelectorAll(".guard-law")].map((li) => li.dataset.law).join(",");
      if (laws !== "core-order,shell-notice,shell-ordered") log("snap:fail the guard's laws: " + laws);
      if (row.querySelectorAll(".guard-law button").length !== 2) log("snap:fail Back this law belongs on the two laws on trust");
      if (!row.textContent.includes("Proving the patch")) log("snap:fail the new law's run does not say where it stands");
      const add = [...row.querySelectorAll("button")].find((b) => b.textContent === "Add a new law");
      if (!add) return log("snap:fail no Add a new law");
      if (${confirm}) {
        add.click();
        await wait(300);
        const sentence = document.querySelector("#map-brief .guard-sentence");
        if (!sentence || sentence.value !== "The shell applies events in the order they were made.") return log("snap:fail the drafted sentence is not shown to confirm: " + sentence?.value);
        if (document.querySelector("#map-brief .guard-id").value !== "shell-keeps-order") log("snap:fail the drafted id");
        if (document.querySelector("#map-brief .guard-part").value !== "shell") log("snap:fail the drafted part");
        if (![...document.querySelectorAll("#map-brief .guard-confirm button")].some((b) => b.textContent === "Start the run")) log("snap:fail no Start the run");
      }
      log("guard ok");`, false, 100),
  map_state: mapState({
    fog: { shell: "thick", understanding: "thin" },
    ready: [{ number: 334, title: "PR briefs on the map", head: "b".repeat(40), brief: { stale: false, writing: false, chapters: ["PR briefs on the map"], quiet: false } }],
    law_patches: [{ id: "19a2f-11", project: "/w/plotkeeper", law: "shell-ordered", state: "proving", line: null, at: runAt, diff: null, proof: null, text: "The shell reads the log in order.", guards: 334 }],
  }),
  snapDone: "guard ok",
});
/**
 * The end-of-story bar (#393, restyled by #448) under the story's last step: on a story with no gist it holds only Skip, with no rating
 * and no note (#443). The S key skips it, closing it and sending nothing to the shell. The screenshot is the story before the skip.
 */
export const mapBriefBar = (): Fixture =>
  mapBrief(briefOf(storySteps()), 3, `
      const sent = [];
      const real = window.fetch;
      window.fetch = (url, init) => (sent.push(String(url) + " " + (init?.body ?? "")), real(url, init));
      const bar = () => document.querySelector("#map-brief .story-bar");
      if (!bar() || bar().hidden) return log("snap:fail no end-of-story bar on the story's last step");
      if (bar().querySelector(".bar-rate, .bar-note, .bar-scale")) log("snap:fail the bar still has a rating or a note");
      if (/clear|rate|note/i.test(bar().textContent)) log("snap:fail the bar still asks for a rating or a note: " + bar().textContent);
      if (bar().querySelector(".bar-explain")) log("snap:fail explain-back on a story with no gist");
      const skip = bar().querySelector(".bar-skip");
      if (skip?.textContent !== "Skip") log("snap:fail no Skip on the bar");
      const key = (target, k) => target.dispatchEvent(new KeyboardEvent("keydown", { key: k, code: /^[0-9]$/.test(k) ? "Digit" + k : "Key" + k.toUpperCase(), bubbles: true }));
      key(document, "4");
      await wait(50);
      if (bar().hidden) log("snap:fail a digit closed the bar");
      key(document, "s");
      await wait(50);
      if (!bar().hidden) log("snap:fail the S key did not skip the bar");
      const bars = sent.filter((call) => call.includes("/api/story_bar"));
      if (bars.length) log("snap:fail the bar sent " + bars.join());
      bar().hidden = false;`, false, 100);
// Explain-back (#397, restyled by #448): the same story, its chapter carrying a gist and lighting Core parts, ends with the one-line field
// in the bar's first row: autocomplete off, a placeholder that shows an example and ends with "…". Before submit, an "s" and a "2" typed
// in the line do not skip, the gist is not shown yet, and an Enter while an input method is composing does not submit. After submit
// (Enter), the line shows with the gist under it and the three marks as radios in a fieldset with a legend, each radio inside its label;
// a click on Partly's label checks it. Each is sent to the shell; no number shows. With the picked mark focused, S skips the bar.
const explainGist = "A ready PR's brief is written in the background and played on the map, and reading a chapter clears its parts' fog.";
export const mapBriefExplainBack = (submit: boolean): Fixture =>
  mapBrief(briefOf(storySteps(), false, { chapters: [{ ...brief(storySteps()).chapters[0], gist: explainGist }] }), 3, `
      const sent = [];
      const real = window.fetch;
      window.fetch = (url, init) => (sent.push(String(url) + " " + (init?.body ?? "")), real(url, init));
      const bar = () => document.querySelector("#map-brief .story-bar");
      const line = bar()?.querySelector(".bar-slot .bar-explain .bar-line");
      if (!line) return log("snap:fail no explain-back line on a Core story with a gist");
      if (bar().firstElementChild.className !== "bar-slot") log("snap:fail explain-back is not the bar's first row");
      if (bar().querySelector(".bar-rate, .bar-note, .bar-scale")) log("snap:fail the bar still has a rating or a note");
      if (bar().querySelector(".bar-skip")?.textContent !== "Skip") log("snap:fail no Skip on the bar");
      if (line.getAttribute("autocomplete") !== "off") log("snap:fail the line's autocomplete is not off");
      if (!line.placeholder.endsWith("…") || !/e\\.g\\./.test(line.placeholder)) log("snap:fail the placeholder shows no example ending in …: " + line.placeholder);
      if (bar().textContent.includes(${JSON.stringify(explainGist)})) log("snap:fail the gist shows before the line is given");
      const key = (target, k, init = {}) => target.dispatchEvent(new KeyboardEvent("keydown", { key: k, code: /^[0-9]$/.test(k) ? "Digit" + k : "Key" + k.toUpperCase(), bubbles: true, cancelable: true, ...init }));
      line.focus();
      key(line, "s");
      key(line, "2");
      await wait(50);
      if (bar().hidden) return log("snap:fail an s typed in the line skipped the bar");
      line.value = "Briefs get written in the background and reading one clears the fog.";
      key(line, "Enter", { isComposing: true });
      await wait(50);
      if (bar().querySelector(".bar-said") || sent.some((call) => call.includes("/api/story_bar"))) log("snap:fail an Enter while composing submitted the line");
      ${submit ? `key(line, "Enter");
      await wait(50);
      const said = bar().querySelector(".bar-said");
      if (said?.textContent !== line.value) log("snap:fail the line is not shown after submit: " + said?.textContent);
      if (!bar().querySelector(".bar-gist")?.textContent.includes(${JSON.stringify(explainGist)})) log("snap:fail the gist is not shown under the line");
      const group = bar().querySelector("fieldset.bar-marks");
      if (!group?.querySelector(":scope > legend")?.textContent.trim()) log("snap:fail the marks are not a fieldset with a legend");
      const radios = [...bar().querySelectorAll('.bar-marks input[type="radio"]')];
      const labels = radios.map((r) => r.closest("label"));
      if (labels.map((l) => l?.textContent.trim()).join() !== "Got it,Partly,Missed") log("snap:fail the marks: " + labels.map((l) => l?.textContent.trim()).join());
      if (new Set(radios.map((r) => r.name)).size !== 1 || !radios[0]?.name) log("snap:fail the radios are not one group");
      labels[1]?.querySelector(".bar-mark-word")?.click();
      await wait(50);
      if (radios.filter((r) => r.checked).map((r) => r.value).join() !== "partly") log("snap:fail Partly is not checked by its label");
      if (/\\d/.test(bar().querySelector(".bar-slot").textContent)) log("snap:fail explain-back shows a number");
      const bars = sent.filter((call) => call.includes("/api/story_bar")).map((call) => JSON.parse(call.slice(call.indexOf(" ") + 1)));
      const kinds = bars.map((b) => b.kind + ":" + (b.line ?? b.mark)).join();
      if (kinds !== "explain-back-given:" + line.value + ",explain-back-marked:partly") log("snap:fail the bar sent " + kinds);
      // A picked mark is no field: S still skips the bar (#443 review). The screenshot is the bar before the skip.
      radios[1].focus();
      key(radios[1], "s");
      await wait(50);
      if (!bar().hidden) log("snap:fail the S key did not skip the bar with a mark picked");
      bar().hidden = false;` : `await wait(50);`}`, false, 100);
/**
 * Overturning in the brief's ratify step (port decision Q2): the prompt goes to the clipboard and the step says so, or, when the clipboard
 * refuses, shows the prompt in a box with a Copy button. The page's clipboard is stubbed to take or refuse the write.
 */
export const mapBriefOverturn = (clipboard: "copies" | "refuses"): Fixture => ({
  ...mapBrief(briefOf(storySteps()), 4, `
      Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: () => ${clipboard === "copies" ? "Promise.resolve()" : 'Promise.reject(new Error("blocked"))'} } });
      document.querySelector("#map-brief .ratify-overturn").click();
      await wait(400);
      const note = document.querySelector("#map-brief .ratify-note");
      if (!note) return log("snap:fail Overturn left no note");
      if (!note.textContent.includes("Prompt copied, paste it into your agent") === ${clipboard === "copies"}) log("snap:fail the note's line: " + note.textContent);
      const box = note.querySelector("textarea.ratify-prompt");
      if (${clipboard === "copies"} ? box : !box || !box.value.includes("#334") || !note.querySelector(".ratify-copy")) log("snap:fail the fallback box: " + note.textContent);
      log("overturn ok");`, false, 100),
  pr_ratify: ratifyRows,
  snapDone: "overturn ok",
});
/** A brief that takes a while to write (port decision Q5): the panel and the tray row say "Writing the brief…" and the map stays usable. */
export const mapBriefWriting = (): Fixture => ({
  ...mapOpened(`
      document.querySelector('#map-tray .tray-row[data-pr="334"]').click();
      await wait(600);
      const panel = document.getElementById("map-brief");
      if (panel.hidden || !panel.textContent.includes("Writing the brief…")) return log("snap:fail the panel does not say the brief is being written");
      if (!document.querySelector('#map-tray .tray-row[data-pr="334"] .tray-note')?.textContent.includes("writing the brief")) log("snap:fail the tray row does not say so");
      if (document.getElementById("map").hidden) log("snap:fail the map is not usable");
      if (!(panel.getBoundingClientRect().width < 820)) log("snap:fail the waiting card is not narrow: " + panel.getBoundingClientRect().width);
      log("writing ok");`, {
    map_state: mapState({ fog: { shell: "thick" }, ready: [{ number: 334, title: "PR briefs on the map", head: "b".repeat(40), brief: null }] }),
    // The write runs in the background: the open answers at once that it is being written.
    pr_brief: { writing: true },
  }),
  snapDone: "writing ok",
});
// A part catch-up (#391): Understanding was fogged by two PRs and a direct push, so its sheet offers the part catch-up as the main button, with
// "I know this" quiet beside it; pressing it plays one story of the part in the brief's reader, with no "See the code" and no PR number.
const catchUpSteps = (): BriefStep[] => [
  step("Where", "Understanding writes what each turn and PR changed", "It turns a session's events into cards and a ready PR into its brief, with a sandboxed writer that has no tools.", ["understanding"], {
    cites: [{ from: "ENTRIES", quote: "writes the PR brief in the background" }],
  }),
  step("Before", "Opening a brief waited for its writer", "Opening a PR's brief that was not written yet held the panel until the writer answered, minutes at a time.", ["understanding", "window"], {
    hops: [["shell-understanding", false], ["shell-window", false]],
  }),
  step("Now", "A brief is written in the background and lands", "An open answers at once that it is being written; the brief lands on the map when the writer is done. A direct push trimmed the facts' budget.", ["understanding"], {
    fig: figure("sequence"), cites: [{ from: "DIFF", quote: "open_in_background" }],
  }),
  step("Must stay true", "The writer still reads only the facts", "The map's law says the writer is asked at most once per item; the claim check and its one retry keep to it.", ["understanding"], { law: "understanding-asked" }),
];
const storedCatchUp = (): StoredCatchUp => ({
  version: 1, part: "understanding", fog: ["c".repeat(40), "d".repeat(40), "e".repeat(40)], written_at: 1, model: "opus", partial: false,
  brief: { ...brief(catchUpSteps()), title: "Briefs are written in the background", chapters: [{ behaviour: "story", parts: ["understanding"], title: "Briefs written in the background", steps: catchUpSteps(), flags: [] }] },
});
export const mapCatchUp = (): Fixture => ({
  ...mapOpened(`
      const media = window.matchMedia.bind(window);
      window.matchMedia = (query) => (query.includes("prefers-reduced-motion") ? { ...media(query), matches: true } : media(query));
      const calm = document.createElement("style");
      calm.textContent = "*, *::before, *::after { transition: none !important; animation: none !important; }";
      document.head.append(calm);
      document.querySelector("#map .plate[data-id=understanding]").click();
      await wait(150);
      const buttons = [...document.querySelectorAll(".sheet .sheet-fog button")].map((b) => b.textContent);
      if (buttons.join("|") !== "Part catch-up|I know this") return log("snap:fail the sheet's fog buttons: " + buttons.join("|"));
      if (!document.querySelector(".sheet .sheet-catch-up").classList.contains("primary")) log("snap:fail the part catch-up is not the main button");
      document.querySelector(".sheet .sheet-catch-up").click();
      await wait(350);
      const panel = document.getElementById("map-brief");
      if (panel.hidden || !panel.classList.contains("open")) return log("snap:fail the part catch-up did not open");
      if (!panel.querySelector(".brief-kick").textContent.includes("Part catch-up")) log("snap:fail the kick does not say part catch-up");
      if (panel.querySelector(".brief-code")) log("snap:fail a part catch-up offers a PR's code");
      // Its diff was cut (#491): its only input of the Partial facts chip.
      if (panel.querySelector(".brief-facts summary")?.textContent !== "Partial facts · 1") log("snap:fail a cut part catch-up has no chip of one");
      document.getElementById("brief-next").click();
      await wait(200);
      document.getElementById("brief-next").click();
      await wait(450);
      if (!document.querySelector(".plate.lit[data-id=understanding]")) log("snap:fail the part is not lit");
      log("catch-up ok");`, {
    map_state: mapState({
      fog: { understanding: "thick", shell: "thin" },
      catch_ups: { understanding: { kind: "catch-up", ready: true, writing: false, failed: false }, shell: { kind: "chapter", pr: 334, chapter: 0 } },
    }),
    part_catch_up: { ...storedCatchUp(), partial: true },
  }),
  snapDone: "catch-up ok",
});
/** A part catch-up over a long fog list (#396): its first step says it summarises the main shifts, with the full fog list behind it. */
export const mapCatchUpSummarising = (): Fixture => ({
  ...mapOpened(`
      const calm = document.createElement("style");
      calm.textContent = "*, *::before, *::after { transition: none !important; animation: none !important; }";
      document.head.append(calm);
      document.querySelector("#map .plate[data-id=understanding]").click();
      await wait(150);
      document.querySelector(".sheet .sheet-catch-up").click();
      await wait(350);
      const panel = document.getElementById("map-brief");
      if (panel.hidden || !panel.classList.contains("open")) return log("snap:fail the part catch-up did not open");
      if (!panel.querySelector(".brief-summarises")?.textContent.includes("summarises the main shifts of 9 fog entries")) return log("snap:fail no summarising line");
      const list = panel.querySelector(".brief-fog-list");
      list.open = true;
      if (list.querySelectorAll(".brief-cite").length !== 9) log("snap:fail the full fog list does not hold 9 entries");
      await wait(150);
      log("catch-up summarising ok");`, {
    map_state: mapState({ fog: { understanding: "thick" }, catch_ups: { understanding: { kind: "catch-up", ready: true, writing: false, failed: false } } }),
    part_catch_up: {
      ...storedCatchUp(),
      fog: [..."abcdefghi"].map((c) => c.repeat(40)),
      summary: {
        line: "This catch-up summarises the main shifts of 9 fog entries; the full list is below.",
        entries: [..."abcdefghi"].map((c, i) => (i % 3 === 2 ? { commit: c.repeat(40), pr: null, title: "fix pushed straight to main" } : { commit: c.repeat(40), pr: 380 + i, title: `Briefs: change number ${i + 1}` })),
      },
    },
  }),
  snapDone: "catch-up summarising ok",
});
/** A part catch-up being written: its button stays pressable and says "Writing…", the panel says so, and the map is not dimmed. */
export const mapCatchUpWriting = (): Fixture => ({
  ...mapOpened(`
      const calm = document.createElement("style");
      calm.textContent = "*, *::before, *::after { transition: none !important; animation: none !important; }";
      document.head.append(calm);
      document.querySelector("#map .plate[data-id=understanding]").click();
      await wait(300);
      if (document.querySelector(".sheet .sheet-catch-up-note")?.textContent !== "Writing…") log("snap:fail the sheet does not say it is being written");
      if (document.querySelector(".sheet .sheet-catch-up").disabled) log("snap:fail the button is not pressable while it is written");
      document.querySelector(".sheet .sheet-catch-up").click();
      await wait(600);
      const panel = document.getElementById("map-brief");
      if (panel.hidden || !panel.textContent.includes("Writing…") || panel.querySelector("progress")?.getAttribute("aria-label") !== "Writing the part catch-up") return log("snap:fail the panel does not say the catch-up is being written");
      if (document.body.classList.contains("brief-open")) log("snap:fail the map is dimmed while it is written");
      log("catch-up writing ok");`, {
    map_state: mapState({ fog: { understanding: "thick" }, catch_ups: { understanding: { kind: "catch-up", ready: false, writing: true, failed: false } } }),
    part_catch_up: { writing: true },
  }),
  snapDone: "catch-up writing ok",
});
/**
 * While a part catch-up is written, the waiting card says which call runs (#450): the stage's label beside a named, indeterminate
 * progress bar, both in a polite live region, at the story card's narrow width.
 */
export const mapCatchUpStage = (stage: CatchUpStage, label: string): Fixture => ({
  ...mapOpened(`
      const calm = document.createElement("style");
      calm.textContent = "*, *::before, *::after { transition: none !important; }";
      document.head.append(calm);
      document.querySelector("#map .plate[data-id=understanding]").click();
      await wait(300);
      document.querySelector(".sheet .sheet-catch-up").click();
      await wait(600);
      const panel = document.getElementById("map-brief");
      if (panel.hidden) return log("snap:fail the waiting card is not shown");
      const live = panel.querySelector('[aria-live="polite"]');
      if (!live) return log("snap:fail the waiting card has no polite live region");
      if (!live.textContent.includes(${JSON.stringify(label)})) log("snap:fail the stage reads: " + live.textContent);
      const bar = live.querySelector("progress");
      if (!bar) log("snap:fail no progress element in the live region");
      else {
        if (bar.hasAttribute("value")) log("snap:fail the progress is not indeterminate");
        if (!bar.getAttribute("aria-label") && !bar.labels?.length && !bar.getAttribute("aria-labelledby")) log("snap:fail the progress has no name");
      }
      const narrow = parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--brief-width"));
      const width = panel.getBoundingClientRect().width;
      if (Math.abs(width - narrow) > 1) log("snap:fail the waiting card is not the story card's width: " + width + " against " + narrow);
      log("catch-up stage ok");`, {
    map_state: mapState({ fog: { understanding: "thick" }, catch_ups: { understanding: { kind: "catch-up", ready: false, writing: true, failed: false, stage } } }),
    part_catch_up: { writing: true },
  }),
  snapDone: "catch-up stage ok",
});
// A step's bold phrase (#446): the writer may wrap one phrase in <b>...</b>; the reader shows it as one bold element and never the tag.
const boldBody = "No process is recorded, so the session <b>goes idle</b> until its next event.";
const boldCheck = (done: string) => `
      const body = document.querySelector("#map-brief .brief-body");
      const bold = [...body.querySelectorAll("b")];
      if (body.textContent !== "No process is recorded, so the session goes idle until its next event.") log("snap:fail the step body reads: " + body.textContent);
      if (bold.length !== 1 || bold[0].textContent !== "goes idle") log("snap:fail bold elements: " + bold.map((b) => b.textContent).join("|"));
      if (body.children.length !== bold.length) log("snap:fail the step body holds another element");
      log("${done}");`;
export const mapCatchUpBold = (): Fixture => ({
  ...mapOpened(`
      const calm = document.createElement("style");
      calm.textContent = "*, *::before, *::after { transition: none !important; animation: none !important; }";
      document.head.append(calm);
      document.querySelector("#map .plate[data-id=understanding]").click();
      await wait(150);
      document.querySelector(".sheet .sheet-catch-up").click();
      await wait(350);
      if (!document.querySelector("#map-brief .brief-kick")?.textContent.includes("Part catch-up")) return log("snap:fail the part catch-up did not open");
      ${boldCheck("catch-up bold ok")}`, {
    map_state: mapState({ fog: { understanding: "thick" }, catch_ups: { understanding: { kind: "catch-up", ready: true, writing: false, failed: false } } }),
    part_catch_up: { ...storedCatchUp(), brief: { ...storedCatchUp().brief, chapters: [{ behaviour: "story", parts: ["understanding"], title: "Idle sessions", steps: [step("Now", "Each line names its agent", boldBody, ["understanding"])], flags: [] }] } },
  }),
  snapDone: "catch-up bold ok",
});
export const mapBriefBold = (): Fixture => ({
  ...mapBrief(briefOf([step("Now", "Each line names its agent", boldBody, ["shell"])]), 0, boldCheck("brief bold ok"), true),
  snapDone: "brief bold ok",
});
/** An unpaired <b> and an <img onerror> string stay text: no element is made from them and nothing runs. */
export const mapBriefBoldUnpaired = (): Fixture => {
  const body = 'A lone <b>tag and <img src="x" onerror="window.__boldRan = 1"> stay text.';
  return {
    ...mapBrief(briefOf([step("Now", "Tags the writer did not pair", body, ["shell"])]), 0, `
      const node = document.querySelector("#map-brief .brief-body");
      if (node.textContent !== ${JSON.stringify(body)}) log("snap:fail the step body reads: " + node.textContent);
      if (node.children.length) log("snap:fail the step body made elements: " + [...node.children].map((c) => c.tagName).join("|"));
      if (window.__boldRan) log("snap:fail the onerror ran");
      log("brief unpaired ok");`, true),
    snapDone: "brief unpaired ok",
  };
};
// A chaptered brief (#341): the tray lists one row per chapter, each chapter opens on its own and the overview plays before the first one opened, once.
const chapterTitles = ["Fog clears by reading", "Broken laws on the map", "The Seen gate"];
const chapterOf = (i: number, parts: string[]): BriefChapter => ({
  behaviour: chapterTitles[i], parts, title: chapterTitles[i], flags: [],
  steps: [
    step("Before", `${chapterTitles[i]}: how it was`, "Before this change the map showed the fact but said nothing of why it changed.", [parts[0]]),
    step("Now", `${chapterTitles[i]}: what happens now`, "The Shell works it out and the Window shows it, with a light along the link.", parts, { hops: [["shell-window", false]] }),
    step("Could break", `${chapterTitles[i]}: what could break`, "A part read on the way in would look clear before it was.", [parts[parts.length - 1]], { risks: [{ part: parts[0], trigger: "A push lands while the brief is open.", check: "Open the brief again." }] }),
  ],
});
const chapteredView = (): BriefView => ({
  partial: false,
  brief: {
    version: 1, pr: 334, head: "b".repeat(40), written_at: 1, stale: false, model: "opus",
    brief: {
      title: "The Map view: fog, broken laws and Seen", overview: [step("Where", "The Map view now says what changed and clears itself", "Three behaviours are new: fog that clears by reading, broken laws and the Seen gate.", ["shell", "window", "core"], { hops: [["shell-window", false]] })],
      chapters: [chapterOf(0, ["shell", "window"]), chapterOf(1, ["core", "shell", "window"]), chapterOf(2, ["window", "shell"])],
      also: [{ part: "hook", line: "A wording change in a comment.", flags: [] }], unseen: [], decisions: [], flags: [],
    },
  },
});
const chapteredState = () =>
  mapState({ fog: { shell: "thick", window: "thin", core: "thin" }, ready: [{ number: 334, title: "The Map view", head: "b".repeat(40), brief: { stale: false, writing: false, chapters: chapterTitles, quiet: false } }] });
const mapChaptered = (row: number, next: number, check: string): Fixture =>
  mapOpened(
    `
      const media = window.matchMedia.bind(window);
      window.matchMedia = (query) => (query.includes("prefers-reduced-motion") ? { ...media(query), matches: true } : media(query));
      const calm = document.createElement("style");
      calm.textContent = "*, *::before, *::after { transition: none !important; animation: none !important; }";
      document.head.append(calm);
      const rows = [...document.querySelectorAll('#map-tray .tray-row[data-pr="334"]')];
      if (rows.length !== 3) return log("snap:fail expected 3 chapter rows, saw " + rows.length);
      if (rows.map((r) => r.querySelector(".tray-title").textContent).join("|") !== ${JSON.stringify(chapterTitles.join("|"))}) return log("snap:fail the chapter rows' titles");
      ${row < 0 ? "" : `rows[${row}].click();
      await wait(300);
      if (document.getElementById("map-brief").hidden) return log("snap:fail the chapter did not open");
      for (let i = 0; i < ${next}; i++) { document.getElementById("brief-next").click(); await wait(150); }
      await wait(200);`}
      ${check}
      log("brief ok");`,
    { map_state: chapteredState(), pr_brief: chapteredView() },
  );
export const mapBriefChapters = (): Fixture => mapChaptered(-1, 0, `if (document.querySelector("#map-tray .tray-note").textContent.includes("1 of 3") === false) log("snap:fail the first row's note");`);
/** Chapter 2 opened first carries the overview: its dots are the overview's one step and its own three; Next once lands on its first step. */
export const mapBriefChapterMid = (): Fixture =>
  mapChaptered(1, 2, `
      if (document.querySelectorAll("#map-brief .brief-dots i").length !== 4) log("snap:fail the first chapter opened should play the overview and its own steps");
      if (!document.querySelector("#map-brief .brief-title").textContent.includes("Broken laws on the map: what happens now")) log("snap:fail mid-chapter step: " + document.querySelector("#map-brief .brief-title").textContent);
      if (document.querySelector("#map-brief .brief-also")) log("snap:fail also-touched shows before the last chapter's end");
      if (document.querySelectorAll(".plate.lit").length < 2) log("snap:fail the chapter's parts are not lit");
      if (document.querySelector("#map-brief .story-bar")) log("snap:fail the end-of-story bar shows mid-chapter");
      // This chapter is not the last, and its last step still ends with the bar (#393).
      document.getElementById("brief-next").click();
      await wait(200);
      if (!document.querySelector("#map-brief .story-bar")) log("snap:fail no end-of-story bar at a chapter's end");
      document.querySelector("#map-brief .brief-back").click();
      await wait(200);`);
/** The last chapter's last step ends with the also-touched lines; another chapter, opened second, has no overview. */
export const mapBriefChapterLast = (): Fixture =>
  mapChaptered(2, 3, `
      if (document.querySelectorAll("#map-brief .brief-dots i").length !== 4) log("snap:fail the last chapter opened first should carry the overview");
      if (!document.querySelector("#map-brief .brief-also")?.textContent.includes("A wording change in a comment")) log("snap:fail no also-touched line on the last step: " + document.querySelector("#map-brief .brief-title").textContent);
      if (!document.querySelector("#map-brief .story-bar")) log("snap:fail no end-of-story bar at the last chapter's end");`);
/** The rail's quiet row "PR brief ready" in the newest turn of the session whose PR a brief waits for (#344); pressing it opens that brief on the map. */
export const railBriefReady = (press: boolean): Fixture => ({
  ...(railFixtures as Record<string, () => Fixture>).railPlain(),
  briefs_waiting: { project: "/w/plotkeeper", numbers: [61] },
  map_state: mapState({ fog: { shell: "thick" }, ready: [{ number: 61, title: "Row controls", head: "b".repeat(40), brief: { stale: false, writing: false, chapters: ["PR briefs on the map"], quiet: false } }] }),
  pr_brief: briefOf(storySteps()),
  snapScript: `
    const wait = (ms) => new Promise((done) => setTimeout(done, ms));
    await wait(300);
    const rows = [...document.querySelectorAll(".now-turn .brief-ready")];
    if (rows.length !== 1) return log("snap:fail expected one PR brief row in the newest turn, saw " + rows.length);
    if (rows[0].textContent !== "PR brief ready") log("snap:fail the row says: " + rows[0].textContent);
    if (document.querySelector(".turn.past .brief-ready")) log("snap:fail an earlier turn holds the row");
    document.getElementById("story").scrollTop = 0;
    if (${press}) {
      rows[0].click();
      await wait(900);
      if (document.getElementById("map").hidden) return log("snap:fail the row did not open the Map view");
      if (!document.getElementById("map-brief").classList.contains("open")) return log("snap:fail the brief did not open");
    }
    log("rail brief row ok");`,
});
