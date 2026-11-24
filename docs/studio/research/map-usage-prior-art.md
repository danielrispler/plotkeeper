# Map usage issues: prior art from comparable tools

Prior-art notes for the six issues Daniel reported on 2026-11-24 (`issues-and-diagnosis.md` beside this file). One section per issue, each ending with "Patterns the benchmarks agree on" and "Options for the grilling". Studio terms follow `docs/studio/CONTEXT.md`; each tool's own words appear only when describing that tool. Earlier notes (`shell-polish.md`, `home.md`, `herdr.md`) are not redone.

## How this was read

- Primary sources only: vendor docs, changelogs, product pages, source code on GitHub, specs. Pages were read through WebFetch/WebSearch on 2026-11-24. Nothing was run, no product was installed, no credential was read.
- Every claim carries its URL. "Unverified" marks what the page read did not confirm; a few claims in sections 5 and 6 are marked "secondary" where only a non-vendor page was found.
- Source code was read at these commits (shallow clones, read as text, never built): t3code `d7ae1e17`, openai/codex `a0d5b0a6`, zed `2b248e60`, tldraw `c9cbd297`, excalidraw `542ad76f`, xyflow `9635a064`, vercel/streamdown (default branch). Line numbers are approximate; GitHub blob URLs use `main`.
- Pages were read through a summarizing fetch tool, so quotes are as that tool returned them. The web-search limit was hit partway through section 2, so Figma, Notion, Linear and Miro panel facts are unverified there.

## Strongest findings

1. **Session names are written by a small model, not cut from the prompt** (Codex: 36 chars, under five words; T3 Code: 3-8 words, under 40 chars, "Do not copy and truncate the user's message"; Zed: 3-7 words). Until the title arrives, the prompt is cut by characters with an ellipsis; no tool cuts at N words. Permission mode lives in the composer row, not the header.
2. **Side sheets cap their height and scroll inside** (Excalidraw: window height minus 166px, `overflow-y: auto`). Resizable docks agree: VS Code (min 170, default 300, 4px sash, saved, double-click resets) and Zed (6px handle, saved, double-click resets).
3. **Click vs drag is decided by pointer-down-to-up distance in screen px** (xyflow 1px, tldraw 4px/6px touch, Excalidraw 10px), and a click on empty canvas deselects in all three. xyflow, where a left drag pans as in the studio's map, decides on pointer up.
4. **Review bots write summaries ahead of time and show a placeholder, not a stream** (CodeRabbit, Bugbot, Greptile); streaming is the chat pattern and one of Anthropic's three latency levers. Anthropic documents a low-effort check with high-effort reruns of failures only, prompt caching from 512 tokens, and Batches at half price for background pre-writing. The retry in 7/7 writes points to a writer-prompt fault.
5. **Nobody puts model HTML into the page unchecked** (OWASP LLM05, marked, react-markdown). Slack and Notion keep emphasis as a field in the data, which plain `textContent` can render.
6. **No tool keeps an empty figure slot**: diagrams appear only when the change calls for one (CodeRabbit, Greptile), and one-column steps just get shorter (Mintlify, Distill).
7. **Every AI assistant's feedback starts as an inline thumbs pair**, with reasons and text revealed after the click and optional (ChatGPT, Claude.ai, Gemini, Copilot, GitLab Duo). No product pairs "say it back" with a quality rating.

## 1. Session header in a narrow panel

### How names get short: per tool

- **Claude Code (CLI, agent view, desktop).**
  - An unnamed session gets a generated title: "a short summary of your first prompt, written by a background request to the small/fast model, normally a Haiku-class model". Accepting a plan replaces it with a title based on the plan. https://code.claude.com/docs/en/sessions ("Name your sessions")
  - Two labels exist: a default display name (working dir + 2-char suffix, e.g. `my-app-3f`) used in running-session listings, and the generated title. Picker rows show the first that exists: name you set, AI title, conversation summary, one of your prompts. https://code.claude.com/docs/en/sessions
  - `/rename` with no argument generates a name from the conversation; names capped at 200 characters (per the commands page as surfaced by search; I did not open that page directly: unverified). https://code.claude.com/docs/en/commands
  - Exact title length or prompt: not published (unverified).
  - Agent view row: `icon(state, colour/animation) | name | activity summary | [#PR] | age`, one line. The automatic name is "a short label written by a Haiku-class model"; the activity text is also written by a Haiku-class model, updates at most every 15 s, "fills the row's remaining width" and is clipped at the edge, with the full text in a peek panel. When grouped by state, the group header names the state, so the row omits it; grouped by directory, the summary starts with a coloured state word (`Needs input · double jump or wall climb?`). Permission mode is not a row field. https://code.claude.com/docs/en/agent-view
  - Desktop app: rename by clicking the session title in the title bar; a caret beside the title opens the session menu. The permission mode selector is "in the row under the prompt box", not in the header (shortcut Cmd-Shift-M). https://code.claude.com/docs/en/desktop

