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

## #535: finished subagents leave the list; the rail keeps their history

- [ ] Run a session that spawns a worker and a helper. When each one finishes, its row leaves the list and its parent's line counts it in "N done". A failed one stays, marked failed, until you open it.
- [ ] Open the session's rail: the collapsed **Subagents** section lists every subagent it ran, newest first, each with its name, its prompt's first line, its result, and its type, duration and kind.
- [ ] Click a finished entry: the rail opens that subagent with its cards and brief. Within seconds, its written summary of what it was asked and what it came back with shows on its intention line, and on the entry when you go back. Opening it again writes no new summary (no new `claude -p` run). Outside auto mode.

## #573: the width rule, focus mode and the needs-you toast

- [ ] On the laptop screen, maximised, with one terminal: the list shows in full, the rail stays on screen, and the terminal has at least 100 columns (hover its header for the grid size, with nothing selected).
- [ ] Narrow the window slowly: the list folds to its strip first, then the rail narrows, then the terminal loses columns, and only last does the rail become an overlay. In the overlay, the top bar shows the selected session's state, and its open question when it needs you.
- [ ] ⌘⇧↩ folds the list and the rail, and the terminal takes the window; ⌘⇧↩ again brings them back. Rebinding `studio::ToggleFocus` in `keymap.json` moves it to the new key.
- [ ] In focus mode, let another session ask a question. A toast shows its name and the question's first line over the terminal's top-right corner, with a sound, and you keep typing in the terminal: nothing moves and no key is lost. A second block shows "+1 more" in the same toast.
- [ ] Click the toast: focus mode ends and that session is selected. Its × dismisses it without taking the terminal's focus.
- [ ] A question from the session in the focused terminal shows no toast and makes no sound.
- [ ] The sound feels right, not alarming.

## #575: a keyboard spine and a palette

