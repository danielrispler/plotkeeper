# A settings page, and where its entry sits

> **Answer:** Copy the agent apps' shape: a category list on the left, one section at a time on the right, title + muted description + control per row, saved on every change with no Save button, opened by ⌘, and closed by Esc (herdr, T3 Code, Superset, cmux, Codex). For the file, copy cmux, herdr and Zed: the page writes the same hand-edited `settings.json` with minimal text edits that keep comments, refuses to write a file that does not parse, and keeps an "Open settings.json" entry on the page. Reset is an undo icon that appears on a row only when its value differs from the default (T3 Code, Zed); it deletes the key. No benchmark tool puts a settings gear in the top bar except Vibe Kanban (right-hand icon group); the rest keep it in the sidebar footer (T3 Code, herdr) or a menu (Zed, Superset).
> **Evidence:** Strong for T3 Code, cmux, herdr, Vibe Kanban, Superset, Zed and VS Code (source read at pinned commits). Docs only for Conductor, Codex, Cursor, Claude desktop. Weak for the top-bar gear: one tool, and no tool's narrow-window behaviour was found. Nothing was run.

Research date: 2026-11-18, for #185. Studio's code is read on `main` at `5f4e51746`. Raw reports, with every file and line cited: `local-evidence/2026-11-18-issue-185-settings-page/reports/` (`benchmark-tools.md`, `agent-apps.md`, `zed.md`, `vscode.md`, `code-facts.md`). Studio terms follow `docs/studio/CONTEXT.md`. This updates `shell-polish.md` §3: T3 Code's settings are now a modal route at `8a6342e7`, not a page with a "Back" row.

## 1. The page's shape

