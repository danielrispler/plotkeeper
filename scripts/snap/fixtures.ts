// Studio state for `npm run snap`: each fixture maps a server command (`POST /api/<command>`) to what it returns.
// A fixture is a function so that its times are relative to the moment it renders.
// The fixtures live by area in the `fixtures-*.ts` files beside this one; this file lists them, in the order `snap:all` renders and
// shards them.

import type { Fixture } from "./base.ts";
import { railFixtures } from "./rail-fixtures.ts";
import { list, withSaved } from "./fixtures-shared.ts";
import { alerts, cutText, cutTextStill, hints, listClearLanded, listHeld, listHints, listNarrow, listSecondLines, logDone, logFixture, logos, noticeErrors, notRecording, paletteFocus, paletteOpen, projectChevron, projectMenuScript, projectsSaved, recording, skipLink, snapRealKeys, toastKeys, whatsNewToast } from "./fixtures-page.ts";
import { mapBadgeList, mapBase, mapCheckout, mapDefaultNoBanner, mapNotGit, mapBriefPairLight, mapBriefSwitch, mapBroken, mapBusy, mapCalm, mapCamera, mapDotsOrder, mapDotsRetry, mapEmpty, mapFog, mapFogPlates, mapFresh, mapGlassMotion, mapGoneFocus, mapHeaderDot, mapMarkHints, mapMarks, mapScan, mapScanMerge, mapScanNoClaude, mapScanSwitch, mapDraftSteps, mapDrafting, mapDraftReview, mapDraftRedraft, mapDraftAccept, mapDraftReplace, mapDraftWrite, mapPartKeys, mapPartRedraw, mapProjectGone, mapProjectRemoved, mapRefresher, mapPurposeMore, mapCaughtUp, mapCatchUpOlder, mapReview, mapScrubbed, mapSmallGaps, mapSpotlight, mapStale, mapSwitchSavesViews, mapTalksTo, readmeHero } from "./fixtures-map.ts";
import { mapFootprint, mapLatePlace, mapPanelBrief, mapPanelDiff, mapNotPlacedHint, mapPanelFieldHints, mapPanelFocus, mapPanelHead, mapPanelHistory, mapPanelLine2, mapPanelNoSession, mapPanelPeel, mapListFold, mapPanelSettings, mapPanelStory, mapPanelToggle, mapSheetClose, mapSheetExplained, mapSheetHere, mapSheetHolds, mapSheetKeptWidth, mapSheetResizeRedraw, mapSheetScrollShort, mapSheetStill, mapSheetUnratified, mapSheetWidth, mapToastBeside, mapTurnLight, mapPanelPatched, mapPanelAskStays, mapPanelBriefFoot } from "./fixtures-panel.ts";
import { mapBackLaw, mapMicroWorld, mapMicroWorldBuilding, mapPreFlight, mapPreFlightOutside, mapPreFlightOutsideKept, mapPreFlightPalette, mapPreFlightPick, mapPreFlightStatus, mapRuns, mapRunsAnnounced, mapRunsBesideSheet, mapRunsPaused, mapTray, mapTrayAfterPush } from "./fixtures-runs.ts";
import { mapBriefAlsoGroups, mapBriefAnnounce, mapBriefBar, mapBriefBold, mapBriefBoldUnpaired, mapBriefChapterLast, mapBriefChapterMid, mapBriefChapters, mapBriefCopied, mapBriefCopyByHand, mapBriefExplainBack, mapBriefExplained, mapBriefFigure, mapBriefGuard, mapBriefHeights, mapBriefMapCheck, mapBriefNoFacts, mapBriefReview, mapBriefStillTrue, mapBriefNoStaleBase, mapBriefOrderRight, mapBriefOrderSlots, mapBriefOrderWrong, mapBriefOverturn, mapBriefPartialFacts, mapBriefPick, mapBriefRatify, mapBriefRatifyDetail, mapBriefRatifyEvidence, mapBriefShort, mapBriefSince, mapBriefStaleBase, mapBriefStart, mapBriefStepCode, mapBriefStory, mapBriefTall, mapBriefTaller, mapBriefTallScroll, mapBriefUnconfirmed, mapBriefWhy, mapBriefWhyDecision, mapBriefWriting, mapCatchUp, mapCatchUpBold, mapCatchUpCard, mapCatchUpStage, mapCatchUpSummarising, mapCatchUpWriting, mapDiffClosed, mapDiffLitGhost, mapDiffRatifyLit, mapDiffRemoval, mapDiffRemovalBrief, mapDiffSplit, mapDiffSplitBrief, listBriefMark, railBriefReady } from "./fixtures-brief.ts";
import { writersBriefsNeedClaude, writersFailed, writersKeyError, writersKeyNotSet, writersKeySet, writersList, writersOpenAI, writersPassed, writersSecretService } from "./fixtures-writers.ts";

