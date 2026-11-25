# Onboarding, what's new and tips

> **Answer:** Developer tools teach mostly in context, not with up-front tours. Claude Code rotates one-line tips while the user waits, picking the tip unshown longest, with per-tip cooldowns, caps and an off switch. VS Code opens a short checklist walkthrough once, on install, and shows release notes once after an update. Zed shows one "Updated to …" toast per update. CodeBoarding has a four-step tour that the user can replay, plus a hold-H shortcut sheet. GitHub's Primer sets the rules for announcements: cap how often a message shows, give the campaign an end date, show one teaching bubble at a time, put it where the feature lives, and offer a clear dismiss. NN/g's research agrees: deck-of-cards tutorials did not improve task success, and readers rated the tasks harder (70 participants). Help that the user pulls up, in context, works better than help pushed at them.
> **Evidence:** Strong for Claude Code, VS Code, Zed and CodeBoarding: official docs, the changelog and source code, read directly. Strong for Primer, an official design system. Moderate for NN/g: one small quantitative study, the rest expert guidance. Thin for Linear, Raycast, Warp and Cursor, whose first-party docs say little about in-product onboarding. The tour completion numbers come from a vendor and are low trust. Nothing was run in plotkeeper.

Research date: 2026-11-25. plotkeeper's code was read on `main` at `534940a2b`. Studio terms follow `docs/studio/CONTEXT.md`. This note does not repeat `codeboarding.md`. It adds only CodeBoarding's onboarding and help facts.

## 1. Claude Code

