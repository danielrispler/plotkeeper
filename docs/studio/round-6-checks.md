# Round 6: Daniel's final checks

Every check in round 6 (#565, a calm studio) that needs Daniel, collected in one place. As in round 5, he does them all at once at the end: he upgrades to the new build, uses it for a real day, and goes through this list. A ticket that adds a by-hand check adds a section here.

Between passes, nothing waits for him. After each pass, a code-review agent that did not write the code reviews it. Once its fixes are in and the automated checks pass, the pass merges to `main`.

## #566: a quiet frame

- [ ] The window reads calmer on a real day: sections are told apart by fill, not lines, and the terminal reads as the main thing.
- [ ] The native title bar is gone. The traffic lights sit at the top bar's start and do not cover the session's name; dragging the top bar moves the window, and double-clicking it zooms the window.
- [ ] Every button reacts to hover and press. Tabbing to a button or field shows a grey ring, never macOS blue.
- [ ] Scrollbars in the list and the rail are thin.
- [ ] Only "needs you" is in colour.

## #567: one selection everywhere

- [ ] Start 3 sessions in the studio. Switch between them by clicking rows, then by Tab and Enter on the rows. The terminal, its title bar and the rail never show two different sessions at once, and the rail never draws an in-between layout.
- [ ] Select a session that runs in another terminal (started outside the studio). The terminal's place says "Runs in another terminal. Open here resumes it." with an "Open here" button, not the login shell. "Open here" resumes it in a new pane, and the rail stays on it.
- [ ] Start a new session from the picker. Until Claude Code reports its start, the rail shows no other session; then the rail and the list select the new one.
- [ ] While one session is shown, let another ask a question. Its row and the needs-you count rise; nothing is typed into or answered in its hidden terminal.

## #562: one conversation shows as 4 rows in the session list

- [ ] Open a new Claude terminal, run `claude`, and pick an earlier session with `/resume`. Do it twice. The list shows that session as one row, with its history, and no extra "Falinks" row with "No step yet".
- [ ] When Claude Code forks a session mid-conversation (its `SessionStart` comes `from fork`), the list keeps one row, under the session's name, and it does not stay "Running" after the turn ends.
- [ ] Delete that row. Only that row goes.

## #570: instant feedback

- [ ] Switching sessions feels immediate: the row shows selected (a fill and an edge bar at its start) as the mouse goes down, before the rail and terminal follow. Rows and buttons react to hover.
- [ ] After a switch, the terminal does not jump: it shows at once at its size and scroll position, with no second or third position. The rail does not draw an in-between layout first.
- [ ] Tab to a row, or select text in a row, and wait for a few updates: the focus ring and the text selection stay.
- [ ] "New session" opens the picker with its rows; no "Reading worktrees…" box flashes first.
- [ ] Tapping a quiz option marks it at once.

## #571: each fact once

- [ ] With a session selected, its name shows only on its list row: not in the top bar and not in the terminal's header. In the strip, hovering its button names it.
- [ ] Only the selected row shows a time ("Turn 12m"), and no clock shows seconds or changes more than once a minute. The rail shows "Step" only once a step has run a minute.
- [ ] No "no estimate", "Step 0s", grid size such as `169 × 38`, or raw `**` anywhere on screen. Hovering the terminal's header shows the grid size.
- [ ] A row or the rail's step with a long path shows its last 2 segments; hovering shows the full line. The picker's paths read the same way.

## #565: pass 6a review fixes

- [ ] Start a session from the studio that resumes another (`/resume`): the resumed session stays selected in the list and the rail, and its pane stays.
- [ ] Press a row, then start a new session before the row's reply lands: the new session's pane stays.
- [ ] With a session selected, the terminal has no empty strip above it; with nothing selected, the header shows the title.
- [ ] A row whose line had a link shows the link's text; hovering shows the full line with its URL.
