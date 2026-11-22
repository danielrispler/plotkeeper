# PR briefs

A **PR brief** is the story of one ready PR, written once by a sandboxed writer from facts code assembles, held to checks, and played on the Map view so Daniel understands what the PR changes before he merges it (spec danielrispler/plotkeeper#334). This is the one doc for it; [the system map](map.md) covers the view it plays on, the fog it clears and the part sheet. Vocabulary (PR brief, chapter, Ready for review, ratify, claim check) is in [`CONTEXT.md`](../CONTEXT.md).

Code: `src-tauri/src/prs.rs`, `src-tauri/src/pr_briefs.rs`, `src-tauri/src/ratify.rs`, `src-tauri/src/fog.rs`, `src-tauri/src/stop_rules.rs`; `understanding/src/pr_brief.rs`, `understanding/src/pr_brief_check.rs`; `ui/map-tray.ts`, `ui/map-brief.ts`, `ui/map-model.ts`, `ui/map-ratify.ts`, `ui/ratify.ts`, `ui/stop-rules.ts`.

## Ready detection

The shell asks `gh pr view <branch> --json number,title,isDraft,state,headRefOid,body,closingIssuesReferences` for each listed session's branch on Landed's triggers, never on a timer: at once when a turn ends, and when the window gains focus or the Map view opens, through one cooldown of a minute per repository with an in-flight guard (`prs.rs`). A PR is **ready** when it is open and not a draft. Its head is kept in memory and replaced by a later lookup. Any PR number found still names its session ("PR #N").

A lookup of a ready PR (`pr_briefs.rs`):

- With no stored brief at that head, the brief is written at once.
- A new head marks the stored brief stale. It is rewritten when Daniel opens it, or when two lookups in a row see the same new head.
- A PR whose touched parts would not fog at Daniel's effective tiers gets no brief (see Shape): nothing is written, it is not gathered again at the same head, and its tray row says "no brief: nothing you track changed".

Home's quiet dot, the rail's row and the history strip's "Brief" link all read the stored briefs (`briefs_waiting`, `any_waiting`); they are described in [the map doc](map.md).

## The writer and its facts

`write_pr_brief` is the one call into the writer (`understanding/src/pr_brief.rs`). The model is the "PR briefs" setting ([settings](../settings.md)), Opus by default, and serves the writer, the claim check and the retry alike. The writer is sandboxed with no tools: everything it reads is in the facts, which `build_facts` assembles from `gh` (`fetch_meta`) and `git` (`gather`):

- The PR's title and body, the spec or ticket it closes (trimmed to the sections the writer needs), its checks' conclusions.
- The map at the PR's head (or as of its birth for an older PR), the map's diff, and the parts the diff touches with their tier (Daniel's tier overrides the map's), laws, the tagged tests that back each law and whether this PR changed one, and the flows through them.
- **Shape**, decided by code: a touched part fogs when its tier is Core, or Interface-only and named in the map diff. Up to two fogging parts is a **single story**; more is **chapters by behaviour** (2 to 4, each one behaviour lighting every part it crosses), with an overview when a flow changed. Parts that do not fog are "also touched", one line each.
- The diff, hunk by hunk inside a 64 KiB budget (`BUDGET`): the logic files of the parts that must be lit first, then other parts' logic, then tests, then the rest, smaller files first within each. What was cut goes into a "DIFF NOT SENT" note, and the budget record says how much was sent and cut. A brief written from part of the diff says so ("Written from part of the diff").

The reply is the brief: a title, steps (kinds Where, Before, Now, Must stay true, Could break, Not in the plan, Your turn), each with a heading, a short body, the parts it lights, the links its light runs along, quotes that cite the facts, risks, a figure drawn by code from the writer's labels (`states`, `sequence`, `compare`), a covered-by test and at most one check; also-touched lines; and the decisions the agent declared. Decisions get ids from code at the first write, and a rewrite keeps an earlier decision's id when its text matches.

## Checks and the claim check

Code checks (`check_brief`) flag each item they concern, and the brief as a whole: word caps on headings and bodies, the banned words ("guarantee", "ensure", "cannot", ...) and phrases that leak the sandbox ("was not sent", "didn't see"), a quote that is not in the facts, a part, link, law or flow the map does not hold, a risk or hop that does not exist, a pick check whose test is not in the diff, an order check not keyed to a changed flow, chapter and step counts, a required part no chapter lights, and a "Your turn" with no check.

The **claim check** (`pr_brief_check.rs`) is a second sandboxed call that reads the same facts and the written brief and says, per item (steps with their check and risks, the also lines, the decisions), whether the facts support it. The code flags and the claim flags go into **one retry** that rewrites only the flagged items; flags of a chapter or of the brief as a whole are left as the code set them. An item still flagged after the retry carries "not confirmed from the facts". A failed claim call or retry leaves the code-checked brief stored and says why in the record.

## The record

Each brief written appends one line to `pr-brief-records.jsonl`, beside the briefs: the PR and head, the shape, the budget and what was cut, the code checks' flags, the claim check's flags before the retry and the ones that still stand after, the flags the stored brief carries, the count left unconfirmed, any claim or retry error, and each call's tokens and cost. It is for tuning prompts, caps and models from real data; nothing shows it. A brief is stored per PR and head under the data folder's `pr-briefs-by-project/<project>/pr-briefs/`, and a reader ignores fields it does not know.

## The tray and the view

The tray at the map's foot lists the project's ready PRs (`ui/map-tray.ts`): a PR whose brief is stored is one row, or one row per chapter, that opens it; one without says its brief is being written or not written yet; a quiet one says "no brief". Opening a row (`openBriefOf`) reads the brief (the shell rewrites a stale one first), returns a scrubbed map to now, and plays it from the step it was left at; closing it with Esc keeps the step and finishing it starts it over (`map-brief.ts`, `map-model.ts`).

Playing: the light runs a step's links at one speed and each part lights as it arrives, the camera frames the parts the brief lights, the panel keeps its tallest step's height and crossfades, citations sit in quiet type ("What this rests on"), and "See the code" opens the quote in the drawer above the brief. Under reduced motion a whole step lights at once. A step with a flagged item shows "not confirmed from the facts"; "Also touched" lines show on the last step of the last chapter. Esc peels the drawer, then the brief, then a scrubbed commit, then the part sheet, then the view.

## Chapters

A chaptered brief has one tray row per chapter ("2 of 3"); each opens on its own and resumes at the step left. A chapter's frame holds every part its behaviour crosses. The overview, when there is one, plays once per brief, before the first chapter opened. A single story is one chapter over all its lit parts.

## The check

A step's check is one prediction. A **pick** is answered in the panel: pick an option, press it again to take it back, then Check; a wrong answer shows the correction. An **order** check is answered on the map: tap parts into numbered slots, Check grades it, a wrong answer nudges the parts out of place and the right path replays along the links. Right or wrong, answering counts for the chapter. Nothing is scored.

## Fog clearing

A chapter is **done** when its last step's words were seen (`watchSeen`) and its check was answered, right or wrong, when it holds one (`chapterGate`). The window tells the shell once (`brief_chapter_done`), which keeps "read at head X" per chapter in `map-state.json` and clears the PR's fog (`fog.rs`) from every part whose chapters are all done, lifting the mist. A part no chapter lights keeps its fog; leaving mid-chapter clears nothing. A PR read before it merges files fog at merge only for what was pushed after the read, limited to the paths the squash commit changed. "I know this" on a part's sheet still clears a whole part. Details of the fog are in [the map doc](map.md).

## Ratifying

The brief ends with a **Ratify** step listing what Daniel has not yet looked at (nothing is added when all is ratified): each map or law change the PR made (read by code from the map's diff, keyed by PR, kind and map id) and each decision the agent declared (keyed by PR and decision id). Each has **Accept** and **Overturn**. Ratifying never blocks anything; the merged map is in effect at once and the record (`ratify.json` in the data folder, per project) only says Daniel looked. Overturn records the decision, then opens New session in the project with a prompt quoting the item, or types the prompt, without Enter, into the PR's session when it is live. The part sheet marks the same two buttons on a law row or a link at the part that landed after the project's first run with no record. The full rules, including the `since` date and `pending` records, are in [the map doc](map.md).

## Stop rules

The brief's use is counted quietly in `stop-rules.jsonl`, with no setting and nothing shown, beside the map's own events ([map doc, Stop rules](map.md)). Each brief event names its project and PR:

- `brief-offered`: the PR's tray row was seen (`watchSeen`), once per PR; the shell keeps the offer to one line however often the tray is drawn or Studio restarts. A row with no brief to open offers nothing.
- `brief-opened`: a tray row (or the rail's or history strip's link) opened the brief.
- `chapter-done`: a chapter was read to its end, as above.
- `check-answered`: a check was graded.
- `ratify`: Accept or Overturn was pressed, from the brief or the part sheet.
- `unconfirmed-seen`: a "not confirmed from the facts" mark was seen, once per PR in a window.

The one-time review (14 active days and 20 merged PRs, whichever comes last) lists **PR briefs** among the little-used modules when briefs were opened for fewer than a quarter of the merged PRs that had one offered. Offered and opened are counted among the PRs merged since tracking began, so an abandoned PR counts for nothing. It also adds a sentence when "I know this" cleared more fog than `chapter-done` did, but only where briefs were offered. Never a score, a streak or a count on screen.
