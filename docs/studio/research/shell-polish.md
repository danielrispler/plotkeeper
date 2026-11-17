# Shell polish: prior art from other agent tools

Prior-art notes from the UI polish grilling of 17 November 2026 (spec #156), saved by #161. They cover agent identity in rows, usage across agents, the settings entry, the title bar, worktree rows, tooltips and new-item buttons. Studio terms follow `docs/studio/CONTEXT.md`; each tool's own words appear only when describing that tool.

## How this was read

- **Code.** Shallow clones read as source, never built or run: [herdr-app](https://github.com/herdrdev/herdr-app) (`321c9a8`), [cmux](https://github.com/manaflow-ai/cmux), [T3 Code](https://github.com/pingdotgg/t3code) (`1da3f3f3`), [workmux](https://github.com/raine/workmux), [CodexBar](https://github.com/steipete/CodexBar) (`1f6da9b`), [ccusage](https://github.com/ccusage/ccusage) and [Zed](https://github.com/zed-industries/zed) (`main`). Doc pages were read through WebFetch.
- **Links.** Links point at each repo's default branch. Line numbers are those of the clone read and may have moved since.
- **Limits.** Nothing was run, no screenshot was viewed and no credential file was read. Daniel's copy, with raw copies of the files cited, is `local-evidence/2026-11-17-ui-polish/` (`reports/competitors.md`, `raw/`). Earlier notes ([herdr.md](herdr.md), [readme-survey.md](readme-survey.md), [multi-repo-workspaces.md](multi-repo-workspaces.md)) were not redone.

## 1. Agent identity and running state in rows

**herdr (app).** Logo and state are two separate elements in one row, not a logo inside a ring.
- The row is `[agent logo 16px] [name, truncated] [pin button on hover | elapsed time] [status element]`, 30px high with a 9px gap ([`defaults/mods/agents/main.js`](https://github.com/herdrdev/herdr-app/blob/master/defaults/mods/agents/main.js), lines ~296-375).
- The logo is the program's icon from [`defaults/programs.json`](https://github.com/herdrdev/herdr-app/blob/master/defaults/programs.json) (`icon(p, 16, color)`): a one-colour SVG tinted with the foreground colour, or a picture. It is not replaced by the state and not animated.
- The status element sits at the row's right end (`status(t, a, compact)`, line ~209 of the same file):
  - working: a 12px ring spinner (a faint full circle plus one 90-degree arc), `.spin(900)`, coloured `t.working`.
  - blocked: a pill "Needs you" on a 15% tint; in the compact priority view, a 14px square "!".
  - idle: a 7px hollow ring with a 1.5px border. done: a 7px filled dot.
- Tabs use the same spinner or a 6px dot ([`defaults/mods/tabs/main.js`](https://github.com/herdrdev/herdr-app/blob/master/defaults/mods/tabs/main.js), lines 122-123).
- The collapsed sidebar rail shows the row number and the status element only, no logo.
- The spinner is native (`svg().spin(900)`). The guide says never to drive a spinner from a timer, as it "costs real battery" ([`defaults/AGENTS.md`](https://github.com/herdrdev/herdr-app/blob/master/defaults/AGENTS.md), ~line 1010).
- Subagents: a fold chevron on the logo (`agent-fold-*`) nests child agents under the parent ([`agents/main.js`](https://github.com/herdrdev/herdr-app/blob/master/defaults/mods/agents/main.js)).
- State comes from OSC 7501 only (see [herdr.md](herdr.md)); the logo comes from the program name, not from the state.

**cmux.** Brand icon and spinner are separate slots.
- Terminal tabs "show the active agent's brand icon in a static slot" ([CHANGELOG](https://github.com/manaflow-ai/cmux/blob/main/CHANGELOG.md), #7607). The icon is resolved from the agent definition that process and hook detection use, and shows only while that agent runs ([`Sources/TerminalTabAgentIcon.swift`](https://github.com/manaflow-ai/cmux/blob/main/Sources/TerminalTabAgentIcon.swift)).
- Workspace rows carry a macOS-spokes `GPUSpinnerNSView`, placed leading or trailing by the user (`loadingSpinnerPosition`), shown when `activeCodingAgentCount > 0`, with a tooltip ([`SidebarWorkspaceRowCellView.swift`](https://github.com/manaflow-ai/cmux/blob/main/Sources/Sidebar/AppKitList/Cells/SidebarWorkspaceRowCellView.swift), lines 628-834).
- The setting "Compact Agent Status" (off by default) merges agent, PR and branch state into one glyph with a tooltip ([CHANGELOG](https://github.com/manaflow-ai/cmux/blob/main/CHANGELOG.md), #14838).
- Attention is a blue ring on the pane, a lit tab and a sidebar badge ([README](https://github.com/manaflow-ai/cmux#readme)).
- Agent spinners "animate at their own step rate and stop while hidden" ([CHANGELOG](https://github.com/manaflow-ai/cmux/blob/main/CHANGELOG.md), #14832).

**T3 Code.**
- The provider icon sits at the right end of the thread row's second line at 60% opacity (`SidebarProviderStack`, [`SidebarThreadRow.tsx`](https://github.com/pingdotgg/t3code/blob/main/apps/web/src/components/sidebar/SidebarThreadRow.tsx), lines 192-240). Earlier providers of a handed-off thread show as smaller greyscale icons beside it.
- State is text first: "Working", "Pending Approval", "Awaiting Input", "Plan Ready" ([`Sidebar.logic.ts`](https://github.com/pingdotgg/t3code/blob/main/apps/web/src/components/Sidebar.logic.ts), lines 637-645), plus pulsing icons (`animate-status-pulse`) for terminal state.
- Each provider has an accent colour on the usage page: Claude `#d97757`, Antigravity `#8c7bd1` and others ([`usageProviders.ts`](https://github.com/pingdotgg/t3code/blob/main/apps/web/src/components/usage/usageProviders.ts)).

**workmux (tmux).**
- The status icon is configurable text (`working: '🤖'`, `waiting: '💬'`, `done: '✅'`; [README](https://github.com/raine/workmux#readme), ~line 527). The sidebar template has separate `{status_icon}` and `{agent_icon}` tokens.
- Default agent icons are letters with brand accent colours: `claude` CC, `codex` CX, `opencode` OC, `gemini` G, `pi` π ([`customization.md`](https://github.com/raine/workmux/blob/main/docs/src/content/docs/guide/sidebar/customization.md), "Agent identity"). Quiet agents show `○`.
- The dashboard sorts by priority: Waiting, Done, Working, Stale.

**Not verified.**
- [AgentsRoom](https://agentsroom.dev) (closed; marketing page only): "each agent has a role, a face, and a thread", a provider per agent, live status of "who is working right now". How a row draws the provider is not described.
- [Conductor](https://www.conductor.build/docs/) lists Claude Code, Codex, Cursor and OpenCode; the page read does not describe the sidebar.
- [Cursor's cloud agents](https://cursor.com/docs/cloud-agent): the page read does not describe list rows.

**Takeaway.** The two native-GUI leaders, herdr and cmux, keep identity and state in two slots: a static logo, and a spinner or dot elsewhere. No tool read puts the logo inside the ring, so a ring around a logo would be a new design, not a copy.

## 2. Usage and limits across agents

| Tool | Surface | Providers | What a provider shows |
| --- | --- | --- | --- |
| [CodexBar](https://github.com/steipete/CodexBar) | macOS menu-bar item per provider, or "Merge Icons" with a switcher, plus a popover; a Qt tray on Linux | 100+, including Claude, Codex, Gemini, Antigravity, Cursor, Copilot | A tile with session, weekly and (Sonnet) bars, a reset countdown, credits, a local cost scan and incident badges ([README](https://github.com/steipete/CodexBar#readme), [`docs/ui.md`](https://github.com/steipete/CodexBar/blob/main/docs/ui.md)) |
| [T3 Code](https://github.com/pingdotgg/t3code) | A "Usage" button in the sidebar footer, beside Settings, Pull requests and Issues, opens a `/usage` page | Claude, Codex, Antigravity for limits; more for tokens and cost ([`usageProviders.ts`](https://github.com/pingdotgg/t3code/blob/main/apps/web/src/components/usage/usageProviders.ts)) | Limit bars of kind session or weekly, "resets in", a pace icon (ahead, on or under pace) with a tooltip, a reset-credit action ([`UsageLimits.tsx`](https://github.com/pingdotgg/t3code/blob/main/apps/web/src/components/usage/UsageLimits.tsx)) |
| Claude Code `/usage` ([costs docs](https://code.claude.com/docs/en/costs)) | A TUI screen | Claude only | Session and weekly bars, activity, attribution; `r` retries; when rate limited it shows "last-known usage" for up to 60 minutes and "rate limited until 3:14am" |
| [ccusage](https://github.com/ccusage/ccusage) | CLI reports and a Claude Code status-line hook | 20 sources, including Claude Code, Codex, Gemini CLI, Antigravity | Daily, weekly, monthly and session reports; `blocks` are Claude's 5-hour windows |
| Antigravity `agy` ([usage docs](https://antigravity.google/docs/cli/commands/usage), read through a search snippet only) | `/usage` (alias `/quota`) in the CLI; `agy -p "/usage" --output-format json` | Antigravity | An interactive quota panel; reset times in the local zone, JSON in UTC |
| Codex | `/status` in the CLI | Codex | 5-hour and weekly limits as percent left plus reset time, from community reports only ([example](https://community.openai.com/t/chatgpt-codex-app-says-i-ve-hit-the-usage-limit-but-status-shows-97-left/1392886)); reports conflict on whether some plans still show the 5-hour line |
| [AgentsRoom](https://agentsroom.dev) | Not described | Several accounts | "Gauges for every account", a warning before the limit, switching to the next account, session and weekly trackers (marketing page only) |

**Data sources.** Listed only; no credential file was read. The studio may use none that read a credential (`AGENTS.md`, Hard rules); #161 measures the two that do not.
- **Claude, plan limits.** `GET https://api.anthropic.com/api/oauth/usage` with the OAuth bearer, `anthropic-beta: oauth-2025-04-20` and the `user:profile` scope; reply keys `five_hour` and `seven_day` ([CodexBar `docs/claude.md`](https://github.com/steipete/CodexBar/blob/main/docs/claude.md), lines 230-250). CodexBar's fallbacks are `/usage` or `/status` in a `claude` PTY, and claude.ai's web API with cookies (`/api/organizations/{id}/usage`). T3 Code asks the Agent SDK instead, with control request `get_usage` returning `rate_limits` ([`claudeUsageLimits.ts`](https://github.com/pingdotgg/t3code/blob/main/apps/server/src/provider/claudeUsageLimits.ts)). The studio uses its own status-line snapshot.
- **Claude, tokens and cost.** The local JSONL under `~/.claude/projects` ([ccusage](https://github.com/ccusage/ccusage); [CodexBar `docs/claude.md`](https://github.com/steipete/CodexBar/blob/main/docs/claude.md), ~line 665).
- **Codex, limits.** JSON-RPC to a local `codex app-server`, methods `account/read` and `account/rateLimits/read` ([CodexBar `docs/codex.md`](https://github.com/steipete/CodexBar/blob/main/docs/codex.md), "Codex CLI RPC", lines 309-322). T3 Code reads the same shape: `primary` and `secondary` windows with `usedPercent`, `resetsAt`, `windowDurationMins`, and `planType` ([`codexUsageLimits.ts`](https://github.com/pingdotgg/t3code/blob/main/apps/server/src/provider/codexUsageLimits.ts)). The alternative, `GET https://chatgpt.com/backend-api/wham/usage`, needs the OAuth token ([CodexBar `docs/codex.md`](https://github.com/steipete/CodexBar/blob/main/docs/codex.md), line 61). Cost comes from `~/.codex/sessions/YYYY/MM/DD/*.jsonl` (ccusage, CodexBar).
- **Antigravity.** CodexBar tries, in order, a local language-server probe, the `agy` CLI (`agy -p /usage --output-format json`, from agy 1.1.11; agy 1.2.2 and later reject tokenless local requests with `401 missing CSRF token`), then a Google OAuth fallback ([CodexBar `docs/antigravity.md`](https://github.com/steipete/CodexBar/blob/main/docs/antigravity.md)). CodexBar runs print mode in a private temporary directory with `AGY_CLI_DISABLE_AUTO_UPDATE=true` and bounds it to 90 seconds. T3 Code posts to `https://cloudcode-pa.googleapis.com/v1internal:<op>` (and the `daily-` host) with a Google access token it refreshes from the app's own token file, windows "5h" and "weekly" ([`antigravityUsageLimits.ts`](https://github.com/pingdotgg/t3code/blob/main/apps/server/src/provider/antigravityUsageLimits.ts)). ccusage reads `~/.gemini/antigravity*/conversations/` for tokens ([ccusage](https://github.com/ccusage/ccusage)).
- **Not verified.** The Gemini CLI quota path. CodexBar says Gemini CLI OAuth was shut down in June 2026 and Antigravity replaces it.

**Pattern for the studio.** A popover or page with one row per provider: two bars (5-hour session and weekly), "resets in", a pace marker, and stale data labelled with its age. When one icon must stand for all providers, CodexBar's merge rule picks the provider with the highest usage or the frontmost one.

## 3. Settings entry

- **herdr.** A "Settings" row with a gear icon in the sidebar footer, above "Add machine"; it runs `app.settings` ([`workspace-chrome/main.js`](https://github.com/herdrdev/herdr-app/blob/master/defaults/mods/workspace-chrome/main.js), line 44). The collapsed rail shows the icon with a tooltip, and a green dot marks it when an update is ready. Settings are also a file, `~/.config/herdr-app`, which the person's agent edits.
- **cmux.** `Cmd+,` opens Settings ([README](https://github.com/manaflow-ai/cmux#readme), shortcuts table), and the sidebar's Help menu has a Settings item ([CHANGELOG](https://github.com/manaflow-ai/cmux/blob/main/CHANGELOG.md), #13211). Settings show one section at a time beside a sidebar of categories. `cmux.json` applies live.
- **T3 Code.** A utility row in the sidebar footer: the Settings gear first, then Pull requests, Issues and Usage ([`SidebarChrome.tsx`](https://github.com/pingdotgg/t3code/blob/main/apps/web/src/components/sidebar/SidebarChrome.tsx), lines 253-300). On a utility page the row becomes a "Back" button.
- **Zed.** A Settings Editor (`zed: open settings`) plus `settings.json`. The title bar's user menu holds Settings, Keymap, Themes, Icon Themes and Extensions ([`title_bar.rs`](https://github.com/zed-industries/zed/blob/main/crates/title_bar/src/title_bar.rs), lines 1396-1408). `Cmd+,` was not confirmed from the page read.
- **CodexBar.** The menu popover has Settings; the settings window lists providers (drag to reorder), plus a JSON config ([`docs/ui.md`](https://github.com/steipete/CodexBar/blob/main/docs/ui.md)).
- **Not verified:** Conductor, AgentsRoom, Cursor.

## 4. Title bar

- **herdr.** No native title bar (`"titlebar":"none"`, traffic lights at 12,16). The 268px sidebar has a 46px brand row: logo and "Herdr" on the left, search and sidebar-toggle buttons on the right ([`workspace-chrome/main.js`](https://github.com/herdrdev/herdr-app/blob/master/defaults/mods/workspace-chrome/main.js)). The work area has a 42px tab strip on top (tabs, then "+"), a 30px header per pane and a 28px status bar ([`defaults/layouts/main.json`](https://github.com/herdrdev/herdr-app/blob/master/defaults/layouts/main.json)). There is no project or branch picker in the top bar: git information lives in the space rows.
- **cmux.** Title-bar buttons (sidebar toggle, notifications, new tab) sit left, beside the traffic lights; an update pill and focus-history back and forward live in the title bar; "minimal mode" hides it ([CHANGELOG](https://github.com/manaflow-ai/cmux/blob/main/CHANGELOG.md), ~line 3258). The exact left and right split was not verified.
- **T3 Code.** The header is a breadcrumb on the left, project (with favicon) then thread title, with actions on the right ([`ChatHeader.tsx`](https://github.com/pingdotgg/t3code/blob/main/apps/web/src/components/chat/ChatHeader.tsx), project crumb ~line 272, thread crumb ~line 326). The right-hand buttons were not fully read.
- **Zed** ([`title_bar.rs`](https://github.com/zed-industries/zed/blob/main/crates/title_bar/src/title_bar.rs), lines 237-460). Left: the application menu (only with `show_menus`), a restricted-mode badge, the project host, the project name (a recent-projects popover), then worktree name and branch (`render_worktree_and_branch`; the branch is truncated to `MAX_BRANCH_NAME_LENGTH`, with an optional git-status icon). Right: collaborators, call controls, connection status, update version, and sign-in or the user menu. Each part has a `title_bar.*` setting (`show_branch_name`, `show_project_items`, `show_worktree_name`, `show_branch_status_icon`, `show_user_menu`).

## 5. Sessions and worktrees in the sidebar

- **herdr.** Two lists. "Spaces" has one row per workspace; a worktree space moves under its main checkout, indented 22px with a branch icon. "Agents" has one row per agent, grouped by space or by priority. A space row shows git information only where unusual, `name · repo · branch ↑2 ↓1`: the branch is dropped when it equals the name (a worktree is named after its branch) or is `main` or `master`, and the repo is dropped for worktrees (`gitTail`, [`spaces/main.js`](https://github.com/herdrdev/herdr-app/blob/master/defaults/mods/spaces/main.js), lines 418-440, and the comments at the file's head). Agent rows under a space header omit the space name; without a header (priority view, pinned) a row shows `space · detail`.
- **cmux.** One row per workspace (a vertical tab). Branch, linked PR, working directory, ports and the last notification appear as extra lines under the title; settings hide them ("Hide All Sidebar Details", "Show Branch + Directory") ([README](https://github.com/manaflow-ai/cmux#readme), [CHANGELOG](https://github.com/manaflow-ai/cmux/blob/main/CHANGELOG.md)). Agents are not separate rows.
- **workmux.** One tile per agent, named by its worktree. `{primary}` is the worktree or window name; `{secondary}` is the next label in the chain, "with worktree appended if not already primary"; git stats and the PR number sit on the second line; tiles group by project or session ([`customization.md`](https://github.com/raine/workmux/blob/main/docs/src/content/docs/guide/sidebar/customization.md)).
- **T3 Code.** Threads nest under a project. A thread's second line shows the branch (middle-truncated) with a worktree indicator, then terminal, PR badge, diff counts and provider icon ([`SidebarThreadRow.tsx`](https://github.com/pingdotgg/t3code/blob/main/apps/web/src/components/sidebar/SidebarThreadRow.tsx), lines 1180-1230). A worktree is a property of a thread, not a row.
- **Conductor.** "Each task gets its own workspace, branch" ([docs](https://www.conductor.build/docs/)); the sidebar layout was not verified.
- **Common rule.** Show the branch only when it differs from the name, hide `main` and `master`, or demote the branch to a subtitle.

## 6. Truncated text and tooltips

- **herdr.** Names use `truncate` with `max_w`. Tooltips are the native component's `.tooltip("...")` on icon buttons, rail icons and the machine login line ([`spaces/main.js`](https://github.com/herdrdev/herdr-app/blob/master/defaults/mods/spaces/main.js), lines 370-408; [`agents/main.js`](https://github.com/herdrdev/herdr-app/blob/master/defaults/mods/agents/main.js), lines 330 and 442). Names in the agents list have no tooltip in the source read. The delay lives in the shell crate, not the mods, and was not verified.
- **T3 Code.** Custom `Tooltip`, `TooltipTrigger` and `TooltipPopup` components (Base UI) wrap icons and pace icons ([`tooltip.tsx`](https://github.com/pingdotgg/t3code/blob/main/apps/web/src/components/ui/tooltip.tsx)). Thread rows have a rich `SidebarThreadTooltip` showing branch, providers and PR. `MiddleTruncate` cuts branch names in the middle, with `showTitle` for a native title ([`middle-truncate.tsx`](https://github.com/pingdotgg/t3code/blob/main/apps/web/src/components/ui/middle-truncate.tsx)). The delay was not found.
- **cmux.** SwiftUI `.help(...)`, the native macOS tooltip, and `.safeHelp`. "Hover tooltips on workspace and pane tabs" ([CHANGELOG](https://github.com/manaflow-ai/cmux/blob/main/CHANGELOG.md), #3329); title-bar hint pills appear instantly and only fade out (#14984).
- **Zed.** Custom `Tooltip::text("New…")` and `Tooltip::for_action(...)`, which shows the shortcut ([`terminal_panel.rs`](https://github.com/zed-industries/zed/blob/main/crates/terminal_view/src/terminal_panel.rs)). The delay was not verified.

## 7. Terminal tabs and new-item buttons

- **herdr.** One "+" at the end of the tab strip ([`tabs/main.js`](https://github.com/herdrdev/herdr-app/blob/master/defaults/mods/tabs/main.js), line 150) opens a menu ([`new-menu/main.js`](https://github.com/herdrdev/herdr-app/blob/master/defaults/mods/new-menu/main.js)): "Terminal" with its shortcut; a heading "Start an agent" over the installed agents (number keys 1-9; agents not installed are greyed with "not installed"); then "New space", "New worktree…" and "Open worktree…". So "+" makes a new tab of any kind, and the agent is chosen in the menu, not by separate buttons.
- **cmux.** `Cmd+T` opens a new tab, `Cmd+D` splits right and `Cmd+Shift+D` splits down. The Dock toolbar's "+" menu offers New Terminal and New Browser ([`docs/dock.md`](https://github.com/manaflow-ai/cmux/blob/main/docs/dock.md)); the browser stack has "New Tab". No separate "new agent" button was found.
- **Zed** ([`terminal_panel.rs`](https://github.com/zed-industries/zed/blob/main/crates/terminal_view/src/terminal_panel.rs), lines 146-215). A "+" icon button, tooltip "New…", opens a popover with "New Terminal" and "Spawn Task". A separate split button, tooltip "Split Pane", opens Split Right, Left, Up and Down. The commands `workspace: new terminal` (panel) and `workspace: new center terminal` differ.
- **T3 Code.** A "New Terminal (shortcut)" button in the terminal drawer ([`ThreadTerminalDrawer.tsx`](https://github.com/pingdotgg/t3code/blob/main/apps/web/src/components/ThreadTerminalDrawer.tsx), line 1647).
- **Common rule.** One "+" per tab strip, with a menu when there is more than one kind of thing to make; split is a separate button.

## Could not verify

- How herdr, cmux and T3 Code draw the logo at pixel level: no screenshot was viewed.
- AgentsRoom, Conductor and Cursor UI details: their pages did not describe rows, settings or usage.
- Tooltip delays in any tool.
- The data endpoint behind Claude Code's `/usage`: the docs do not name it, and the OAuth usage endpoint comes from CodexBar's docs.
- The exact layout of Codex's `/status`: community reports only.
- Whether herdr-app has a built-in usage view: none was found in `defaults/mods`.
