# Home as the waiting room

> **Answer:** No benchmark tool has a dashboard home: the session list grouped by project is the main surface, needs-attention rises to the top, and finished work is cleared by a reversible archive, in bulk with an undo (Cursor) or by itself once its PR merges (Claude Code, T3 Code). None puts anything about learning on its home. That part is the studio's own: one optional recall item, served at the moment Daniel waits, since retrieval spaced over days is what makes a change stick, and a list of past explanations is only re-reading. Numbers that inform, against Daniel's own past, are allowed; streaks and live percent-correct move showing up, not learning, and are tried only as reversible trials.
> **Evidence:** Moderate for the shell patterns (official docs for Claude Code, Zed, GitHub; forum and changelog posts for Cursor, T3 Code, Superset; thin for Codex, Antigravity, Windsurf). Strong for retrieval practice and spacing. Moderate for what scores and streaks do to motivation and showing up, mostly from mass-market apps and schoolchildren; none tests a solo developer's private prompts. Nothing was run.

Research date: 2026-11-20, for #218 ([resolution](https://github.com/danielrispler/plotkeeper/issues/218#issuecomment-7037432816)). Studio's code is read on `main` at `2769fc239`. Raw reports, with every source: `local-evidence/2026-11-20-issue-218-home/reports/` (`agent-ides.md`, `litt.md`, `learning-tools.md`, `home-data.md`, `visual-design.md`, `no-score-origin.md`, `retention-science.md`, `adherence-evidence.md`). Studio terms follow `docs/studio/CONTEXT.md`.

## 1. How agent IDEs build their home