- **Codex (TUI / app).**
  - Generated title, hard limit `THREAD_TITLE_MAX_CHARS = 36`, model `gpt-5.6-luna` at low reasoning effort (falls back to the session's current model if that one is not available); prompt input capped at 960 bytes. https://github.com/openai/codex/blob/main/codex-rs/tui/src/app/thread_title.rs (~line 34-37)
  - Prompt (verbatim): "Generate a concise, single-line task title of at most 36 characters and under five words where possible. Start with an imperative verb. Capitalize only the first word unless ... Preserve ticket references exactly. Write in the user's language. Do not use quotes, markdown, or trailing punctuation. Do not answer the request." Output is structured JSON with `maxLength: 36`, and the result is cut to 36 chars again after normalising. Same file, ~lines 328, 336-346, 536.
  - Until the title arrives (and as fallback): "Desktop shows a single line from the first message": markdown stripped, whitespace collapsed, cut at 60 characters with `…`, trailing space trimmed (character cut, not word cut). Same file, `fallback_thread_title`, ~lines 348-375.
  - A manual rename cancels pending generation. Same file, ~lines 1-5, 55-65.
  - App: rename with Cmd-Opt-R or by double-clicking in the thread list (app 26.203). https://developers.openai.com/codex/app/commands , https://developers.openai.com/codex/changelog?type=codex-app
  - App-server thread status values: `notLoaded`, `idle`, `systemError`, `active` (+ `activeFlags`). https://developers.openai.com/codex/app-server

- **T3 Code.**
  - Seed title before generation: prompt text normalised and cut to 50 characters plus `...` (`truncate(text, maxLength = 50)`), or "Attached image(s)", "File: name", else "New thread". https://github.com/pingdotgg/t3code/blob/main/packages/client-runtime/src/operations/threadTitle.ts (~line 54-80), https://github.com/pingdotgg/t3code/blob/main/packages/shared/src/String.ts (line 1-9)
  - Generated title rule: "3-8 words, fewer than 40 characters", "compact noun phrase or clear action phrase", "Do not copy and truncate the user's message", never include the project name because "the UI already shows it next to the title", drop incidental instructions (models, tools, output formats). Returns JSON `{title, needsRefinement}`; a regenerate prompt re-titles later from the conversation. A user setting `threadTitleInstructions` overrides length rules. https://github.com/pingdotgg/t3code/blob/main/packages/provider-core/src/server/textGenerationPrompts.ts (~lines 395-470)
  - Default title model: `gpt-6-luna`, reasoning effort `low`, user-changeable (`textGenerationModelSelection`). https://github.com/pingdotgg/t3code/blob/main/packages/contracts/src/model.ts (~line 136-139), https://github.com/pingdotgg/t3code/blob/main/packages/contracts/src/settings.ts (~line 1560)
  - Header title: one line, CSS `truncate` (ellipsis) in a `min-w-0` flex item, full title in a tooltip on hover; click opens the thread menu, double-click renames inline. https://github.com/pingdotgg/t3code/blob/main/apps/web/src/components/chat/ChatHeader.tsx (~lines 336-400), https://github.com/pingdotgg/t3code/blob/main/apps/web/src/components/WorkspaceBreadcrumb.tsx (~line 27-35)
  - Permission mode ("Supervised" / "Auto-accept edits" / "Full access") lives in the composer, not the header. https://github.com/pingdotgg/t3code/blob/main/apps/web/src/components/chat/ComposerPermissionsSection.browser.test.tsx (~lines 50-75)

- **Zed agent panel.**
  - Title prompt (verbatim): "Generate a concise 3-7 word title for this conversation, omitting punctuation. Go straight to the title, without any preamble ... Be descriptive. DO NOT speak in the first person." https://github.com/zed-industries/zed/blob/main/crates/agent_settings/src/prompts/summarize_thread_prompt.txt
  - Only the first line of the streamed reply is kept (stops at the first `\n`/`\r`); no character cap in code. https://github.com/zed-industries/zed/blob/main/crates/agent/src/thread.rs (`stream_thread_title`, ~line 5492-5512)
  - Model: setting `agent.thread_summary_model`, else the provider's default fast model, else the default model. https://github.com/zed-industries/zed/blob/main/crates/language_model/src/registry.rs (~line 522-532), https://github.com/zed-industries/zed/blob/main/docs/src/ai/agent-settings.md (~line 40)
  - Placeholder until generated: "New Agent Thread". https://github.com/zed-industries/zed/blob/main/crates/agent_ui/src/agent_ui.rs (`DEFAULT_THREAD_TITLE`)
  - Header line layout of the panel: not checked (unverified).

- **Workmux.** No session titles; tmux window name is `<prefix><handle>` from the branch. `--auto-name` asks a cheap model for a kebab-case branch name, default prompt "1-3 words, max 4 if necessary"; agent defaults include `claude --model haiku -p` and `codex exec ... -m gpt-6-luna`. Status is a single icon prefixed to the window name (working / waiting / done, configurable). Sidebar row: status icon (spinner) + `project/worktree` + elapsed time; sidebar 10% of terminal width clamped to 25-50 columns. https://github.com/raine/workmux/blob/main/README.md (~lines 527-541, 933-1040, 1955-2010)

- **cmux.** Vertical tabs show git branch, PR status/number, working directory, listening ports and latest notification text; a waiting agent gets a blue ring on its pane and a lit tab. No generated titles found. https://github.com/manaflow-ai/cmux/blob/main/README.md (~lines 65, 150, 418)

- **Cursor cloud agents.** Display name up to 100 characters; "When omitted, Cursor auto-derives a name from the prompt" (method not published: unverified). States `ACTIVE`/`IDLE`/`ARCHIVED`; an idle parent's header reads "Waiting on" its child agents. https://cursor.com/docs/cloud-agent/api/endpoints , https://cursor.com/docs/release-notes/cloud-agents

- **herdr, AgentsRoom.** herdr already known (row `[logo][name][time][status]`). AgentsRoom: no primary source found (unverified).

### Patterns the benchmarks agree on

- **Model-written short title, not a word cut.** Claude Code (Haiku-class), Codex (36 chars, under 5 words), T3 Code (3-8 words, under 40 chars), Zed (3-7 words) all generate a title with a small/fast model. T3 Code's prompt says outright "Do not copy and truncate the user's message".
- **A placeholder while it generates is the prompt cut by characters with an ellipsis**: Codex 60 chars + `…`, T3 Code 50 chars + `...`. Nobody cuts at N words.
- **Rendering is one line + CSS ellipsis, full text on hover or in a peek** (T3 Code tooltip, Claude Code agent view peek panel). Rename is a click or double-click on the title (T3 Code, Claude desktop, Codex app).
- **Permission mode is not in the header.** Claude Code desktop and T3 Code put it in the composer row; Claude agent view rows leave it out.
- **State is an icon/colour, not a word, when space is tight** (Claude agent view icon, Workmux icon, cmux ring); the word appears only when no group header already says it.
- **Title rules strip what the UI already shows** (T3 Code: no project name; Codex/T3: no quotes or trailing punctuation).

Split: Codex demands an imperative verb ("Fix login redirect"); T3 Code allows a noun phrase; Zed asks for "descriptive". Hard char cap: Codex enforces 36 in schema and code; Zed has none; T3 Code only in the prompt.

### Options for the grilling

1. **Generated title like Codex**: a cheap model call (Haiku-class) with "at most 36 characters, under five words, imperative verb, no quotes/punctuation", schema `maxLength` + code clamp; placeholder is the prompt cut at ~50-60 chars with `…`. (Codex; T3 Code for placeholder.)
2. **Generated title like T3 Code / Zed**: "3-8 words, fewer than 40 characters, noun or action phrase, don't copy the prompt, drop incidental instructions", regenerate later when the subject is known. (T3 Code, Zed.)
3. **No model, full prompt line with CSS ellipsis + tooltip**: drop the 6-word cut, render the first line of the prompt in one `min-width:0; text-overflow: ellipsis` line with the full text on hover. (T3 Code header rendering, Claude agent view clipping.)
4. **Header in one line: state icon + title + actions**, with the mode moved out: status as a coloured/animated icon left of the title (Claude agent view, Workmux), "auto mode" moved next to the prompt/composer or to a details area (Claude desktop, T3 Code), phase on the second line only. (Claude Code agent view + desktop.)
5. **Two lines: title row, then a state line** (`Running · Orienting`) like Claude agent view's directory grouping (`Needs input · question`), with mode as a small badge only when it differs from the default (unverified as a benchmark pattern; no tool found showing mode in the header).

## 2. Part sheet over the map

### Per tool

- **Excalidraw (properties panel, "App-menu__left").**
  - Height: inline `maxHeight = window height - 166px` ("approximate height of hamburgerMenu + footer"), minus the minimap's reserved height when the minimap is on; the panel has `overflow-y: auto`, so it scrolls inside instead of running off-screen. https://github.com/excalidraw/excalidraw/blob/master/packages/excalidraw/components/LayerUI.tsx (~lines 258-317), https://github.com/excalidraw/excalidraw/blob/master/packages/excalidraw/css/styles.scss (~lines 514-520); class name from https://github.com/excalidraw/excalidraw/blob/master/packages/common/src/constants.ts (~line 119)
  - Width: container `width: fit-content`, panel `max-width: 12.5rem` (200px); compact mode `min-width: 48px`. Not resizable. styles.scss ~lines 461-467, 514-520. The compact variant uses `max-height: calc(100vh - 200px); overflow-y: auto` (https://github.com/excalidraw/excalidraw/blob/master/packages/excalidraw/components/Actions.scss ~line 237).
  - Click vs drag: `DRAGGING_THRESHOLD = 10` px (screen px; scene distances are divided by zoom). A pointer that moved more than this between down and up is not a click. https://github.com/excalidraw/excalidraw/blob/master/packages/common/src/constants.ts (line 34), App.tsx ~line 1606
  - Deselect: on pointer down on empty canvas (no element hit, no shift, not inside the selection's bounding box) it calls `clearSelection`, and the panel (which shows only with a selection) goes away. Panning is a separate gesture (hand tool / space / wheel), so a plain left drag on empty canvas starts a box selection, not a pan. https://github.com/excalidraw/excalidraw/blob/master/packages/excalidraw/components/App.selectionTool.ts (~lines 320-345)

- **tldraw.**
  - Drag threshold options: `dragDistanceSquared: 16` (4 px), `coarseDragDistanceSquared: 36` (6 px, touch), UI `uiCoarseDragDistanceSquared: 625` (25 px); `doubleClickDurationMs: 450`, `multiClickDurationMs: 200`. Compared as squared screen distance from pointer-down origin. https://github.com/tldraw/tldraw/blob/main/packages/editor/src/lib/options.ts (~lines 290-305), https://github.com/tldraw/tldraw/blob/main/packages/editor/src/lib/editor/Editor.ts (~lines 11398-11408)
  - Deselect: the select tool's `PointingCanvas` state calls `selectNone()` on entering, that is on pointer down on empty canvas (unless shift/accel); if the pointer then passes the drag threshold it becomes a brush selection. https://github.com/tldraw/tldraw/blob/main/packages/tldraw/src/lib/tools/SelectTool/childStates/PointingCanvas.ts (~lines 10-31)
  - Right-button drag: stays a "pointing" state until it moves more than `dragDistanceSquared`, then turns into panning (otherwise it is a right click / context menu). Editor.ts ~lines 11360-11370.
  - Properties panel sizing: not checked (unverified).

- **React Flow / xyflow.**
  - `paneDragThreshold` (default `1` px): "Distance in pixels the pointer has to move before dragging the pane starts panning or a selection. If the pointer moves less, the gesture is treated as a click: `onPaneClick` ... is called and the viewport does not move." Applies to every button in `panOnDrag` and to `selectionOnDrag`. https://github.com/xyflow/xyflow/blob/main/packages/react/src/types/component-props.ts (~lines 259-266)
  - It replaced `paneClickDistance` (#6115); earlier `paneClickDistance` default was set to 1 (#5621). https://github.com/xyflow/xyflow/blob/main/packages/react/CHANGELOG.md (~lines 67, 462)
  - Check is Euclidean squared, same as d3-zoom's `clickDistance`: `dx*dx + dy*dy > threshold^2`; a pinch always counts as a drag. https://github.com/xyflow/xyflow/blob/main/packages/system/src/xypanzoom/eventhandler.ts (~lines 280-292), XYPanZoom.ts ~line 161
  - On a pane click React Flow calls `onPaneClick` and then `resetSelectedElements()`, so a click on empty pane deselects by default while a drag pans. https://github.com/xyflow/xyflow/blob/main/packages/react/src/container/Pane/index.tsx (~lines 122-138)
  - `nodeDragThreshold` and `connectionDragThreshold` default 1 px. component-props.ts ~lines 672-684.
  - Side-panel examples: not checked (unverified).

- **VS Code.**
  - Side bar and secondary side bar: `minimumWidth = 170`, `maximumWidth = Infinity`. https://github.com/microsoft/vscode/blob/main/src/vs/workbench/browser/parts/sidebar/sidebarPart.ts (~line 48), https://github.com/microsoft/vscode/blob/main/src/vs/workbench/browser/parts/auxiliarybar/auxiliaryBarPart.ts (~line 54)
  - Default sizes 300 (side bar, auxiliary bar, panel), persisted per profile (`sideBar.size`, `auxiliaryBar.size`, `StorageScope.PROFILE`). https://github.com/microsoft/vscode/blob/main/src/vs/workbench/browser/layout.ts (~lines 3459-3461)
  - Sash (drag handle): default 4 px wide, hover highlight delay 300 ms; double-click fires `onDidReset` (resets size). https://github.com/microsoft/vscode/blob/main/src/vs/base/browser/ui/sash/sash.ts (~lines 147-155, 265, 451)

- **Zed docks.**
  - Default widths: project panel 240, outline panel 300, git panel 360, collaboration panel 240, agent threads sidebar 300, agent panel 640 fixed (but `flexible: true` by default = proportional width; "resetting the panel restores the default proportion"), bottom dock height 320. https://github.com/zed-industries/zed/blob/main/assets/settings/default.json (~lines 882-1212)
  - Resize handle 6 px; panel size clamped to at least the handle size and to the window; double-click on the handle resets panel sizes and the size is saved with the workspace (`serialize_workspace`). https://github.com/zed-industries/zed/blob/main/crates/workspace/src/dock.rs (~lines 25, 405, 1220-1292)

- **Figma.** Left sidebar is resizable by dragging its right edge. https://help.figma.com/hc/en-us/articles/360039831974-View-layers-and-pages-in-the-left-sidebar . Whether the right properties panel is resizable today: unverified (only old community threads say it was not; my search quota ran out before I could check the current help center).

- **Notion side peek.** Opens the page on the right while the database "remains interactive on the left"; Table/Board/List/Timeline default to side peek, Gallery/Calendar to center peek; ⤡ expands to full page. https://www.notion.com/help/views-filters-and-sorts , https://www.notion.com/help/layouts . Resize and click-outside behaviour: not documented (unverified).

- **Linear peek.** Space toggles peek; holding Space keeps it open only while held. https://linear.app/docs/board-layout , https://linear.app/changelog/2019-09-23-improved-peek . Width / click-outside: not documented (unverified).

- **Miro.** No primary source found (unverified).

### Patterns the benchmarks agree on

- **Bounded height, inner scroll.** Excalidraw caps the panel at window height minus its chrome (166 px) and scrolls inside (`overflow-y: auto`). None of the tools let a panel run past the window edge.
- **Click vs drag is a small screen-pixel distance between pointer down and up, not time.** xyflow 1 px (default), tldraw 4 px (6 px touch), Excalidraw 10 px. All measure Euclidean distance in screen pixels (Excalidraw and tldraw undo zoom). Time is used only for double-click (tldraw 450 ms).
- **A click on empty canvas deselects.** xyflow (`resetSelectedElements` after `onPaneClick`), tldraw (`selectNone`), Excalidraw (`clearSelection`). An inspector tied to the selection therefore closes with it.
- **Split on when deselect fires.** tldraw and Excalidraw deselect on pointer down (left drag there means box-select, and panning is a separate tool/key). xyflow decides on pointer up after the threshold, because left drag pans by default. A canvas where left drag pans (like plotkeeper's map) matches xyflow's model: deselect only if the pointer moved no more than the threshold.
- **Resizable docks: drag handle, min width, default ~300, persisted, double-click resets.** VS Code (min 170, default 300, 4 px sash, persisted per profile, double-click reset) and Zed (6 px handle, defaults 240-360, persisted, double-click reset) agree. Floating canvas inspectors (Excalidraw) are fixed / fit-content and not resizable.
- **Esc and close button**: not compared here from primary sources (unverified for these tools).

### Options for the grilling

1. **Cap and scroll (Excalidraw)**: `max-height: calc(100% - <top+bottom chrome>)` on the sheet with `overflow-y: auto`, measured against the map's box rather than the window. Smallest change that fixes the overflow.
2. **Click on empty map closes, drag still pans (xyflow)**: record the pointer-down point; on pointer up, if `dx*dx + dy*dy <= T*T` and the target is empty map, close the sheet. T = 1 px copies xyflow; 4 px copies tldraw (6 px for touch); 10 px copies Excalidraw. Keep × and Esc.
3. **Fixed width, fit content up to a cap (Excalidraw)**: keep 300 px or move to `width: fit-content; max-width: ...`. No handle, no stored state.
4. **Resizable with a drag handle (VS Code / Zed)**: 4-6 px handle on the sheet's inner edge, min ~170 px (VS Code), default ~300 px (VS Code/Zed), max capped to a share of the map width, width saved per user (localStorage in a web page), double-click on the handle resets to default (both).
5. **Dock instead of float (VS Code / Zed / Notion side peek)**: make the sheet a docked column beside the map that shrinks the map, so it never covers it and its height is the column's height with inner scroll; Notion keeps the left side interactive in the same way.

Baseline this research answers (from the brief, not re-measured): one catch-up = writer 30.2 s + claim check 19.9 s + retry 13.0 s = 63 s, three serial Opus calls, about $0.82; the retry ran in 7/7 writes; pre-generation covers 3 core parts; the user sees only "Writing the part catch-up..." until all three calls finish.

## 3. Slow part catch-up

### Products

**CodeRabbit (walkthrough + high-level summary)**

- Trigger: "CodeRabbit reviews a pull request automatically when it opens against the default branch." Incremental review is on by default (`auto_incremental_review: true`): "Each push to a reviewed PR gets a review of the newly added commits." It pauses after 5 reviewed commits by default (`auto_pause_after_reviewed_commits: 5`; `0` never pauses). Drafts are skipped by default (`drafts: false`). Source: https://docs.coderabbit.ai/configuration/auto-review
- Budget: automatic incremental reviews "count toward the same per-developer rate limit as manual reviews"; the page warns that `auto_pause_after_reviewed_commits: 0` can use a branch's allowance quickly. Source: https://docs.coderabbit.ai/configuration/auto-review
- The summary is pre-generated, not on demand: it is appended to the PR description "after a review" and "regenerated on every incremental push, so it always reflects the latest state of the branch." Source: https://docs.coderabbit.ai/pr-reviews/summaries
- Waiting state: "While a review is in progress, CodeRabbit posts a fortune message as a placeholder in the walkthrough comment", replaced when the review finishes; key `in_progress_fortune`, default `true`. No step-by-step streaming of the walkthrough. Source: https://docs.coderabbit.ai/pr-reviews/walkthroughs
- Generation time: not stated in the docs (unverified).
- Manual escape hatch: `@coderabbitai review` (incremental) and `@coderabbitai full review`. Source: https://docs.coderabbit.ai/guides/commands

**GitHub Copilot PR summaries**

- Fully on demand: the user clicks the Copilot icon in the description field and chooses "Summary". Source: https://docs.github.com/en/copilot/how-tos/use-copilot-for-common-tasks/create-a-pr-summary
- Not pre-generated and not refreshed: "When updates or changes are made to the pull request, the summary is not automatically refreshed." Source: https://docs.github.com/en/copilot/responsible-use/pull-request-summaries
- Input budget: "Files with more than 400 combined additions and deletions are excluded from summarization." Single pipeline: "a workflow uses the code diffs from files to build a prompt". No separate check pass is described. Source: same page.
- Output: "a prose paragraph giving an overview of the changes, followed by a bulleted list of key changes" linked to lines. Source: same page.
- Streaming or progress display: not described (unverified).

**Graphite (AI Reviews, formerly Diamond)**

- Runs on every PR without setup: AI reviews "analyze every pull request in your repositories ... instantly". No duration, trigger detail, progress indicator or PR-description feature is given on the docs page (unverified). Source: https://graphite.com/docs/ai-reviews
- Vendor claim: "sub-3% false-positive rate" (marketing, not a latency fact). Source: https://graphite.com/features/ai-reviews

**Cursor Bugbot**

- Pre-generates on every PR update by default; repo config `frequency: oncePerPr` instead of `everyPush`; manual trigger by commenting `cursor review` or `bugbot run`; drafts skipped by default. Source: https://cursor.com/docs/bugbot
- Writes a PR summary per review; "Post PR Summary" puts it in the description (default), in a comment that updates each review, or off. Source: same page.
- Budgets: skips PRs over a default size limit (changed lines and diff characters) and those skips "use no review usage"; files marked `linguist-generated` excluded; API limit 30 review triggers per minute per team; usage-based billing per review. Source: same page.
- Waiting state: only a final status check ("Cursor Bugbot" on GitHub); no progress indicator documented. Duration not given; "higher effort levels take longer". Source: same page.

**Greptile**

- Automatic on PR open; draft PRs skipped by default; manual re-run by commenting `@greptileai` or a re-trigger button. Source: https://www.greptile.com/docs/code-review/first-pr-review
- Duration: "~3 minutes" for the Analyzing status. Source: same page. (A size breakdown of 1-2 / 3 / 3-5 minutes appeared in a search snippet of a troubleshooting page that I did not open: unverified.)
- Waiting state: emoji reactions on the PR: eyes while analyzing, thumbs-up when done, confused face on failure. Source: same page.
- Summary contents: plain-language summary, a 0-5 confidence score, findings list, sequence diagrams only for multi-service or API-flow changes. Source: same page.

**Sourcegraph Amp**

- Uses a smaller, faster model for a secondary task: "We switched the model powering the `finder` subagent to Haiku 4.5", "an approximate 50% speedup", "with no noticeable loss in quality" (2025-10-21). Source: https://ampcode.com/news/faster-search-agent
- Later posts change this: a search result said subagents now run the main thread's model and that the finder later moved to Gemini 3 Flash. I did not open those pages, so the current setup is unverified. Source of the snippet: https://ampcode.com/chronicle, https://ampcode.com/docs/tools
- Sourcegraph Cody: not researched further (product superseded by Amp; unverified).

### Claude API facts

Current models and prices (per million tokens; Claude API first-party):

| Model | ID | Comparative latency | Input | Output | Cache hit | 5m write | 1h write |
|---|---|---|---|---|---|---|---|
| Claude Opus 5.5 | `claude-opus-5-5` | Moderate | $4 | $20 | $0.20 (0.05x) | $5 | $8 |
| Claude Sonnet 5.5 | `claude-sonnet-5-5` | Fast | $2 | $10 | $0.10 (0.05x) | $2.50 | $4 |
| Claude Haiku 5.5 (prompt up to 100K) | `claude-haiku-5-5` | Fastest | $0.10 | $0.50 | $0.01 | $0.125 | $0.20 |

Sources: https://platform.claude.com/docs/en/about-claude/pricing (prices), https://platform.claude.com/docs/en/about-claude/models/overview ("Comparative latency: Relative to the current lineup. Actual latency depends on prompt length, output length, and thinking effort."). The docs give no seconds or tokens-per-second figures per model.

Default effort: Opus 5.5 `medium`, Sonnet 5.5 `high`, Haiku 5.5 `medium`. Opus 5.5 thinking is "Adaptive (always on)". Source: https://platform.claude.com/docs/en/about-claude/models/overview

**Streaming**

- `"stream": true` returns server-sent events: `message_start`, then per block `content_block_start` / `content_block_delta` / `content_block_stop`, then `message_delta`, `message_stop`; `ping` events may appear anywhere; text arrives as `text_delta`, tool input as `input_json_delta`, thinking as `thinking_delta`. Source: https://platform.claude.com/docs/en/build-with-claude/streaming
- Tool-input deltas "are *partial JSON strings*"; by default "the API buffers each key and value in `input` until it is complete", so there can be pauses while a value is written. Source: same page.
- Thinking: with the default `display: "omitted"` on Opus 5.5, the thinking block streams with an empty `thinking` string and a signature, so a streamed reply can open with a silent gap. Source: same page; default per https://platform.claude.com/docs/en/build-with-claude/streaming and the claude-api skill (Opus 5.5 default `omitted`).
- Fine-grained tool streaming (GA, no beta header): set `eager_input_streaming: true` per tool; "Skipping the buffering step reduces the time to the first fragment of a large parameter"; "you might receive partial or invalid JSON". Without it "nothing prints for a large parameter until Claude has finished generating it." No latency numbers given. Source: https://platform.claude.com/docs/en/agents-and-tools/tool-use/fine-grained-tool-streaming
- Structured outputs (`output_config.format`, JSON schema, constrained decoding) "Stream structured outputs like normal responses"; the doc only shows accumulating the full JSON before parsing; it does not describe partial-JSON delivery. First use of a schema adds grammar-compile latency (no number); compiled grammars are "cached for 24 hours from last use"; changing `output_config.format` "invalidates any prompt cache for that conversation thread." Required properties are emitted first, in schema order. Source: https://platform.claude.com/docs/en/build-with-claude/structured-outputs
- Implication for a 3-4 step catch-up (inference, not a doc claim): with a schema like `{steps: [{title, body}]}` streamed as text, step 1's JSON is complete before step 2 starts, so a tolerant partial-JSON parser can show step 1 early.

**Prompt caching**

- Minimum cacheable prompt: 512 tokens on Opus 5.5, Sonnet 5.5, Haiku 5.5 (Fable/Mythos 5.x and Opus 5 too); 1,024 on Opus 4.8 / Sonnet 5; 4,096 on Haiku 4.5. Shorter prompts are "processed without caching, and no error is returned." Source: https://platform.claude.com/docs/en/build-with-claude/prompt-caching
- TTL: 5 minutes default, refreshed free on each hit; 1 hour optional. "The lifetime is measured from the start of the request". Source: same page.
- Multipliers: 5m write 1.25x, 1h write 2x, read 0.1x on most models but 0.05x on Opus 5.5 and Sonnet 5.5. Break-even: 5m after one read, 1h after two reads. Source: https://platform.claude.com/docs/en/about-claude/pricing
- Latency: no number. "You generally see improved time-to-first-token for long documents"; 5m and 1h caches "behave the same with respect to latency." Source: https://platform.claude.com/docs/en/build-with-claude/prompt-caching
- Concurrency: "a cache entry only becomes available after the first response starts." Source: same page.
- Pre-warming: `max_tokens: 0` writes the cache and returns no output, "Zero output tokens are billed"; rejected with `stream: true`, structured outputs, extended thinking, or batches. Source: same page.
- Automatic caching: one top-level `cache_control: {type: "ephemeral"}`. Source: same page.
- Relevance here (inference): the writer and the claim check probably share the same diff and part context. If that shared context is the first part of both prompts, the check and retry can read it from cache. Caching cuts cost more than it cuts time; the writer's 30 s is mostly output generation.

**Effort and model choice**

- Effort applies to "all tokens in the response", thinking included; `low` is "Most efficient ... Simpler tasks that need the best speed and lowest costs, such as subagents." Best practice: "When latency matters or tasks are straightforward, low effort can significantly reduce response times and costs." Source: https://platform.claude.com/docs/en/build-with-claude/effort
- Anthropic's measured numbers for Opus 5.5 `low` vs `medium`: "84% less time" (DRACO), "30% less" (HLE), "47% less" (physics), at scores 13.3, 3.4 and 14.0 points lower. SWE-bench Pro subset: Opus 5.5 `low` 87.4% vs Sonnet 5.5 default 87.6%, about the same per solved task. Source: https://platform.claude.com/docs/en/about-claude/models/optimizing-for-cost-and-intelligence
- Checker pattern: "Run at `low` effort and re-run failures at `high`: in Anthropic's tests, about 40% lower cost than everything at `high`, at the same pass rate", with the warning "a checker that passes bad work lets those failures through" and "every first-pass failure takes two runs' worth of wall-clock time." Source: same page.
- Haiku 5.5 on GPQA Diamond: 85% vs 91% for Opus 5.5 and Sonnet 5.5 at "About a twentieth of Claude Opus 5.5's cost per correct answer"; recommended for "high-volume and latency-sensitive work with checkable outputs." Source: same page.
- Reduce-latency page lists exactly three levers: choose the right model (Haiku 5.5 at `low` for speed-critical work), shorten prompt and output ("ask for paragraph or sentence count limits" rather than word counts; `max_tokens`), and stream. Source: https://platform.claude.com/docs/en/test-and-evaluate/strengthen-guardrails/reduce-latency
- Fast mode: Opus 5.5 "up to 2.5x higher output tokens per second" at $8 / $40 per MTok (2x); `speed: "fast"`; not with Batch API. Source: https://platform.claude.com/docs/en/about-claude/pricing

**Message Batches (for pre-generation)**

- 50% off input and output; "most batches finishing in less than 1 hour"; results available when all finish or after 24 hours; batches expire at 24 hours; results kept 29 days; up to 100,000 requests or 256 MB. Source: https://platform.claude.com/docs/en/build-with-claude/batch-processing
- Caching in batches is best-effort: "cache hit rates ranging from 30 percent to 98 percent"; the docs suggest the 1-hour TTL for shared context. `max_tokens: 0` pre-warming is not allowed in a batch. Source: same page.
- Opus 5.5 batch price: $2 / $10 per MTok. Source: https://platform.claude.com/docs/en/about-claude/pricing
- Fit (inference): a batch suits background pre-generation of every part's catch-up overnight or on a commit trigger. It does not suit a user who is waiting.

### Patterns the benchmarks agree on

- **Pre-generate on a code event, show on view.** CodeRabbit (each push), Cursor Bugbot (each PR update, or once per PR) and Greptile (PR open) all write the summary before anyone asks. Copilot is the one exception: on demand and never refreshed. Sources above.
- **Budget the pre-generation.** Every tool that pre-generates caps it: CodeRabbit pauses after 5 commits and counts auto reviews against a rate limit; Bugbot skips PRs over a size limit and generated files; Copilot drops files over 400 changed lines; all skip drafts by default.
- **A placeholder, not a stream.** None of the review bots stream their summary. They show an in-progress marker (CodeRabbit's fortune placeholder, Greptile's eyes emoji, Bugbot's pending check) and swap in the finished text. Streaming partial output is the chat-product pattern (Anthropic's reduce-latency page lists it as one of three levers).
- **Smaller models for secondary passes.** Amp moved its search subagent to Haiku (about 50% faster); Anthropic's own guidance recommends `low` effort or Haiku for checkable, latency-sensitive work, re-running only failures at higher effort.
- **Split:** on how fresh to keep it. CodeRabbit regenerates every push; Bugbot lets you choose every push or once per PR; Copilot never refreshes.

### Options for the grilling

1. **Pre-generate every part on a code event, within a budget** (copies CodeRabbit / Cursor Bugbot). Write catch-ups for every changed part after a commit lands, not only the 3 core parts. Cap it the way they do: a per-day count, skip parts below a change threshold, pause after N commits. Optional: send the background batch through Message Batches at half price, since "most batches finish in less than 1 hour".
2. **Placeholder with a stage label** (copies CodeRabbit's in-progress placeholder and Greptile's reactions). Replace the bare "Writing..." with stages the server already knows: "Writing" (call 1), "Checking claims" (call 2), "Fixing N claims" (retry). Cheapest change. It does not shorten the wait.
3. **Stream the writer and show steps as they finish** (copies chat products and Anthropic's "Stream responses" lever). Stream the writer call and parse partial JSON so step 1 appears in about a quarter of the writer's 30 s. Catch: the claim check runs after the writer, so a streamed step can later be corrected or withdrawn. Either label steps "unchecked" until the check passes, or check each step as it completes.
4. **Run the claim check on a cheaper, faster model** (copies Amp's Haiku finder; Anthropic's checker guidance). Run the check on `claude-sonnet-5-5` or `claude-haiku-5-5`, or on Opus at `low` effort, and measure it against the Opus check on saved catch-ups first. Anthropic warns that a weak checker lets bad work through.
5. **Remove the retry, not just speed it up** (no direct benchmark copy; follows from 7/7). A retry on every write means the writer prompt reliably produces what the check rejects. Fix the writer prompt (or feed the check's rules into it) so the retry becomes rare. This cuts about 13 s and a third of the cost.
6. **Lower the writer's effort, cache the shared context** (copies Anthropic's effort and caching guidance). Set `output_config.effort: "low"` on the writer and measure it (Opus 5.5 defaults to `medium`). Put the shared part/diff context first, with `cache_control`, so the check and retry read it from cache: 0.05x input price on Opus 5.5, and some time to first token saved. The 512-token minimum is easy to reach.
7. **Fast mode for the waited-on call only** (Anthropic fast mode). `speed: "fast"` on an on-demand write: up to 2.5x output speed at 2x price. Keep pre-generation at standard speed.

## 4. Inline emphasis in model output

The bug as briefed: the prompt allows `<b>...</b>` once; the UI sets the step text with `textContent`, so the tags show literally. `textContent` is the safe choice. The question is what format to ask the model for, so that emphasis renders and stays safe.

### Products and libraries

**What models are asked to emit**

- Anthropic's prompting guide treats Markdown as the model's default formatting and shows how to steer it. It suggests saying what to do instead of what not to do ("Your response should be composed of smoothly flowing prose paragraphs."), using XML format indicators, and matching the prompt's own style ("removing markdown from your prompt can reduce the volume of markdown in the output"). Its sample prompt keeps Markdown "primarily for `inline code`, code blocks ... and simple headings" and says "Avoid using **bold** and *italics*." Source: https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices
- Structured outputs (`output_config.format` JSON schema) guarantee the JSON shape through constrained decoding, but do not constrain what is inside a string. The fetched pages say nothing about HTML or Markdown inside string values. `enum`/`const` casing is not guaranteed either. Source: https://platform.claude.com/docs/en/build-with-claude/structured-outputs
- ChatGPT and claude.ai: both render Markdown in their chat UIs (unverified from a primary source in this pass). Anthropic's published claude.ai system prompt for Opus 5.5 contains no Markdown rule I could find, and those prompts "do not apply to the Claude API". Source: https://platform.claude.com/docs/en/release-notes/system-prompts/overview, https://platform.claude.com/docs/en/release-notes/system-prompts/claude-opus-5-5. OpenAI's Model Spec says that in programmatic settings the model should "output exactly what the preceding message has asked for, in the exact format specified"; I could not read its Markdown-default section (fetch failed): unverified. Source: https://model-spec.openai.com/2026-10-29.html

**Markdown renderers and how they stay safe**

- react-markdown: "secure by default"; it "typically escapes HTML (or ignores it, with `skipHtml`)"; `skipHtml` defaults to `false`; `rehype-raw` only "if you are in a trusted environment (you trust the markdown)"; `rehype-sanitize` for an allowlist schema; the default `urlTransform` allows `http`, `https`, `irc`, `ircs`, `mailto`, `xmpp` and relative URLs. Source: https://github.com/remarkjs/react-markdown
- marked: "Marked does not sanitize the output HTML." "Please use a sanitize library, like DOMPurify (recommended), sanitize-html or insane on the output HTML!" Example: `DOMPurify.sanitize(marked.parse(...))`. Source: https://github.com/markedjs/marked
- Streamdown (Vercel; "A drop-in replacement for react-markdown, designed for AI-powered streaming"). Source: https://github.com/vercel/streamdown
  - Defaults, from source (`packages/streamdown/index.tsx`, commit 0cd0c44, 2026-11-24): rehype plugins `raw: rehypeRaw`, `sanitize: [rehypeSanitize, defaultSanitizeSchema]`, and `harden` with `allowedImagePrefixes: ["*"]`, `allowedLinkPrefixes: ["*"]`, `allowedProtocols: ["*"]`, `allowDataImages: true`; `parseIncompleteMarkdown` defaults to `true`. So Streamdown parses raw HTML, then sanitizes it with a GitHub-style schema, and leaves links and images permissive. Source: https://github.com/vercel/streamdown/blob/main/packages/streamdown/index.tsx
  - Security docs: "By default, Streamdown is configured with **permissive security**"; "Without `rehype-raw`, HTML tags will be escaped and displayed as text." Source: https://streamdown.ai/docs/security
  - Unterminated markup is handled by `remend`, which "Auto-closes bold, italic, code, links, images, strikethrough, and math blocks": `**text` becomes `**text**`; an incomplete link `[text](url` becomes `[text](streamdown:incomplete-link)`. Source: https://github.com/vercel/streamdown/blob/main/packages/remend/README.md
  - The Vercel AI SDK's AI Elements `MessageResponse` renders model Markdown on Streamdown (`parseIncompleteMarkdown` default `true`, link/image prefixes default `["*"]`). Source: https://elements.ai-sdk.dev/components/message

**Sanitizers**

- DOMPurify "sanitizes HTML and prevents XSS attacks"; strict allowlist example: `DOMPurify.sanitize(dirty, { ALLOWED_TAGS: ['b'] })`; warning: "if you first sanitize HTML and then modify it afterwards, you might easily void the effects of sanitization." Source: https://github.com/cure53/DOMPurify
- HTML Sanitizer API: `Element.setHTML()` "provides an XSS-safe method to parse and sanitize a string of HTML"; it "removes any elements and attributes that are considered XSS-unsafe, even if allowed by a passed sanitizer"; allowlist via `new Sanitizer({ elements: [...] })`. Status: "Limited availability ... not Baseline because it does not work in some of the most widely-used browsers." Spec: WHATWG HTML, https://html.spec.whatwg.org/multipage/dynamic-markup-insertion.html#sanitizer. Sources: https://developer.mozilla.org/en-US/docs/Web/API/Element/setHTML, https://developer.mozilla.org/en-US/docs/Web/API/HTML_Sanitizer_API. (The project's modern-web-guidance tool, which would give exact per-browser support, was blocked by the permission system in this session, so per-browser support is unverified.)

**OWASP**

- LLM05:2025 Improper Output Handling: "insufficient validation, sanitization, and handling of the outputs"; the example given is "JavaScript or Markdown is generated by the LLM and returned to a user", which the browser then runs as XSS. Mitigations: treat the model as an untrusted user, "Implement context-aware output encoding based on where the LLM output will be used", and a strict Content Security Policy. Source: https://genai.owasp.org/llmrisk/llm052025-improper-output-handling/

**Structured spans (prior art)**

- Slack Block Kit `rich_text`: an array of elements; a `text` element carries a `style` object, e.g. `{"type": "text", "text": "I am a bold rich text block!", "style": {"bold": true}}`; `italic` and `strike` the same way. Sources: https://docs.slack.dev/reference/block-kit/blocks/rich-text-block, https://docs.slack.dev/reference/block-kit/block-elements/rich-text-section-element.md
- Notion rich text: an array of objects, each with `type`, a `text` object (`content`), an `annotations` object (`bold`, `italic`, `strikethrough`, `underline`, `code` booleans, `color`), `plain_text` ("The plain text without annotations") and optional `href`. Source: https://developers.notion.com/reference/rich-text
- In both, emphasis is data, never markup in a string. The renderer builds DOM nodes from it, so there is nothing to sanitize. Neither page says the format was designed for LLM output. It is prior art for the shape, not for model prompting.

### Patterns the benchmarks agree on

- **Model output is untrusted.** OWASP, marked, react-markdown and MDN all say not to put model or user HTML into `innerHTML` unless it has been through a sanitizer or a renderer that escapes HTML. plotkeeper's current `textContent` is the safe baseline. What is missing is a way to show emphasis.
- **Markdown is the common format for model text; raw HTML in it is escaped or sanitized.** react-markdown escapes raw HTML by default. Streamdown parses it, then runs `rehype-sanitize`. marked tells you to run DOMPurify on its output. Nobody passes model HTML through unchanged.
- **Split:** on raw HTML inside Markdown. react-markdown escapes it by default (it would show `<b>` literally, as plotkeeper does now). Streamdown parses and sanitizes it by default.
- **Streaming needs repair of unclosed markup.** Streamdown/remend auto-closes `**bold` mid-stream. This matters only if option 3 of issue 3 (streaming) is chosen.
- **Structured spans avoid the whole problem** (Slack, Notion): no parser, no sanitizer, no unclosed tags. The cost is a larger schema, and the model has to split text into spans.

### Options for the grilling

1. **Allowlisted HTML, sanitized** (copies marked + DOMPurify `ALLOWED_TAGS: ['b']`, or `setHTML` with `new Sanitizer({elements: ['b']})`). Keep the prompt's `<b>` and render it through a one-tag allowlist. Small change. Adds a dependency (DOMPurify), or relies on `setHTML`, which is "not Baseline", so it needs a fallback.
2. **Tiny Markdown subset, hand-rendered** (copies the Markdown convention of ChatGPT-style UIs, with react-markdown's escape-by-default stance). Ask for `**...**`. On the page, split on `**` pairs and build `<b>` nodes with `textContent` for every text run; treat any unpaired `**` as literal. No HTML parsing at all, so there is nothing to sanitize. Anthropic's guide says Claude defaults to Markdown, so this matches what the model does unprompted.
3. **Structured emphasis field** (copies Slack Block Kit `style` / Notion `annotations`). Change the step schema to carry emphasis as data: either `{"text": "...", "emphasis": "goes idle"}`, with the page wrapping the first match in `<b>` (fits "once"), or a span array `[{"text": "...", "bold": false}, ...]`. With `output_config.format` the shape is guaranteed by constrained decoding. The page builds nodes with `textContent`. This is the safest option and the most testable; the cost is a schema change, and the emphasis text has to appear verbatim in the body.
4. **Drop emphasis** (copies Anthropic's sample "Avoid using **bold** and *italics*" prompt). Remove the `<b>` allowance from the prompt and keep `textContent`. Zero code; the visual cue is lost.
5. **Full Markdown renderer** (copies Streamdown / react-markdown). Overkill for one bold span in a page with no framework dependency. Worth it only if catch-ups will grow lists, code or links. If chosen, keep raw HTML escaped (react-markdown's default), not Streamdown's permissive link and image defaults.

## 5. Empty space in catch-up cards: visuals

### Per-tool facts

- **CodeRabbit.** The walkthrough is "a structured overview of the changes that appears at the top of the PR comment thread", made of sections that can each be toggled: changed-files summary (related files grouped into rows with a plain-language description), sequence diagrams, estimated review effort (1-5), merge risk, related issues/PRs, suggested labels/reviewers, poem (off by default) ([walkthroughs](https://docs.coderabbit.ai/pr-reviews/walkthroughs)). Diagrams are **model-written Mermaid sequence diagrams**, generated only "for PRs affecting component interactions, such as API calls, event flows, or async workflows", rendered inline by GitHub/GitLab; PRs without such interactions get no diagram section at all ([walkthroughs](https://docs.coderabbit.ai/pr-reviews/walkthroughs)). Config: `reviews.sequence_diagrams` (default `true`), `reviews.collapse_walkthrough` (default `true`, wraps the walkthrough in a collapsible section), `reviews.poem` (default `false`) ([configuration reference](https://docs.coderabbit.ai/reference/configuration)). Layout lesson: diagram is its own section, omitted entirely when it doesn't fit; text never reserves room for it.
- **Greptile.** "Greptile automatically generates a diagram to visualize the changes in your PR"; type chosen by change: sequence (multi-service interactions, API flows), entity-relation (schema), class (hierarchy), flow (control flow / business logic); "For minimal or trivial changes, no diagram is generated"; user can ask for a type by replying or tagging `@greptileai` ([first PR review](https://www.greptile.com/docs/code-review/first-pr-review.md)). Config has a `sequenceDiagramSection` object with `included`, `collapsible`, `defaultOpen` beside `summarySection`, `issuesTableSection`, `confidenceScoreSection` ([greptile.json reference](https://greptile.com/docs/code-review-bot/greptile-json)). Mermaid as the output format: shown in the doc's example block but not stated (unverified). Marketing page says a diagram for "every PR" ([sequence-diagrams](https://www.greptile.com/sequence-diagrams)), which contradicts the docs; docs taken as authoritative.
- **GitHub (markdown) / Copilot.** GitHub markdown renders fenced `mermaid`, `geojson`, `topojson` and ASCII `stl` blocks in issues, discussions, PRs, wikis and Markdown files ([creating diagrams](https://docs.github.com/en/get-started/writing-on-github/working-with-advanced-formatting/creating-diagrams)). This is the substrate CodeRabbit and Greptile rely on: the model writes text, the host renders. Copilot's PR summary goes "in the description field or as a comment"; the doc says nothing about diagrams ([create a PR summary](https://docs.github.com/en/copilot/how-tos/use-copilot-for-common-tasks/create-a-pr-summary)). So Copilot PR summaries are text-only by documentation (diagrams in them: unverified/not documented).
- **DeepWiki (Cognition/Devin).** Docs say it "automatically generates architecture diagrams" and pages can be steered with `.devin/wiki.json` (pages with `title`, `purpose`, `parent`, `page_notes`; up to 30 pages) ([Devin docs: DeepWiki](https://docs.devin.ai/work-with-devin/deepwiki)). Neither the docs nor the launch post ([cognition.com/blog/deepwiki](https://cognition.com/blog/deepwiki)) say how diagrams are made; that they are LLM-written Mermaid is stated only by secondary sources (unverified). The open-source clone **deepwiki-open** (not Cognition's) is model-generated Mermaid by source: its wiki prompt says "EXTENSIVELY use Mermaid diagrams (e.g., `flowchart TD`, `sequenceDiagram`, `classDiagram`, `erDiagram`, `graph TD`)", "Ensure diagrams are accurate and directly derived from information in the [RELEVANT_SOURCE_FILES]", "Provide a brief explanation before or after each diagram", and forces vertical layout: "NEVER use \"graph LR\"", "Maximum node width should be 3-4 words" ([api/services/wiki/prompts.py](https://github.com/AsyncFuncAI/deepwiki-open/blob/main/api/services/wiki/prompts.py), lines 61-68 at time of reading). Note the vertical rule: diagrams flow down inside the text column instead of sitting in a side column.
- **Google Code Wiki.** Public preview announced 2025-11-13; "automatically generates always-current architecture, class, and sequence diagrams", regenerated as code changes ([Google Developers Blog](https://developers.googleblog.com/en/introducing-code-wiki-accelerating-your-code-understanding/)). Whether diagrams are model-written text or drawn from static analysis: not stated (unverified).
- **Swimm.** Diagrams are **human-written Mermaid** in the editor (`/mermaid` slash command or Mermaid Live Editor templates) ([release 0.9.7](https://docs.swimm.io/changelog/release-0.9.7), [diagrams and charts](https://docs.swimm.io/features/diagrams-and-charts)). Labels can hold "Smart Tokens" coupled to code identifiers so the diagram follows refactors, and auto-sync updates code snippets when linked code changes, asking the user when a token is removed ([Swimm blog: Mermaid integration](https://swimm.io/blog/create-up-to-date-diagrams-with-swimm-s-mermaid-integration), [auto-sync](https://swimm.io/blog/how-does-swimm-s-auto-sync-feature-work/)). Pattern: diagram text authored once, references to code kept live deterministically.
- **CodeSee.** Acquired by GitKraken, announced 2024-05-14; maps folded into GitKraken's DevEx platform ([GitKraken press release](https://gitkraken.com/press/gitkraken-acquires-codesee-launches-devex-platform), [GitKraken blog](https://www.gitkraken.com/blog/gitkraken-launches-devex-platform-acquires-codesee)). A standalone CodeSee shutdown ("closing its doors ... Thursday, Feb 22nd") appears only in a LinkedIn post (unverified). CodeSee maps were drawn deterministically from repo structure (from the product's own description; current docs gone, unverified).
- **Mintlify.** Fenced `mermaid` blocks render with zoom, pan and reset controls, shown by default only when the diagram is taller than 120px; `actions`, `placement`, `className` props; ELK renderer option for big flowcharts ([Mermaid diagrams](https://www.mintlify.com/docs/components/mermaid-diagrams)). Its `<Steps>`/`<Step>` component is a **single column**: numbered indicator, title, content under the title; any image or code sits inside the step's own content, so a text-only step is just shorter ([Steps](https://mintlify.com/docs/components/steps)).
- **Cursor.** IDE chat renders model-written Mermaid inline; Cursor 2.2 streams Mermaid diagrams into Plan Mode plans ([forum: Cursor 2.2](https://forum.cursor.com/t/cursor-2-2-plan-mode-improvements/145827)). CLI (2026-02-18) renders Mermaid blocks as inline ASCII diagrams, with Ctrl+O toggling rendered view and source ([CLI changelog](https://cursor.com/changelog/cli-feb-18-2026)). Forum reports show the costs of model-written diagrams: unsupported diagram types fail, syntax errors, diagrams disappearing ([timeline fails](https://forum.cursor.com/t/mermaid-timeline-diagrams-fail-to-render-in-cursor-chat/166749), [syntax error](https://forum.cursor.com/t/mermaid-syntax-error/106091), [agent doesn't see its Mermaid mistakes](https://forum.cursor.com/t/cursor-does-not-see-when-it-made-a-mistake-in-its-mermaid-syntax-in-chat/146292)).
- **Sourcegraph.** Not researched to primary-source depth; no documented diagram feature for change explanations found (unverified).

### Explainer-layout prior art (text-only steps)

- **Distill.** Text lives in `.l-body`; figures choose a width (`.l-middle`, `.l-page`, `.l-screen`) or a `.side` variant that floats right "so they don't fully interrupt the text"; `.l-gutter` holds asides. No column is reserved for figures: a text-only stretch is just body text ([Distill guide](https://distill.pub/guide/)).
- **Scrollytelling (Scrollama / The Pudding).** Side-by-side layout = one sticky graphic (`position: sticky`) beside a column of text steps; the graphic stays stuck for a run of steps and changes state per step, then unsticks ([Pudding: sticky](https://pudding.cool/process/scrollytelling-sticky/), [Scrollama README](https://cdn.jsdelivr.net/gh/russellsamora/scrollama@main/README.md)). The figure is shared by all steps, so no step has an empty figure slot; it only works when one picture can carry the whole story.
- **Apple DocC tutorials.** Primary doc fetch failed (swift.org page rendered empty); layout of text-only steps unverified.

### Patterns the benchmarks agree on

- **Diagram is optional and omitted, not reserved.** CodeRabbit (only for interaction changes), Greptile (none for trivial changes), Mintlify steps and Distill all drop the figure entirely when there is none; none keeps an empty figure column.
- **Model-written Mermaid is the norm for AI change explainers** (CodeRabbit documented, Cursor documented, deepwiki-open by source; Greptile/DeepWiki/Code Wiki likely but unverified). Rendering is deterministic, done by the host (GitHub, Mintlify, Cursor).
- **Diagram type follows the change.** Greptile maps change kind to diagram kind; CodeRabbit only does sequence diagrams and only for interaction changes.
- **Diagrams sit in the flow, vertical.** deepwiki-open forbids left-right graphs; Mintlify/Distill put figures in or across the text column; only scrollytelling uses a side column, and there one figure serves all steps.
- **Cost of model-written diagrams is broken syntax** (Cursor forum; deepwiki-open has Mermaid error correction per its notes, unverified). Swimm's answer is human-authored diagrams with code-coupled tokens.
- **Split:** deterministic drawing from data (CodeSee maps, unverified detail; Swimm's coupled tokens) vs model-written text (CodeRabbit, Cursor, deepwiki-open). No benchmark found that draws a per-change before/after figure deterministically.

### Options for the grilling

1. **Collapse the figure column when a step has none** (copies Mintlify Steps, Distill `.l-body`, CodeRabbit omitting the section): the card is one column; a figure, when present, goes under or beside the text only for that card. Removes the empty space without new generation.
2. **Size the panel to the current card, not the tallest** (copies Mintlify/Distill flowing layout): accept height changes between steps instead of min-height = tallest step.
3. **One shared sticky figure for the whole catch-up** (copies Scrollama/Pudding scrollytelling): e.g. the part's slice of the map, with each step highlighting files or edges it talks about. Deterministic from data plotkeeper already has; every step has a visual, none is empty.
4. **Model-written Mermaid, chosen by change kind** (copies Greptile's type mapping and CodeRabbit's sequence diagrams): the prompt may emit a small sequence/flow diagram; render client-side; drop it on parse failure. Risk: broken syntax (Cursor forum).
5. **Model picks, code draws** (copies Swimm's code-coupled tokens): the model names files/symbols and a diagram kind from a fixed set (before/after, sequence), code draws it from the stored data, so labels are always real.
6. **Keep text-only, shrink the card** (copies Copilot PR summary, text-only by documentation): drop the figure slot from the design and narrow the panel.

## 6. End-of-catch-up feedback

Plotkeeper's current bar for comparison: 1 full-width text input (say it back) + 5 number buttons + 1 note input + Skip = 8 controls, all visible at once, at the end of the story.

### Per-tool facts

- **ChatGPT (OpenAI).** Thumbs-down sits under each message (web, iOS, Android); clicking it leads to "Select an issue", a list of issue options (one is "Safety or Legal concern"), then on-screen prompts ([report content](https://help.openai.com/en/articles/10245791)). The full option list and whether free text appears are not documented (unverified). Submitting thumbs feedback may send the whole conversation for training ([data controls FAQ](https://help.openai.com/en/articles/7730893-data-controls-faq)). OpenAI deprecated its separate model-behaviour web form in favour of the in-message thumbs-down ([form](https://openai.com/form/model-behavior-feedback)).
- **Claude.ai (Anthropic).** "You can use the thumbs down button to let us know if a particular response was unhelpful", otherwise email support ([incorrect responses](https://support.claude.com/en/articles/8525154-claude-is-providing-incorrect-or-misleading-responses-what-s-going-on)). Thumbs feedback stores the whole conversation for up to 5 years ([model training](https://support.claude.com/en/articles/7996868-is-my-data-used-for-model-training)); Team/Enterprise owners can turn thumbs feedback off ([feedback settings](https://support.claude.com/en/articles/10504853-manage-user-feedback-settings-on-claude-console)). What appears after the click (a text box then "Submit") is shown only by a third-party walkthrough (unverified) ([Guideflow](https://www.guideflow.com/tutorial/how-to-give-negative-feedback-on-a-response-in-claudeai)).
- **Gemini Apps (Google).** Steps: click "Good response" or "Bad response" under the response; then **optionally** select a reason (e.g. "Offensive / Unsafe"), enter written feedback, and/or attach uploads; "If you add written feedback, click Submit" ([send feedback](https://support.google.com/gemini/answer/13275746)). So the one click is the rating; reason and text are optional follow-ups, and Submit is only needed if you wrote something.
- **Microsoft 365 Copilot.** After thumbs up or down "you can provide more information for what you liked" / "for what went wrong"; "no data is collected other than your initial thumbs-up or thumbs-down choice" until you submit the follow-up; it may ask permission to attach prompt/response/logs, with previews ([providing feedback](https://support.microsoft.com/privacy/microsoft-copilot/providing-feedback)). Same page's own article-feedback widget: "What affected your experience?" checkboxes, then optional "Any additional feedback?" and "Submit feedback" (same URL).
- **GitHub Copilot (VS Code).** Thumbs up/down appear on hover over a chat response (Microsoft Learn training module, not docs.github.com) ([MS Learn](https://learn.microsoft.com/en-us/training/modules/github-copilot-across-environments/3-git-hub-copilot-chat)). Follow-up flow: not documented (unverified).
- **GitLab Duo (public design issue).** Phase 1 is deliberately binary: "Simple thumbs up/down on each Duo response", stored as `feedback_type` + timestamp only, in "an AI action bar at the bottom of the most recent response" next to Copy, styled like the other action buttons; selected up turns green, selected down orange/red; no follow-up reasons in this phase ([gitlab#578583](https://gitlab.com/gitlab-org/gitlab/-/issues/578583)).
- **AWS Cloudscape design system (GenAI patterns).** Thumbs up / thumbs down placed together as inline actions; on thumbs-down an **optional** additional-feedback dialog; "Let users dismiss the additional feedback dialogue and proceed"; "Don't ask extensive followup questions in the additional feedback dialogue"; after submit, disable the buttons with a reason that thanks the user and confirms which feedback was sent; the general user-feedback pattern is explicitly not for GenAI responses ([GenAI feedback](https://cloudscape.design/patterns/genai/feedback), [GenAI chat](https://cloudscape.design/patterns/genai/generative-AI-chat/), [collect user feedback](https://cloudscape.design/patterns/general/collect-user-feedback/), [inline actions](https://cloudscape.design/patterns/genai/inline-actions)). Quotes taken from search-indexed text of these pages; direct fetch returned 404 at time of reading.
- **Apple HIG, Ratings and reviews.** Ask "only after people have demonstrated engagement", e.g. after completing a level or significant task; "Avoid interrupting people while they're performing a task"; look for "natural breaks or stopping points"; "Avoid pestering people ... at least a week or two between requests"; the system prompt asks for a rating and an *optional* written review, and "People can supply feedback or dismiss the prompt with a single tap or click"; limited to three showings per app per 365 days ([HIG: ratings and reviews](https://developer.apple.com/design/human-interface-guidelines/ratings-and-reviews)). Relevance: end of a catch-up is the right moment; asking every time is "pestering".
- **Intercom surveys.** Question types include NPS (0-10), numeric scale (custom start/end/labels), 5-star rating, emoji rating (3- or 5-emoji, custom labels), text and choice; branching only on closed-ended questions ([survey types](https://intercom.com/help/en/articles/5973834-intercom-s-survey-types)). Exact "small format" limits differ between cached versions (unverified).
- **Typeform Opinion Scale.** Default 11 steps (0-10), minimum 5 steps (1-5); optional left/right end labels only, no mid-scale labels; for "not applicable", make the question optional or use a choice question ([Opinion Scale](https://help.typeform.com/hc/en-us/articles/360052363932)).
- **Duolingo.** Documented feedback is per exercise, not end-of-lesson: a "Report" flag next to the solution after checking an answer, opening a menu of error types that depends on exercise type and on whether the answer was graded right or wrong ([Duolingo blog: user reports](https://blog.duolingo.com/how-user-reports-improve-course-content)). End-of-lesson rating: none found (unverified).
- **Brainscape (confidence ratings in a learning app).** After flipping a card the learner rates "How well did you know this?" 1 (Not at all) to 5 (Perfectly); low ratings bring the card back sooner ([confidence-based repetition](https://www.brainscape.com/academy/confidence-based-repetition-definition), [Brainscape research](https://brainscape.com/research)). The 1-5 rating there is about the learner's own recall and drives scheduling, not about the material's quality.
- **Khan Academy / Khanmigo.** No documented "how confident are you" check-in found; mastery levels (Familiar / Proficient / Mastered) come from answers, and the Readiness Check states it "doesn't affect your mastery" and is not about a score ([mastery system](https://support.khanacademy.org/hc/en-us/sections/18411186533517-Understanding-the-Khan-Academy-Mastery-system)). Confidence prompt: unverified/not found.
- **Brilliant.** Treats explaining why a method works as a sign of understanding ([how Brilliant teaches math](https://brilliant.org/resources/choosing-brilliant/how-brilliant-teaches-math/)); no documented end-of-lesson rating or explain-back control found (unverified).
- **Linear, Notion.** No primary-source documentation of their in-product feedback widgets found; Linear routes feedback through its support page ([Linear support](https://linear.app/contact/support)); Notion AI thumbs described only by a third party (unverified) ([eesel](https://www.eesel.ai/blog/notion-ai-feedback-mechanism)).
- **Self-explanation research.** Chi, de Leeuw, Chiu, LaVancher (1994), "Eliciting self-explanations improves understanding", *Cognitive Science* 18:439-477: students prompted to explain each sentence of a text vs a control group reading it twice for equal time; the prompted group learned more ([ASU listing](https://education.asu.edu/lcl/publications/chi-m-t-h-de-leeuw-n-chiu-m-h-lavancher-c-1994-eliciting-self-explanations), [PDF](https://pcl.sitehost.iu.edu/rgoldsto/courses/cogscilearning/chiselfexplanation.pdf); effect sizes not re-read here). Note the prompts were *during* reading, per sentence, not one line at the end.

### UI facts at a glance

| Product | Controls shown first | Follow-up | Follow-up only after negative? | Placement | Dismiss |
|---|---|---|---|---|---|
| ChatGPT | thumbs up/down (2) | issue list, prompts | documented for thumbs-down | inline under message | unverified |
| Claude.ai | thumbs up/down (2) | text + Submit (unverified) | unverified | inline under message | unverified |
| Gemini | Good/Bad (2) | optional reason, text, files | no, both | inline under response | optional, Submit only if text |
| M365 Copilot | thumbs (2) | "more information" pane | no, both | inline | yes, nothing sent beyond thumb |
| GitLab Duo (ph.1) | thumbs (2) | none | n/a | action bar, last response | n/a |
| Cloudscape | thumbs (2) | optional dialog | yes, thumbs-down | inline actions | must be dismissible |
| Apple ratings | 5 stars | optional review | no | system prompt at a natural break | one tap |
| Brainscape | 1-5 confidence | none | n/a | after card flip | no (required to advance) |
| Duolingo | Report flag | error-type menu | user-initiated | next to solution | yes |

### Patterns the benchmarks agree on

- **Two controls first.** Every AI assistant documented (ChatGPT, Claude.ai, Gemini, M365 Copilot, GitHub Copilot, GitLab Duo, Cloudscape guidance) leads with a binary thumbs pair, inline next to the content, not a 1-5 scale. 1-5 scales show up in surveys (Intercom, Typeform minimum 1-5) and in learning apps where the number rates the learner's own recall (Brainscape).
- **The click is the feedback; everything else is optional.** Gemini (reason/text optional, Submit only if you wrote), M365 Copilot (nothing beyond the thumb is collected unless you submit), Cloudscape (optional, dismissible, short), Apple (optional review, one-tap dismiss).
- **Progressive disclosure.** Reason/text appears only after a click (all AI assistants); Cloudscape scopes it to thumbs-down; Gemini and M365 Copilot open it for both.
- **Reasons as chips/checkboxes before free text.** ChatGPT "Select an issue", Gemini "select a reason", M365's "What affected your experience?" checkboxes, Duolingo's error-type menu.
- **Confirm in place.** Cloudscape disables the buttons with a thank-you state; GitLab Duo keeps the selected thumb coloured.
- **Ask at a natural break, not every time** (Apple HIG: after a completed task, rate-limited).
- **Split:** no benchmark pairs a "say it back" free-text recall prompt with a quality rating in one bar. Explain-back exists in research (Chi et al., prompted during reading) and in Brilliant's stated philosophy, not as a documented end-of-lesson control in any product found. Confidence ratings (Brainscape) rate the learner, while thumbs rate the content; plotkeeper's "How clear was this?" rates the content with a learner-style 1-5 scale.

### Options for the grilling

1. **Thumbs pair inline under the last card, nothing else visible** (copies GitLab Duo phase 1, ChatGPT/Claude.ai placement): 2 controls replace 8; selected state stays coloured as confirmation.
2. **Thumbs, then an optional one-line reason only on thumbs-down** (copies Cloudscape): dismissible, short, no extra page; positive click ends it.
3. **Thumbs, then reason chips with optional text for either answer** (copies Gemini, M365 Copilot): chips such as "too vague", "wrong part", "too long"; Submit only appears if text is typed.
4. **Keep the say-it-back line but move it out of the feedback bar** (copies Chi et al.'s prompted self-explanation, per step): an optional "in your words?" on one card mid-story, separate from rating the story.
5. **Ask only sometimes** (copies Apple HIG rate-limiting): show the rating on a fraction of catch-ups or once per session, never on every one.
6. **Replace "How clear was this?" 1-5 with a recall-confidence rating that drives something** (copies Brainscape): only if the answer changes what plotkeeper shows next (e.g. re-show this part's catch-up sooner); otherwise drop the scale.
7. **Report flag on a specific card** (copies Duolingo's per-exercise Report): a small flag per step with an error-type menu, so "unclear" points at the step instead of a note about the whole story.