| What | When it shows | How it is limited or dismissed | Source |
|---|---|---|---|
| **Spinner tips**: one line under the spinner, such as "Use Plan Mode to prepare for a complex request…" | While Claude works, so only during a wait | `spinnerTipsEnabled: false` ("Show tips" in `/config`) hides all tips. Default `true` | [settings reference](https://code.claude.com/docs/en/settings-reference#spinnertipsenabled), [settings](https://code.claude.com/docs/en/settings) |
| **How the rotation picks a tip** | Each tip has a show history, keyed on its `id`. `cooldownSessions` (0–1000) is how many sessions pass before a tip can show again. `priority` (−10 to 10) breaks ties among tips "that have gone unshown equally long", so the tip unshown longest comes first | At most 200 custom tips. A bad entry is dropped with a debug warning, so it never breaks the settings file | [`spinnerTipsOverride`](https://code.claude.com/docs/en/settings-reference#spinnertipsoverride) (v2.1.247+) |
| **Custom tips**: `spinnerTipsOverride.tips`, `tipsFile`, `label` (prefix, default `Tip`), `excludeDefault` | Shown in the same rotation as the built-in tips | If no custom tip loads, the built-in tips stay rather than leaving an empty spinner | same; [CHANGELOG 2.1.45, 2.1.247](https://github.com/anthropics/claude-code/blob/main/CHANGELOG.md) |
| **Tips that depend on context** | A notebook tip only "once Claude has read or edited a Jupyter notebook" (2.1.325). An Explanatory output style tip "when you ask Claude for explanations". A tip on Bash permission prompts that points to auto mode | Tips are hidden when their feature is off or not available to the account (2.1.271: fixed "spinner tips suggesting commands that aren't available for your account type or are disabled"). The frontend-design tip is "capped at 3 lifetime impressions instead of repeating indefinitely" (2.1.217) | [CHANGELOG](https://github.com/anthropics/claude-code/blob/main/CHANGELOG.md) |
| **Tips get pruned** | — | Several tips were removed: "the spinner tip that suggested `/clear` 30 minutes into a running turn" (2.1.326), the custom-subagents startup tip (2.1.232), and a dynamic-workflows tip (2.1.282) | same |
| **Where the tip history is stored** | — | **Unverified.** The docs say only that history is "keyed" on the id. Community posts name a key in `~/.claude.json`. That file holds credentials, so it was not read | — |
| **`/powerup`** | On demand | "quick interactive lessons with animated demos" (added 2.1.90). It once had a "nudge" for subagents, later removed (2.1.232) | [commands](https://code.claude.com/docs/en/commands), CHANGELOG |
| **`/help`** | On demand. `/help <command>` added in 2.1.326 | A tabbed list of commands, including your custom commands | [commands](https://code.claude.com/docs/en/commands), [quickstart](https://code.claude.com/docs/en/quickstart) |
| **Release notes** | A "release-notes splash" after an upgrade (2.1.121 removed its Recent Activity panel "for faster startup"). `/release-notes` is an interactive version picker (2.1.92) | The notes "appear in your transcript without entering the conversation Claude sees" (fixed in 2.1.208, after "Show all" put the whole changelog into context). `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC` turns off release-notes fetching. A cached copy is kept in `cache/changelog.md` | [commands](https://code.claude.com/docs/en/commands), [env vars](https://code.claude.com/docs/en/env-vars), [claude directory](https://code.claude.com/docs/en/claude-directory), CHANGELOG |
| **First run** | Login prompts, then the prompt with the version, model and folder above it: "Type `/help` for available commands" | No tour in the terminal. The VS Code extension has an "onboarding checklist" (named in the CHANGELOG, not documented) | [quickstart](https://code.claude.com/docs/en/quickstart), CHANGELOG |
| **`companyAnnouncements`** | At startup, one entry picked at random per session. On "a person's very first launch it shows the first entry" | Off by default | [settings reference](https://code.claude.com/docs/en/settings-reference#companyannouncements) |
| **Quiet notices** | "model auto-updates and the team-onboarding tip now show as quiet notices under the logo". Two startup messages were removed (2.1.162) | — | CHANGELOG |

**Takeaway.** Tips fill the wait. There is one at a time, each with a cooldown and history, gated on context and on the feature being available. A tip that annoys people is capped or removed. The verbs `/powerup` and `/release-notes` let the user pull help up whenever they want it.

## 2. CodeBoarding

`codeboarding.md:49` already records that the demo map shows "Take a tour" and "Press H for help". Its idea 2 (`codeboarding.md:208`) covers the "How it's decided" explainers. What they actually do was read from the public demo's JavaScript (`app.codeboarding.org`, chunk `6465-fa69802d66f7510f.js`, fetched 2026-11-25) and from the extension's Marketplace page.

| What | When it shows | How it is limited or dismissed | Source |
|---|---|---|---|
| **Tour: four steps, each pointing at a real element** (`data-tour-target`): "Your codebase as a map" (each box is a component, lines are calls, + shows the parts inside); "The details panel"; "Zoom and fit"; "That is the tour" ("Hold H for every shortcut. You can replay this tour from here any time.") | Started by `requestTour()` from the "Take a tour" button. The app can also pass a `startTour` prop, but what sets it was not found (**unverified**). The VS Code extension runs a "Guided Onboarding Tour … the first time they open a diagram" (0.10.4) | A step counter "N of M". The overlay is `aria-live="polite"` and honours `motion-reduce`. When the tour ends, focus goes back to where it was. It records `completed`, `step` and `abandoned` (with the step index and a reason) | demo bundle (above); [Marketplace](https://marketplace.visualstudio.com/items?itemName=Codeboarding.codeboarding) |
| **Hold-H help**: a small card listing the toolbar (zoom, fit) and diagram gestures (scroll or pinch, double-click to expand, Ctrl+Click to multi-select, "Click a status badge to mark the component viewed") | While H is held down, outside text fields. It shows on keydown and hides on keyup or when the window loses focus | Hidden during the tour. The "Take a tour" and "Press H for help" chips show only while help is not active | demo bundle |
| **Disabled controls that explain themselves** | With no workspace open | Analysis controls stay disabled "with a helpful tooltip until a workspace is available", next to a one-click folder picker (0.13.x) | Marketplace |
| **Generated "on_boarding.md" docs** | For each repository | — | [awesome-architecture-mds](https://github.com/CodeBoarding/awesome-architecture-mds), formerly GeneratedOnBoardings, [README](https://github.com/CodeBoarding/CodeBoarding) at `b5341f2e` |

**Takeaway.** A short tour anchored on real elements, that can be replayed, with a key that shows a reference card while held. The cost is small and it gets out of the way.

## 3. VS Code

| What | When it shows | How it is limited or dismissed | Source |
|---|---|---|---|
| **Walkthroughs** (`contributes.walkthroughs`): a title and steps. Each step has Markdown, an image or Markdown media, and `when` clauses | "automatically opened on install of your extension". Shown on the Getting Started page | A step checks off when its button is clicked, or when it is opened if it has no button. `completionEvents` can instead tick it on `onCommand`, `onSettingChanged`, `onContext`, `extensionInstalled`, `onView` or `onLink`. A ticked step stays ticked until "Getting Started: Reset Progress" | [contribution points](https://github.com/microsoft/vscode-docs/blob/main/api/references/contribution-points.md#contributeswalkthroughs) |
| **Walkthrough UX rules** | — | Don't "add an excessive number of steps", and don't "add multiple walkthroughs unless absolutely necessary". Give every step an action, phrased as a verb. Use SVGs drawn in theme colours | [UX guidelines](https://code.visualstudio.com/api/ux-guidelines/walkthroughs) |
| **Welcome page** | `workbench.startupEditor`, default `welcomePage`, "if none are restored from the previous session". Other values: `none`, `readme`, `newUntitledFile`, `welcomePageInEmptyWorkbench`, `terminal` | The user changes the setting | [source](https://github.com/microsoft/vscode/blob/main/src/vs/workbench/contrib/welcomeGettingStarted/browser/gettingStarted.contribution.ts) |
| **Release notes after an update** | `update.showReleaseNotes`, default `true`: "Show Release Notes after an update" | Turn off the setting. Notes come from an online service | [source](https://github.com/microsoft/vscode/blob/main/src/vs/platform/update/common/update.config.contribution.ts) |
| **Empty-editor watermark**: "Show All Commands", "Go to File", "Open Chat", each with its live keybinding | When no editor is open | `workbench.tips.enabled` ("show the watermark tips when no editor is open"), default `true` | [watermark source](https://github.com/microsoft/vscode/blob/main/src/vs/workbench/browser/parts/editor/editorGroupWatermark.ts), [setting source](https://github.com/microsoft/vscode/blob/main/src/vs/workbench/browser/workbench.contribution.ts) |
| **Tips and Tricks** | A docs page. An in-app command for it was not verified | — | **unverified** |

**Takeaway.** Walkthroughs are a checklist the user keeps, not a modal tour. Steps tick off when the user actually does the thing. The empty state teaches the two or three verbs that matter, using the user's real keys.

## 4. Zed

| What | When it shows | How it is limited or dismissed | Source |
|---|---|---|---|
| **Update toast**: "Updated to Zed X.Y.Z" with "View Release Notes", which opens the notes locally in a Markdown buffer | Once, on the first launch after an auto-installed update (`should_show_update_notification`, then cleared). Never on Nightly | Shown once. A fallback notice links to the notes online when they can't load locally | [`auto_update_ui.rs`](https://github.com/zed-industries/zed/blob/main/crates/auto_update_ui/src/auto_update_ui.rs) (main at `77228009`) |
| **Announcement toast for a version** (currently "Introducing Delta"): a heading, three bullets, and two actions | Replaces the plain update toast for versions ≥ 1.22.0 | Never shown again once dismissed (a `Dismissable` key). Skipped when AI is turned off | same |
| **Welcome page** | Only when nothing else is open | — | [docs](https://zed.dev/docs/getting-started), via `home.md` |

## 5. GitHub, Linear, Raycast, Warp, Cursor

| Tool | What | Limits | Source |
|---|---|---|---|
| **GitHub (Primer)** | Feature onboarding patterns: teaching bubbles, page banners, inline banners, empty states, a dashboard bulletin, a feature-preview dialog, lifecycle labels | "Specify a maximum number of impressions". "Timebox … a coordinated campaign". "Do not leave feature announcement campaigns running forever". "Show one teaching bubble at a time", about 160 characters. "Don't point to hidden elements". Use a descriptive dismiss button rather than an X. "No more than 2 alerts at a time". Put it "close to where the feature will live in perpetuity". Labels are "a softer way to accentuate new features" | [Primer: Feature onboarding](https://primer.style/product/ui-patterns/feature-onboarding/) |
| **GitHub Feature preview** | Profile menu → Feature preview → Enable/Disable for each preview | Primer: no feature stays in preview "longer than 2 months unmonitored". No "new" dot is documented | [docs](https://docs.github.com/en/get-started/using-github/exploring-early-access-releases-with-feature-preview) |
| **Linear** | The `?` key opens a help sidebar with search, guide articles pre-filled for the current view, and a link to the changelog | No in-app "what's new" popup was found | [changelog 2022-03-15](https://linear.app/changelog/2022-03-15-new-desktop-app) |
| **Raycast** | A "Walkthrough command" of onboarding tasks with tracked progress (2021). A few onboarding steps on install (Windows) | — | [changelog 1.26](https://www.raycast.com/changelog/1-26-0), [manual](https://manual.raycast.com/) (being rebuilt, thin) |
| **Warp, Cursor** | Docs guides and a weekly changelog. Cursor added an in-app update notification | No first-party docs on in-product onboarding were found | [Warp](https://docs.warp.dev/guides/getting-started/welcome-to-warp/), [Cursor changelog](https://cursor.com/changelog) — thin |

## 6. UX research and guidance

| Claim | Source | Trust |
|---|---|---|
| Deck-of-cards tutorials did not raise success (91% for readers vs 94% for skippers, not significant) or speed. Readers rated the tasks harder (4.92 vs 5.49, p=0.047). Remote test, 70 participants, 4 iOS apps | [NN/g, Kendrick 2020](https://www.nngroup.com/articles/mobile-tutorials/) | Moderate: one small study, on mobile |
| "Avoid creating app onboarding whenever possible and instead spend your resources making the UI more usable". Promote features in context, or to existing users when the features are new | [NN/g, Kendrick 2020](https://www.nngroup.com/articles/mobile-app-onboarding/) | Expert guidance |
| Push revelations are "devoid of context, and intrusive". Pull revelations are triggered by a signal that help is useful now. Make help easy to dismiss and to call back up, such as Evernote's always-available "What's new". Tutorials are fine for a "novel interaction paradigm" | [NN/g, Laubheimer 2023](https://www.nngroup.com/articles/onboarding-tutorials/) | Expert guidance |
| Paradox of the active user: people skip docs and start on their task, though learning first would save them time. "Design for the way users actually behave" | [NN/g, Nielsen 1998](https://www.nngroup.com/articles/paradox-of-the-active-user/), after Carroll & Rosson 1987 | Classic, well replicated in the field |
| Progressive disclosure: put frequent items first and defer the rest. Usually no more than two levels | [NN/g, Nielsen 2006](https://www.nngroup.com/articles/progressive-disclosure/) | Expert guidance |
| Empty states should say the status, give learning cues (pull revelations), and offer a direct way to the key task | [NN/g, Kaplan 2021](https://www.nngroup.com/articles/empty-state-interface-design/) | Expert guidance |
| Coach marks: one interaction per hint. Chained hints get dismissed faster. Hints should look distinct from controls | [NN/g, Harley 2014](https://www.nngroup.com/articles/mobile-instructional-overlay/) | Expert guidance, older |
| Tooltips: never essential information. Brief, shown on keyboard focus as well as hover, used consistently | [NN/g, Kendrick 2019](https://www.nngroup.com/articles/tooltip-guidelines/) | Expert guidance |
| Tours: 61% average completion. Self-started tours complete 123% more often, and checklist-started ones 21% more. Completion falls with each step after about 3–4. Later editions give averages of 30–34% | [Chameleon](https://www.chameleon.io/blog/product-tour-benchmarks-highlights) | **Low**: vendor data, undated, numbers shift between editions |

**Accessibility.**
- **WAI-ARIA APG tooltip:** `role="tooltip"`, referenced by `aria-describedby`. Escape dismisses it. It never takes focus. The pattern "does not yet have task force consensus". Hover content that can be focused should be a non-modal dialog ([APG](https://www.w3.org/WAI/ARIA/apg/patterns/tooltip/)).
- **WCAG 1.4.13 (AA):** hover and focus content must be dismissible without moving the pointer or focus, hoverable, and persistent ([Understanding 1.4.13](https://www.w3.org/WAI/WCAG22/Understanding/content-on-hover-or-focus.html)).
- **`popover="hint"`:** it does not close `auto` popovers, light-dismisses, and answers close requests ([MDN](https://github.com/mdn/content/blob/main/files/en-us/web/html/reference/global_attributes/popover/index.md)).
- **Interest invokers** (`interestfor`, `interest-delay`): show on hover, focus or long-press, and Esc cancels. Chrome 142+ only, marked experimental in browser-compat-data ([MDN guide](https://github.com/mdn/content/blob/main/files/en-us/web/api/popover_api/using_interest_invokers/index.md)). plotkeeper runs in Daniel's own browser, so its support may not matter (inference).

## 7. What plotkeeper already has

- **A command palette.** It lists every action with its live keys (`ui/keys.ts:105-120`, danielrispler/Falinks#575).
- **Button titles with their keys,** such as "Settings (⌘,)" (`ui/keys.ts:91-96`).
- **Shared hover texts** (`ui/hovers.ts`, #220).
- **Toast and notice modules** (`ui/toast.ts`, `ui/notice.ts`).
- **A one-time ask.** It is stored in `localStorage` (`notifications-asked`, `ui/alerts.ts:29-30`).
- **A version, and since #512 a changelog.** There is a package version (`server/Cargo.toml:4`, 0.1.2). At research time there was no changelog; #512 has since added a root `CHANGELOG.md` and a GitHub Release per published version (on `main` at `429d9ee6c`). Merged commits carry an issue number in their title (`git log`).
- **#477, "Marks explain themselves"** (open spec, part A). Clicking a reader mark gives "the facts of this case behind it" (user story 6). Per-case facts sit on part sheets. #477 also adds "Partial facts · N" and a stale-base line. So the "explain this mark" kind of contextual help is already specified for the PR brief reader. The options below don't repeat it.

## Patterns worth weighing for plotkeeper

These are options, not decisions. Each needs `$grilling`.

1. **Wait tips, in the style of Claude Code's spinner tips.** One line in the rail while a session is Running. Each tip has an id, a cooldown and a show history, is gated on context ("you have a map but never opened Ratify"), and has a setting to turn tips off.
   - *For:* it uses the Wait, which is already the waiting room (`home.md`), and is the most proven pattern here.
   - *Against:* it needs a store for the show history, and someone has to write and prune the tips. The tips compete with the Intention for attention.
2. **Empty states that teach.** No projects, no map, no PR briefs, a session with no turns: each says what will appear there and offers the one action to fill it, in the manner of VS Code's watermark with live keys from `titled()`.
   - *For:* this is pull help that is never in the way, and NN/g and Primer both endorse it.
   - *Against:* it disappears once there is data, so it does not cover later features.
3. **A first-run checklist, in the style of VS Code walkthroughs.** Three to five steps (add a project, see a turn, open the map, read a PR brief), each ticked by the real event, not by a click. It can be dismissed and reset.
   - *For:* it is self-paced, and checklist-started tours complete more often (low trust).
   - *Against:* it needs completion events from server state, and a solo user sees it once.
4. **A short replayable tour, like CodeBoarding's.** At most four steps, anchored on real elements, skipping any hidden target, with a "replay" entry in the palette.
   - *For:* it is cheap and familiar.
   - *Against:* NN/g's data says up-front tours don't help. Steps break when the layout changes, and it needs snap fixtures.
5. **A hold-key shortcut card** (hold `?` or H). The palette already lists keys, so this would only add a glanceable view.
   - *For:* it costs very little.
   - *Against:* it partly duplicates the palette.
6. **What's new after a rebuild, once, like Zed.** On the first load after the embedded version or commit changes, a toast says "Updated: N changes", with a "Show" link to a list built from merged commit titles since the last commit Daniel saw. Each line is one sentence and links to its issue.
   - *For:* Daniel rebuilds after each merge (AGENTS.md "Checks"), so the trigger is natural. It also feeds understanding.
   - *Against:* commit titles are written for git, not for a reader. A hand-kept changelog costs a step in every PR. It must show once and be silenceable.
7. **New labels at the feature's own place,** following Primer. A small "New" label on a control added since the last version Daniel saw, which clears when he uses it or after N days.
   - *For:* it sits where the feature lives in perpetuity.
   - *Against:* it needs each new control to be tagged with the version it arrived in. Too many at once become noise, and Primer caps it at two.
8. **A "How it works" entry in the palette or settings,** linking to `docs/studio/behaviour/*.md` for each area.
   - *For:* this is pull help with no new UI.
   - *Against:* the docs are written for agents and tickets, and may read poorly as help.

## Pitfalls

- **Front-loading.** Tours at first launch get skipped and don't improve tasks (NN/g). Prefer pull help and help in context.
- **Chaining.** Several hints in a row get dismissed faster (NN/g 2014). Primer says one bubble at a time and at most two alerts.
- **Campaigns that never end.** Cap impressions and give each campaign an end date (Primer). Claude Code had to cap one tip at three lifetime shows and remove others.
- **Hints for features the user can't use.** Claude Code fixed tips suggesting commands that were off or unavailable. Gate every hint on the capability, in the same way as `ui/capabilities.ts`.
- **Pointing at hidden elements, or at a moved layout.** Primer says "Don't point to hidden elements". CodeBoarding anchors its steps on `data-tour-target`.
- **Essential information in tooltips.** Anything needed to finish a task must be on the page (NN/g). Hover content must be dismissible, hoverable and persistent (WCAG 1.4.13), and must also work on keyboard focus.
- **Context and cost leaks.** Claude Code once put the whole changelog into the model's context through `/release-notes`. A what's-new view must stay out of any writer or agent prompt.
- **Teaching that turns into a score.** A checklist's progress bar can read like the scores and streaks that `home.md` §5 warns about. Keep progress private and plain.