| Tool | Home | Clearing finished work | Source |
|---|---|---|---|
| Cursor (Agents window) | Sidebar of agents grouped by repository; archived ones hidden behind a filter | "Archive All" in the "…" menu, reversible, with an "Undo all" toast; users still panicked at the empty "No agents yet" | [forum, staff reply](https://forum.cursor.com/t/archive-all-caused-all-agent-chats-to-disappear-from-the-agents-sidebar/167312) |
| Claude Code desktop | Sessions grouped by project; "+ New" opens a start screen (environment, folder, permission mode, model, prompt) | Archive one by one; auto-archive after PR merge or close is a setting; bulk archive is a repeated request | [docs](https://code.claude.com/docs/en/desktop), [request](https://claudeissues.com/issue/85704-feature-bulk-archive-for-code-sessions-in-the-desktop-app-sidebar) |
| T3 Code (Sidebar V2) | Active threads as cards; finished ones in a compact "Settled" list of one-liners with a hidden count | Settles after 3 idle days or when its PR merges; snooze; approval-blocked threads pinned on top with their wait | [video notes](https://openclawdatabase.com/news/videos/2026-07-27-t3-code-sidebar-v2/index.md), [PR #4068](https://github.com/pingdotgg/t3code/pull/4068) |
| Superset | Workspaces page for triage: Needs attention, Working, Needs review, Idle, Merged, with a board toggle | Multi-select; bulk delete flags unpushed work first | [changelog](https://paradev.com/changelog/2026-08-16-workspaces-triage-editable-markdown) |
| GitHub Copilot agents | "Pick up where you left off" with "View all" | Cloud sessions cannot be deleted | [docs](https://docs.github.com/en/copilot/how-tos/use-copilot-agents/cloud-agent/track-copilot-sessions) |
| Zed | Welcome page of verbs (open, clone, docs) that shows only when nothing else does | Threads to history | [docs](https://zed.dev/docs/getting-started) |

**Where they agree:** no separate dashboard; group by project with a status per row; needs-attention on top or loudly marked; clearing means a reversible archive, and bulk clear comes with an undo. **Rare:** auto-settle rules, snooze, and Claude Code's `/stats` heatmap with streaks ([blog](https://blog.vincentqiao.com/en/posts/claude-code-stats/)), which counts activity, not understanding. **None:** a learning or recall element on home, a digest of what happened while away, or a "do you understand this?" step before clearing (inference from absence).

## 2. Geoffrey Litt

- **"Understanding is the new bottleneck"** ([2026-07-02](https://www.geoffreylitt.com/2026/07/02/understanding-is-the-new-bottleneck)) is the studio's thesis in his words: skipped understanding is "cognitive debt", we understand "to participate", not only to verify. His explainers are prose "literate diffs" ending in a five-question quiz: "A quiz is a speed regulator." "I won't send code to others until I can pass the quiz." No threshold, no grader named.
- **HUDs, not copilots** ([2025-07-27](https://www.geoffreylitt.com/2025/07/27/enough-ai-copilots-we-need-ai-huds)): extend the user's own perception rather than talk to them (after Weiser).
- **Code like a surgeon** ([2025-10-24](https://www.geoffreylitt.com/2025/10/24/code-like-a-surgeon)): high visibility on primary work, secondary work in the background.
- **AI as teleportation** ([2025-09-10](https://www.geoffreylitt.com/2025/09/10/ai-as-teleportation)): keep the friction that was doing hidden work.

For Home (inference, not his words): a calm view of real state, a "landed but not understood" signal, recall tied to current work. He says nothing for or against graphs.

## 3. Learning tools: what holds without a live score

- **Show what, not how well:** Readwise, Day One "On this day" (absent when empty), decision logs.
- **The schedule is the progress:** Quantum Country and Orbit show "back in N days"; a forgotten prompt drops an interval, not to zero.
- **Quiet overload:** SuperMemo postpones low-priority items instead of showing a backlog; a growing overdue count is what users ask to cap ([Mochi forum](https://forum.mochi.cards/posts/412)).
- **A flat "done for now"** (Anki).
- **Re-showing is re-reading:** no controlled study shows a home view, reminder list or graph raises retention; the gain is in the delayed retrieval itself, so a reminder should ask before it shows.

## 4. The no-score rule: where it came from

Daniel's `problems.md` (2026-10-23) listed "Guessing, not testing … with no score to fail" as a hypothesis ("None of them is decided"), beside "a calibration rating or a chess-style rating". The research of 10-26/27 said "None of it needs a score" (learning that sticks, #272) and advised a rare, reflective number at a weekly debrief rather than live counters (measuring improvement, #280; gamification, #275, "Moderate … None of it is about developers"). Agent-written tickets then hardened it: "never score it" ([Falinks#315](https://github.com/danielrispler/Falinks/issues/315)), "Guesses are logged, not shown", listed as "Later" ([Falinks#321](https://github.com/danielrispler/Falinks/issues/321#issuecomment-6493004697)), "The app shows no rate, score or tally" ([Falinks#346](https://github.com/danielrispler/Falinks/issues/346)). It was never tested: the weekly command never ran and the #102 self-test never started.

## 5. Retention versus showing up

| Mechanic | Retention | Showing up | Strength |
|---|---|---|---|
| Retrieval practice, spaced over days | Large gain (testing effect g ≈ 0.5) | Neutral | Strong |
| Explaining every option after the pick | Gain; corrects confident errors | Neutral | Strong |
| A bare score in place of explanation | Little or none; over a third of feedback interventions lowered performance (Kluger & DeNisi 1996) | Small | Moderate |
| Grades beside comments | Hurt interest (Butler 1988, schoolchildren) | Hurt | Moderate, weak transfer |
| Confidence before the pick, then the outcome | Better calibration | Neutral | Moderate |
| Streak | None shown | Up while alive, down when broken, most with self-blame ([Silverman & Barasch 2023](https://doi.org/10.1093/jcr/ucac029)); developers kept GitHub streaks until the counter went ([Moldon et al.](https://arxiv.org/abs/2006.02371)) | Moderate |
| Points, levels | Quantity up, quality flat | Up, fades with novelty | Moderate |
| Better scheduling of items | Aimed at learning | +12% daily engagement ([Settles & Meeder 2016](https://aclanthology.org/P16-1174/)) | Moderate |
| One prompt at a natural break, low friction | — | Raises seen and answered | Strong in general, untested here |

Duolingo's streak figures (about +14% retention from the streak wager) are company-reported ([Lenny's Newsletter](https://lennysnewsletter.com/p/behind-the-product-duolingo-streaks)). Several classic citations above came from the agents' recall; the raw reports mark which.

## 6. What Home had to work with

On 2026-11-20 (`home-data.md`, `scripts/home-tally.py`): 494 Quizzes offered and 12 answered; 187 Retests waiting and 0 answered. Home draws every unanswered Retest at once and marks each "shown" as it builds the row, seen or not ([`ui/home.ts:31-40`](https://github.com/danielrispler/plotkeeper/blob/2769fc239/ui/home.ts#L31-L40)). A visibility bug hid cards for several days, so answer rates since 11-14 are not used.

## 7. The look

Calm, crafted homes share five moves: one page title that says where and when; few modules with plain labels; colour only for what needs you; an empty state that feels like a result (Superhuman's Inbox Zero, [post](https://blog.superhuman.com/how-superhuman-chooses-inbox-zero-images)); bulk clear by one action, not an X per row ([Linear Inbox](https://linear.app/docs/inbox)). Of three directions (calm briefing, instrument panel, logbook), the calm briefing page after Linear and Things ([features](https://culturedcode.com/things/features/)) fits the waiting room; the instrument panel reads most like a score.