| Tool | Where it opens | Layout | Size | Saving |
|---|---|---|---|---|
| [herdr-app](https://github.com/herdrdev/herdr-app/tree/4deb605f0b3d25d409c80cff9082d5c19a043ea8) | Dialog over the window, 880×620 (`src/window.rs` 3714-3722) | 6 pages left (Look, Keyboard, Machines, Alerts, Updates, Privacy), no search; row = title 14px, muted 12.5px description, control right (`defaults/frame/settings/main.js` 161) | ~25-30 controls | Instant |
| [T3 Code](https://github.com/pingdotgg/t3code/tree/8a6342e761357feff652a57d3dd16f0c78026c25) | Near-full-window modal route `/settings/<section>` (`AppRouteModal.tsx`) | Grouped nav with search; `SettingsRow` = title, description, control (`settingsLayout.tsx` 313) | ~30 sections | Instant |
| [cmux](https://github.com/manaflow-ai/cmux/tree/3f2f16143ae8d67d7f505c5c8dd246cd5d75e99a) | Separate native window, min 820×540 | ~22 categories left, search | Hundreds | Instant, live |
| [Superset](https://github.com/superset-sh/superset/blob/dbb16b8f6b226a1bd884e877bec4b5d336549691/apps/desktop/src/renderer/routes/_authenticated/settings/layout.tsx) | Whole-window route replacing the workspace, "Back" link at the top of its sidebar | 26 sections, search with match counts; `SettingsRow` = label, hint, control | Large | Instant |
| [Vibe Kanban](https://github.com/BloopAI/vibe-kanban/blob/3f095f0186f6e68d67143d38b02da2238f6524d8/packages/web-core/src/shared/dialogs/settings/SettingsDialog.tsx) | Modal 900×700 | 7 sections, no search | Medium | Draft + sticky Save bar; "Discard?" on leaving |
| Zed (`5b282ac8`) | Separate window "Zed — Settings" (`settings_ui.rs` 851-900) | Pages left + search; title, undo icon, muted description, control | 15 pages, ~480 rows, hand-written (`page_data.rs`) | Instant |
| VS Code (`36e8c028`) | Editor (modal by default in current docs) | Search, group tree, User/Workspace tabs | All registered settings, generated | Instant |

**Where they agree:** a left category list with one section at a time; rows of title, description and control; instant saving. Only Vibe Kanban has a Save button. Search appears only in the large ones; herdr's 6 pages have none. **Inference:** at Studio's ~12 settings, herdr is the size match.

**Over the work, not in a pane.** Every tool opens settings over the workspace (dialog, modal, window or whole-window route). None puts it in a side pane like Studio's skills browser.

**The list is hand-written.** Zed hand-curates every row with a typed getter and setter (`page_data.rs`); herdr and T3 Code do the same. Only VS Code generates rows from a schema. Settings without a control stay file-only: Zed docs, "Some advanced options … require editing the JSON file directly" (`docs/src/configuring-zed.md` 23); VS Code shows "Edit in settings.json" on objects and arrays (`settingsTree.ts` 1210).

## 2. Reset and "modified"

- **T3 Code:** `SettingResetButton`, a 12px undo icon, tooltip "Reset to default", rendered only when the value differs from the default (`settingsLayout.tsx` 585). Plus a "Restore device defaults" header button with a confirm.
- **Zed:** an undo icon "Reset to Default" beside the title when the value is set in the file and differs from the default; reset deletes the key (`settings_ui.rs` 1414-1481).
- **VS Code:** a coloured bar on the row's left for a modified value (`settingsEditor2.css` 413-430), a per-row gear menu with Reset Setting; reset passes `undefined`, which deletes the key.
- **herdr, Vibe Kanban, Superset:** none per row (herdr only on Zoom; Superset by CLI).

## 3. Writing the file

| Tool | Store | Comments kept | Broken file | Raw-file link |
|---|---|---|---|---|
| cmux | Hand-edited `~/.config/cmux/cmux.json`, JSONC | Yes: lossless JSONC edits, a `.cmux-write.lock` sidecar (`CmuxSettingsFileStore+Live.swift` 55-62, `docs/config-transactions.md`) | Unverified | "cmux.json" category with "Open cmux.json" row; palette entry |
| herdr | Hand-edited `settings.json` | Unverified (plain JSON parse) | Refuses: "settings.json has errors: fix it first"; writes only if the file is unchanged since read, 3 retries (`main.js` 115-148) | None; the file is handed to agents |
| Zed | Hand-edited `settings.json`, JSONC | Yes: diffs old and new values, applies minimal tree-sitter text edits (`update_value_in_json_text`) | Refuses: "Settings file could not be parsed. Fix syntax errors before updating."; load-time notice with "Open Settings File" | "Edit in settings.json" button in the page header; cmd-alt-, |
| VS Code | Hand-edited `settings.json`, JSONC | Yes: `setProperty` from `jsonEdit.ts`, one edit range (`configurationEditing.ts` 194-251) | Refuses (`hasParseErrors`, 501-509), message points to the file | "Open Settings (JSON)" icon top-right |
| T3 Code | Machine-owned `client-settings.json` + server `settings.json` | n/a | Falls back to defaults | Only `keybindings.json` |
| Superset | Local SQLite; CLI `superset settings get/set/reset` | n/a | n/a | None |
| Conductor | App store; dropped its `settings.toml` files, imported once ([reference](https://www.conductor.build/docs/reference/settings)) | n/a | n/a | None |

**Two camps.** Tools whose users hand-edit a file (cmux, herdr, Zed, VS Code) have the page write that same file in place, keep comments, and refuse to touch a broken file. Tools that never had a hand-edited file (T3 Code, Superset, Vibe Kanban, Conductor) keep an app-owned store. None splits one setting across both. Studio's file is hand-edited JSONC, so it sits in the first camp. **Inference.**

## 4. ⌘, and the entry

- **⌘, opens the page** in T3 Code, cmux, herdr, Superset, Codex, Zed and VS Code (by default). Zed keeps a second action for the raw file (`zed::OpenSettingsFile`, cmd-alt-,); VS Code a setting (`workbench.settings.editor`: `ui` or `json`).
- **An app-menu "Settings…"** item: T3 Code, cmux, herdr, Superset, Codex, Conductor.
- **The visible button:**
  - Sidebar footer gear row: T3 Code (`SidebarChrome.tsx` 253-300), herdr (`workspace-chrome/main.js` 44; icon only when the rail is collapsed). Studio's gear today (#159) copies these.
  - Side-nav Settings button: Conductor.
  - **Top bar, right-hand icon group:** Vibe Kanban only, a gear between Reload and the command-bar button, hidden on one page (`Navbar.tsx` 362-372).
  - Top-bar dropdown item: Superset (organization dropdown), Zed (user menu: Settings, Keymap, Themes; `title_bar.rs` 1396-1408).
  - VS Code: the Manage gear at the bottom-left of the activity bar.
- **Narrow windows:** no tool's top-bar gear behaviour when narrow was found.
- **Command palette entry:** cmux, herdr, Superset.

## 5. Leaving

Esc closes it in T3 Code, herdr, Vibe Kanban and Superset. herdr closes an open sub-picker first; Superset ignores Esc while typing in a field or with a popover open. T3 Code also has an X with tooltip "Close (Esc)" and closes on a backdrop click, blurring the focused field first so a commit-on-blur field saves (`AppRouteModal.tsx` 43-52). Superset's "Back" link returns to the route it came from. cmux is a window and closes like one.

## 6. Keybindings

T3 Code, cmux and Zed have a keybindings page with a raw-file link (T3: "Open keybindings.json", `KeybindingsSettings.tsx` 1640-1647). herdr has a macOS "Keyboard" page. Not researched further.

## Unverified

Whether cmux preserves comments on a broken file or closes on Esc; whether herdr's file keeps comments; Cursor's, Codex's and Claude desktop's page shape, storage and leaving; any tool's narrow-window gear.