- [ ] A whole loop without the mouse: with two sessions asking a question, press ⌘J. The first one that needs you is selected. Tab into the list and press 1: its docked question is answered. With the terminal focused, 1 types "1" into it and the card stays open; in focus mode, or with the rail overlay closed, 1 from the list does nothing. Press ⌘J again for the second one (in focus mode, ⌘J jumps to the toast's session and leaves focus mode), answer it, then ⌘] and ⌘[ to step to the next and previous session.
- [ ] Tab into the session list, then ↑ and ↓ move the selection row by row; the terminal still gets ↑ and ↓ when it has focus.
- [ ] ⌘N opens the new-session picker; ⌘, opens `settings.json` in your editor, creating it empty if it was missing.
- [ ] ⌘⇧P opens the palette: every action with its key. Typing filters it, Enter runs the first one, and Esc closes it. Focus mode is listed with ⇧⌘↩.
- [ ] Rebind one key in `keymap.json`, such as `{ "bindings": { "cmd-shift-j": "studio::NextNeedsYou" } }`: the new key works, ⌘J no longer does, and the palette shows ⇧⌘J.
- [ ] Ctrl+letters (Ctrl+C, Ctrl+R) still reach Claude Code in the terminal.

## #572: one-line rows, status by shape, a needs-you chip

- [ ] With 10 sessions open, at full width, which one needs you is clear at a glance: it is first, with the only coloured mark (the "Needs you" pill), and the top bar's chip says "1 needs you". Clicking the chip selects it.
- [ ] Narrow the window until the list is a strip. The needs-you session is still clear at a glance (the coloured dot), and no two strip labels read the same. Hovering the strip opens the list over the terminal; the terminal does not move or resize.
- [ ] Each row is one line: a running session shows a turning spinner, your turn a dot, idle and ended a ring. Only running and needs-you rows have a second, muted line. Hovering a row names its state in words.

## #576: Places, a home for tasks (with #559's counts)

- [ ] Open Places at the bottom of the list. It lists the main checkout and each task, one row each; the sessions themselves stay in the list above.
- [ ] On a real task with commits and edits, the row's counts match git, run in that task's folder: `git status --porcelain | wc -l` for "changed", and `git rev-list --left-right --count HEAD...origin/HEAD` for "ahead" and "behind" (for the main checkout, `HEAD...@{upstream}`). A clean task level with `main` shows no counts.
- [ ] Each row's session count matches the sessions working in that folder.
- [ ] Click a task: "New session here" starts a session in it; Remove asks first when it would lose work, then removes the worktree. Create a task from Places' field: a session starts in it and its row appears.
- [ ] The new-session picker (⌘N) only starts sessions: it has no Remove buttons and no branch-name field.

## #574: the rail opens on now, as a feed of turns

- [ ] Select a session with a few turns and cards, without scrolling the rail: the newest card is in view at the top, then the boxed intention and the latest turn's summary. (In the recording, reaching a card took 7 s of scrolling.)
- [ ] Let the session ask a question: it docks at the top of the rail, under the header, and stays there while the rail scrolls. Once answered, the newest card is back at the top.
- [ ] The rail's header is one line: the verdict and the phase. Hovering it shows the times and the model. Rename, Close and Delete (and Open all, Close all) are in its "⋯" menu, which closes on a choice or a click outside.
- [ ] Below, the turns read newest first, each with its prompt in one line, its summary, its learning question (folded), its cards, its subagents, its checks and its full response (folded). Opening an older turn's full response shows its final reply. No "Last 30 minutes", "Older", "Responses", "Questions" or "Subagents" section is left; #535's subagent entries are in the turn that ran them.
- [ ] Each turn shows one "Ask about this", only on hover; asking about a turn works as before.
- [ ] With summaries written on open (settings), opening a session writes its waiting turns' summaries once, not again on each redraw.

## #529: split terminals

- [ ] With two sessions running in the studio, select one and press ⌘D: its pane stays and the next session's terminal shows beside it, the selected one nearer the rail. Both take input: click into each and type. The rail follows the selection, not the pinned pane.
- [ ] On a wide screen (each pane at least 100 columns), the two sit side by side. Narrow the window: they stack, one above the other, before the rail turns into an overlay. Focus mode (⌘⇧↩) puts them side by side again when the whole window has room.
- [ ] Drag the divider between the panes: both resize, and Claude Code redraws in each at its new size; the divider stops before a pane gets too small.
- [ ] Selecting the pinned session shows it alone; selecting another brings the split back. ⌘D again closes the split. Closing the pinned session's terminal closes the split.
- [ ] ⌘⇧P lists "Toggle split" with ⌘D. Dropping a file types its path into the focused pane only.

## #558: a program that fails at start shows its exit code and last lines

- [ ] Start a session in a folder where `claude` cannot start (for example, with a failing `claude` script first on `PATH`). Within 2 s, the terminal's place says "claude could not start: code <n>" above the program's last lines, with "Show output", which shows its pane and full output.
- [ ] At the same time, a toast at the terminal's bottom-right corner names the session (or program) and the code, with "Show output" and ×. Typing in the terminal goes on: the toast takes no focus and nothing moves.
- [ ] Let a hidden pane fail at start, for example a revived terminal or a Doctor entry while another pane shows. The shown terminal stays; only the toast appears, and its "Show output" shows the failed pane.
- [ ] A session that runs a while and then exits, and a program that exits with code 0, show no notice: only the pane's usual exit line.

## #541: start a session in auto mode

- [ ] Open the picker (⌘N): "Auto mode" is unticked. Tick it and start a session: it runs in Claude Code's auto mode (its footer says so), and after its first prompt the rail's line reads "Running auto mode …". No bypass option is offered anywhere.
- [ ] In that session, ask for something auto mode's classifier blocks: the row shows **needs you** and the toast fires, as in default mode.
- [ ] A session started without the tick shows no mode on the rail's line.
- [ ] End the auto session, then "Open" it from the rail's "⋯" menu (or "Open here"): it resumes in auto mode. Restarting the terminal daemon (#470) also brings it back in auto mode.