export const fixtures = {
  ...railFixtures,
  // What's new after an update (#527).
  whatsNewToast,
  // The toasts from the keyboard (#457).
  toastKeys,
  // Real keys from a fixture (#454).
  snapRealKeys,
  // The palette as a popover listbox (#458).
  paletteFocus,
  // The small keyboard, pointer and layout gaps (#462).
  mapSmallGaps,
  listNarrow,
  // The page's structure: the skip link (#461).
  skipLink,
  // The hints (spec #582).
  hints,
  // The list's hints open left of the list (spec #658).
  listHints,
  // Cut text scrolls in place (spec #582).
  cutText,
  cutTextStill,
  writersList,
  writersPassed,
  writersFailed,
  writersKeyNotSet,
  writersKeySet,
  writersOpenAI,
  writersBriefsNeedClaude,
  writersKeyError,
  writersSecretService,
  writersListNarrow: (): Fixture => fixtures.writersList(),
  writersPassedNarrow: (): Fixture => fixtures.writersPassed(),
  writersFailedNarrow: (): Fixture => fixtures.writersFailed(),
  writersBriefsNeedClaudeNarrow: (): Fixture => fixtures.writersBriefsNeedClaude(),
  // The contrast pass (#455) runs here too.
  mapTray: (): Fixture => ({ ...mapTray(), contrast: true }),
  mapTrayAfterPush,
  mapRuns,
  // The contrast pass (#455) runs here too.
  mapPreFlight: (): Fixture => ({ ...mapPreFlight(), contrast: true }),
  mapPreFlightStatus,
  mapPreFlightOutside,
  mapPreFlightOutsideKept,
  mapPreFlightPalette,
  mapMicroWorld,
  mapMicroWorldBuilding,
  mapRunsPaused,
  mapRunsAnnounced,
  mapRunsBesideSheet,
  mapBackLaw,
  railBriefRow: () => railBriefReady(false),
  railBriefOpened: () => railBriefReady(true),
  // The session row's PR brief mark (#630).
  listBriefMark,
  mapBriefStart,
  // The contrast pass (#455) runs here too.
  mapBriefStory: (): Fixture => ({ ...mapBriefStory(), contrast: true }),
  mapBriefAnnounce,
  mapBriefStepCode,
  mapBriefAlsoGroups,
  mapBriefCopied,
  mapBriefCopyByHand,
  mapBriefSince,
  mapBriefStates: () => mapBriefFigure("states"),
  mapBriefSequence: () => mapBriefFigure("sequence"),
  mapBriefCompare: () => mapBriefFigure("compare"),
  mapBriefHeights,
  mapBriefTall,
  mapBriefTaller,
  mapBriefTallScroll,
  mapBriefUnconfirmed,
  mapBriefPartialFacts: () => mapBriefPartialFacts(false),
  mapBriefPartialFactsOpen: () => mapBriefPartialFacts(true),
  mapBriefNoFacts,
  mapBriefStaleBase,
  mapBriefNoStaleBase,
  mapBriefWhy: () => mapBriefWhy(false),
  mapBriefWhyEsc: () => mapBriefWhy(true),
  mapBriefWhyDecision,
  mapBriefPickRight: () => mapBriefPick(1),
  mapBriefPickWrong: () => mapBriefPick(0),
  mapBriefOrderSlots,
  mapBriefOrderWrong,
  mapBriefOrderRight,
  mapBriefRatify,
  mapBriefExplained: () => mapBriefExplained("where"),
  mapBriefRatifyExplained: () => mapBriefExplained("ratify"),
  mapBriefExplainedOff: () => mapBriefExplained("off"),
  mapBriefRatifyDetail,
  mapBriefRatifyEvidence: () => mapBriefRatifyEvidence("line"),
  mapBriefRatifyEvidenceList: () => mapBriefRatifyEvidence("list"),
  mapBriefRatifyEvidenceSite: () => mapBriefRatifyEvidence("site"),
  mapBriefMapCheck: () => mapBriefMapCheck(false),
  mapBriefMapCheckAlone: () => mapBriefMapCheck(true),
  mapBriefReviewFlag: () => mapBriefReview("flag"),
  mapBriefReviewRunning: () => mapBriefReview("running"),
  mapBriefStillTrueRow: () => mapBriefStillTrue("row"),
  mapBriefStillTrueAlone: () => mapBriefStillTrue("alone"),
  mapBriefGuard: () => mapBriefGuard(false),
  mapBriefGuardConfirm: () => mapBriefGuard(true),
  mapBriefBar,
  mapBriefExplainBack: () => mapBriefExplainBack(false),
  mapBriefExplainBackDone: () => mapBriefExplainBack(true),
  mapBriefOverturnCopied: () => mapBriefOverturn("copies"),
  mapBriefOverturnFallback: () => mapBriefOverturn("refuses"),
  mapBriefWriting,
  mapDiffSplit,
  mapDiffSplitBrief,
  mapDiffRemoval,
  mapDiffRemovalBrief,
  mapDiffLitGhost,
  mapDiffRatifyLit,
  mapDiffClosed,
  mapBriefShort,
  mapCatchUp,
  mapCatchUpWriting,
  mapCatchUpWritingStage: () => mapCatchUpStage({ kind: "writing" }, "Writing…"),
  mapCatchUpCheckingStage: () => mapCatchUpStage({ kind: "checking" }, "Checking claims…"),
  mapCatchUpFixingStage: () => mapCatchUpStage({ kind: "fixing", claims: 2 }, "Fixing 2 problems…"),
  mapCatchUpSummarising,
  mapCatchUpBold,
  mapCatchUpCard: (): Fixture => ({ ...mapCatchUpCard(), contrast: true }),
  mapBriefBold,
  mapBriefBoldUnpaired,
  mapSheetUnratified,
  mapStale,
  mapMarksThick: () => mapMarks("thick"),
  mapMarksThin: () => mapMarks("thin"),
  mapMarksStale: () => mapMarks("stale"),
  mapMarksBroken: () => mapMarks("broken"),
  mapMarkHints,
  mapRefresher,
  mapPurposeMore,
  mapCaughtUp: (): Fixture => ({ ...mapCaughtUp(), contrast: true }),
  mapCatchUpOlder: (): Fixture => ({ ...mapCatchUpOlder(), contrast: true }),
  mapTalksTo,
  mapSheetExplained: () => mapSheetExplained(true),
  mapSheetExplainedOff: () => mapSheetExplained(false),
  mapBriefPairLight,
  mapBriefChapters,
  mapBriefChapterMid,
  mapBriefChapterLast,
  logRunning: () => logFixture("running"),
  logDone,
  // The contrast pass (#455) runs here too.
  list: (): Fixture => ({ ...list(), contrast: true }),
  listSecondLines,
  listClearLanded,
  listHeld,
  paletteOpen,
  logos,
  noticeErrors,
  notRecording,
  recording,
  alerts,
  projectsSaved,
  projectChevron,
  // A header's "⋯" menu (#458) opens under its button and inside the window, and flips over it near the window's bottom edge.
  projectMenu: () => withSaved(projectMenuScript(false), { realPointer: true, snapDone: "project menu ok" }),
  projectMenuBottom: () => withSaved(projectMenuScript(true), { realPointer: true, snapDone: "project menu ok" }),
  mapProjectGone,
  mapProjectRemoved,
  mapCalm,
  mapCamera,
  mapFog,
  mapPreFlightPick,
  mapFogPlates,
  // The Map view busy (#311, #562): a badge on each part a session's latest turn touched, a Worker's part counting its session, a Needs-you
  // badge, and the not placed line for the one that needs Daniel (the others have no files yet, are agy, status-only Codex or ignored-only edits); a Helper and a finished session never show.
  mapBusy,
  mapSheetStill,
  // Session badges (#562): an accented badge and its open list, and the spotlight's view with a dimmed and a full Needs-you badge.
  mapBadgeList,
  mapSpotlight,
  mapFootprint,
  // "This session here" on a part's sheet beside the open story (#561), with and without its footer.
  mapSheetHere: () => mapSheetHere(false),
  mapSheetHereOlder: () => mapSheetHere(true),
  mapLatePlace,
  // Brief and Decisions open into view at the panel's foot (spec #661).
  mapPanelBriefFoot,
  mapTurnLight,
  // The story patched in place (spec #661): a press right after a live update lands where it was aimed.
  mapPanelPatched,
  // A side question's answer stays with its turn's past row (spec #661).
  mapPanelAskStays,
  // The map from the keyboard (#460).
  mapPartKeys,
  mapPartRedraw,
  mapGlassMotion,
  // The left panel over the map (#385): the story with a part's sheet, Esc's peel and a second press, the widened diff, History, Settings,
  // `]`'s toggle, and a brief over both panels. `mapPanelNarrow` is the story at 1000x800 (snap:all's size for it), where only one overlay opens.
  mapPanelStory,
  mapPanelHead,
  mapPanelLine2,
  mapPanelNoSession,
  mapPanelPeel,
  mapPanelToggle,
  // `[` in a wide window: the full list swaps for its strip and back (spec #658).
  mapListFold,
  mapListFoldNarrow: mapListFold,
  mapPanelDiff,
  mapPanelHistory,
  mapPanelSettings,
  mapPanelFieldHints,
  mapNotPlacedHint,
  mapToastBeside,
  mapPanelBrief,
  mapPanelNarrow: mapPanelStory,
  mapPanelFocusNarrow: mapPanelFocus,
  // CI on the default branch broke a law (#312): the red mark on the shell next to a Needs you chip; then with "See the proof" open, and
  // with the broken-law drill-down open.
  mapBroken: () => mapBroken("none"),
  mapBrokenSheet: () => mapBroken("sheet"),
  // Laws left open in this browser (#637), so the law with its proof shows.
  mapProof: (): Fixture => ({ ...mapBroken("proof"), storage: { "map-sheet-folds": JSON.stringify(["laws"]) } }),
  mapBrokenLog: () => mapBroken("log"),
  // A part's history strip scrubbed to an older commit, with the sheet open (#313).
  // The contrast pass (#455) runs here too.
  mapScrubbed: (): Fixture => ({ ...mapScrubbed(), contrast: true }),
  mapReview,
  // No map on the default branch: the empty state shows the drafting prompt to give to a session.
  // The map as the page's base (#386), each also below 1007 px, where the list is its strip.
  mapBase,
  mapBaseNarrow: mapBase,
  readmeHero,
  mapHeaderDot,
  mapHeaderDotNarrow: mapHeaderDot,
  // Switching projects: both views saved, a gone folder never read on focus, a PR brief closed.
  mapSwitchSavesViews,
  mapGoneFocus,
  mapBriefSwitch,
  // The header dots: a failed read is tried again, and an older answer never lands over a newer one.
  mapDotsRetry,
  mapDotsOrder,
  mapFresh,
  mapFreshNarrow: mapFresh,
  // The free scan of a repository with no map (#760): its grey plates and lines, Draft the map muted without claude, and the layout following the payload.
  mapScan,
  mapScanNarrow: mapScan,
  mapScanMerge,
  mapScanNoClaude,
  mapScanSwitch,
  // The Map draft (#763): the layout steps from scan to review, drafting with a group in, one checking and one failed, the review screen
  // with a flag and the Core line, and Redraft this part relocking Accept, then Discard back to the scan.
  mapDraftSteps,
  mapDrafting,
  mapDraftingNarrow: mapDrafting,
  mapDraftReview,
  mapDraftRedraft,
  mapDraftAccept,
  mapDraftAcceptNarrow: mapDraftAccept,
  mapDraftReplace,
  mapDraftWrite,
  // The part sheet in the map: one scrolling column, a resizable width, and a click on the empty map closes it (#445).
  mapSheetScrollShort,
  // The sheet's purpose, then its held items with their key files (#483).
  mapSheetHolds,
  mapSheetWidth,
  mapSheetResizeRedraw,
  mapSheetKeptWidth,
  mapSheetClose,
  mapEmpty,
  // The map's sources (#753): a checkout map with its check's problems, the default branch's with none, and a folder in no git repository.
  mapCheckout,
  mapCheckoutNarrow: mapCheckout,
  mapDefaultNoBanner,
  mapNotGit,
} satisfies Record<string, () => Fixture>;
