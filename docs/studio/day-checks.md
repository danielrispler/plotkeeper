# Daniel's day of checks

Every by-hand check that is left, in one list, in the order Daniel meets them during one normal working day in the studio. It merges the round 5 checks (#517), the round 6 checks (#565), #521's live check, the real model calls (`real-calls.sh`), #578's evidence and the test-day note for #471 and #321. A ticket that adds a by-hand check adds it here, at the point in the day where it happens.

Tick a box when the check holds. When one fails, note it under the item and report it on its ticket. The day ends with one note on #599 (section 6).

Two tickets share the list. #598 runs every check that can be staged, in a guided session before the day; #599 is the day of use and takes the rest. Each section names its ticket.

The day itself is #321's test ([resolution, "Done when"](https://github.com/danielrispler/Falinks/issues/321#issuecomment-6493004697)): one normal day on Falinks that includes coding, with no other terminal app open. An editor's built-in terminal counts as one.

## 1. Before work: build, upgrade, real calls (#598)

### Build the new app

In any terminal, for the last time today:

```sh
cd /Users/danielrispler/work/Falinks
git switch main && git pull --ff-only
cd apps/studio
npm ci
npm start
```

`npm start` builds `Studio.app` and `studio-hook`, then opens the app. Quit the old app first if one runs. From now on, work only in the studio's terminals.

- [ ] **#555:** if the app offers a daemon upgrade, accept it. Every terminal revives.

  2026-11-11 (#598): no upgrade was offered. `PROTOCOL` is still 1, so the Nov 6 daemon (from before #555) kept running, and its folder and socket stayed `rwxr-xr-x`. Its fixes apply only once a new daemon starts.
- [ ] **#555:** the data folder is `drwx------` and the socket `srw-------`:

  ```sh
  ls -la ~/Library/Application\ Support/dev.falinks.studio/
  ```

- [x] **#466:** the bundle places and signs `Contents/MacOS/studio-terminal-daemon`:

  ```sh
  codesign --verify --verbose /Users/danielrispler/work/Falinks/apps/studio/target/release/bundle/macos/Studio.app/Contents/MacOS/studio-terminal-daemon
  ```

- [ ] **#464:** a rebuild that brings no daemon upgrade leaves the daemon running: its pid is the same before and after `npm start`.

### Start the real model calls

In a studio terminal, outside auto mode:

```sh
cd /Users/danielrispler/work/Falinks
apps/studio/real-calls.sh
```

It writes every output to a new `local-evidence/2026-11-09-round-5/real-calls-<time>/` folder, with its own terminal output in `run.log`. It runs on Daniel's subscription and takes a few minutes; carry on while it runs. It also rebuilds the `StudioRepro` app for #543 and quits, kills and crashes it. Dismiss the "quit unexpectedly" dialog if it shows; the live app is never touched.

Check the results when it ends:

- [x] **#518:** `518-real-card.txt` carries no plugin footer; the script greps for one and says so in `run.log`.
- [ ] **#583, the quiz before and after:** compare the new `526-summaries.md` with the earlier run's `local-evidence/2026-11-09-round-5/real-calls-20261110-153629/526-summaries.md`.
  - An orchestration turn (a merge, or an `/implement-spec` status) gets no learning section.
  - The other turns' questions ask why or how something works, and no answer is guessable by its length.
  - A guessable key is asked once more (**Calls:** 2). A second guessable reply shows as "**No learning section.**" under a "**Logged:**" line naming it. A turn with nothing to teach shows "**No learning section.**" with no logged line.
  - Weak questions are the trigger to try a larger model, which reopens #526's decision 14 (Daniel's call).
- [ ] **#526:** judge the summaries, and record their time and cost on #526.

### #521: the status line wrapper

- [x] Start a session in the studio. Find its `claude` process and check `--settings` wins over `~/.claude/settings.json`:

  ```sh
  pgrep -fl 'claude' | grep -v grep
  ps -o args= -p <claude pid>
  ```

  The arguments show `--settings` with `workbench/statusline.sh`. The session's usage file updates after a turn:

  ```sh
  ls -l ~/Library/Application\ Support/dev.falinks.studio/usage/
  ```

  The status line looks the same as before, padding included. If `--settings` does not win, stop and report on #521: that reopens its decision 5.
- [ ] Then, with Daniel's approval, remove the #481 line from `~/.claude/statusline-command.sh`. It is the line copied from `apps/studio/workbench/statusline-usage.sh`, line 4, after `input=$(cat)`. Check the top bar's usage still updates from a studio-started session, and that it matches `/usage`.

## 2. First look at the new window (#598)

Before opening any work, with the app maximised on the laptop screen.

- [ ] **#566:** the window reads calmer: sections are told apart by fill, not lines, and the terminal reads as the main thing. Only "needs you" is in colour.
- [ ] **#566:** the native title bar is gone. The traffic lights sit at the top bar's start and do not cover anything; dragging the top bar moves the window, and double-clicking it zooms it.
- [ ] **#566:** every button reacts to hover and press. Tabbing to a button or field shows a grey ring, never macOS blue. Scrollbars in the list and the rail are thin.
- [ ] **#573:** maximised, with one terminal, the list shows in full, the rail stays on screen, and the terminal has at least 100 columns (hover its header, with nothing selected, for the grid size).
- [ ] **#573:** narrow the window slowly. The list folds to its strip first, then the rail narrows, then the terminal loses columns, and only last does the rail become an overlay. In the overlay, the top bar shows the selected session's state, and its open question when it needs you.
- [ ] **#572:** at strip width, no two strip labels read the same. Hovering the strip opens the list over the terminal; the terminal does not move or resize.
- [ ] **#524:** saving `settings.json` changes zoom and font size live. ⌘=, ⌘- and ⌘0 reach the window and are not taken by a menu.

## 3. Morning: start the day's sessions (#599, except where a section names #598)

Open the sessions for the day's work: at least two in parallel, one in a task worktree, and one in auto mode.

### Starting and switching (#599)

- [ ] **#575:** ⌘N opens the new-session picker. It only starts sessions: no Remove buttons and no branch-name field (#576).
- [ ] **#570:** the picker opens with its rows; no "Reading worktrees…" box flashes first.
- [ ] **#567:** start a session from the picker. Until Claude Code reports its start, the rail shows no other session; then the rail and the list select the new one.
- [ ] **#541:** in the picker, "Auto mode" is unticked. Tick it and start a session: it runs in auto mode (its footer says so), and after its first prompt the rail's line reads "Running auto mode …". No bypass option is offered anywhere. A session started without the tick shows no mode.
- [ ] **#567:** with 3 sessions running, switch between them by clicking rows, then by Tab and Enter on the rows. The terminal, its title bar and the rail never show two different sessions at once, and the rail never draws an in-between layout.
- [ ] **#570:** switching feels immediate. The row shows selected (a fill and an edge bar) as the mouse goes down, before the rail and the terminal follow. After a switch, the terminal shows at once at its size and scroll position, with no second or third position.
- [ ] **#570:** Tab to a row, or select text in a row, and wait for a few updates: the focus ring and the selection stay.
- [ ] **#565 (6a fix):** press a row, then start a new session before the row's reply lands: the new session's pane stays.
- [ ] **#565 (6a fix):** with a session selected, the terminal has no empty strip above it. With nothing selected, the header shows the title.

### One row per conversation (#598)

- [ ] **#562:** open a new Claude terminal, run `claude`, and pick an earlier session with `/resume`. Do it twice. The list shows that session as one row, with its history, and no extra "Falinks" row with "No step yet". Open `/resume` once more and back out with Esc: no new row appears. A session you switch to shows as a row once you send it a prompt.
- [ ] **#565 (6a fix):** in a session the studio started, `/resume` another: the resumed session stays selected in the list and the rail, and its pane stays.
- [ ] **#567:** select a session that runs in another terminal (started outside the studio, for example in Zed's terminal before the day began). The terminal's place says "Runs in another terminal. Open here resumes it." with "Open here", not a login shell. "Open here" resumes it in a new pane, and the rail stays on it.
- [ ] **#474:** a live `/clear` in a studio terminal keeps one row, with its name. "Open" afterwards resumes the new conversation. **#472:** after the `/clear`, at most one spurious "Steps only" mark shows, for one poll.
- [ ] **#469:** close a running session. No process of its group is left (the daemon's `list` and `ps`), and its row ends.
- [ ] **#469:** rename a session, then quit and reopen the app: it keeps its name.

### Rows, facts and the rail at a glance (#599)

- [ ] **#572:** each row is one line: a running session shows a turning spinner, your turn a dot, idle and ended a ring. Only running and needs-you rows have a second, muted line. Hovering a row names its state in words.
- [ ] **#571:** with a session selected, its name shows only on its row, not in the top bar or the terminal's header. In the strip, hovering its button names it.
- [ ] **#571:** only the selected row shows a time ("Turn 12m"). No clock shows seconds or changes more than once a minute. The rail shows "Step" only once a step has run a minute.
- [ ] **#571:** no "no estimate", "Step 0s", grid size such as `169 × 38`, or raw `**` anywhere. A long path shows its last 2 segments; hovering shows the full line. A row whose line had a link shows the link's text, and hovering shows the URL.
- [ ] **#519:** a row and the rail show `context N%` with the same number as Claude's status line. The hover reads "… tokens used · Claude's status line shows this as M% left".
- [ ] **#523:** the rail header shows the model and effort, such as `claude-opus-5-5 · effort medium`, on hover.
- [ ] **#574:** select a session with a few turns and cards, without scrolling the rail: the newest card is at the top, then the boxed intention and the latest turn's summary.
- [ ] **#574:** the rail's header is one line, the verdict and the phase; hovering shows the times and the model. Rename, Close and Delete (and Open all, Close all) are in its "⋯" menu, which closes on a choice or a click outside.
- [ ] **#574:** below, the turns read newest first, each with its prompt in one line, its summary, its learning question (folded), its cards, subagents, checks and full response (folded). Opening an older turn's full response shows its final reply. Each turn shows one "Ask about this", only on hover, and asking works as before.
- [ ] **#472:** a working session's intention updates live; judge the dimming.

### Keys and the palette (#598)

- [ ] **#575:** ⌘⇧P opens the palette: every action with its key. Typing filters, Enter runs the first match, Esc closes it. It lists focus mode with ⇧⌘↩, "Toggle split" with ⌘D and "Toggle panel" with ⌃`.
- [ ] **#575:** Tab into the list; ↑ and ↓ move the selection row by row. The terminal still gets ↑ and ↓ when it has focus. Ctrl+C and Ctrl+R still reach Claude Code.
- [ ] **#575:** ⌘, opens `settings.json` in the editor, creating it empty if it was missing.
- [ ] **#575:** rebind one key in `keymap.json`, such as `{ "bindings": { "cmd-shift-j": "studio::NextNeedsYou" } }`. The new key works, ⌘J no longer does, and the palette shows ⇧⌘J. Undo it after.
- [ ] **#465:** ⌘1, ⌘2 and Esc work in the real window. Panes and dialogs (the picker, the palette) draw correctly.

### Terminal parity with Zed's (#598)

With a real `claude` session in the pane (#466, #464):

- [ ] Ctrl+V and Cmd+V paste a screenshot as an `[Image #N]` chip. Cmd+V pastes text, a long paste included.
- [ ] Cmd+C copies; Ctrl+C interrupts. Esc, Shift+Enter, Shift+Tab and Option as Meta work.
- [ ] Cmd+F searches. Resizing and scrollback work, and Claude's window title appears.
- [ ] Five terminals at once show no lag. OSC 7501 status switches on in a live `claude`.
- [ ] `studio-terminal-daemon attach <terminal>` works from another terminal app. A replay into a window of another size looks right in the visible pane.

### Doctor and skills (#598)

- [ ] **#527:** select a session in a task, open the palette and type "doctor". It lists "Doctor: Check the installation" marked "no tokens" and "Doctor: Full checkup" marked "starts a Claude session, spends tokens". There is no Doctor button anywhere.
- [ ] **#527:** run each. Both open a new terminal in the selected session's folder: the first runs `claude doctor` and leaves a shell; the second starts `claude` on `/doctor`. With nothing selected, both run in the repository.
- [ ] **#527:** add your own entry under `doctor` in `settings.json`: on save, the palette lists it in place of the defaults, with its kind's warning.
- [ ] **#565 (6c fix):** add a Doctor `shell` entry whose command fails, `"command": "(exit 3)"`. A notice names the entry's label with code 3, and the shell stays open. Remove the entry after.
- [ ] **#528:** ⌘⇧P, then "Skills". The browser takes the rail's place, grouped as "Your skills", "This project" and one group per plugin. Opening a skill shows its `SKILL.md` rendered. Esc brings the rail back.
- [ ] **#528:** "Edit" on one of your skills opens its `SKILL.md` in Zed. Plugin skills have no "Edit".
- [ ] **#592:** compare the browser's plugin groups with `/plugin` in Claude Code, in the same project. Each enabled plugin shows, at the version `/plugin` names; a plugin turned off there is missing. Turn one off in `/plugin`, reopen "Skills": its group is gone. Turn it back on after. Note any difference on #592.
- [ ] **#565 (6c fix):** symlink a skill folder from outside `~/.claude/skills` into it. The browser does not list it. Remove the link after.

## 4. During the work (#599, except where a section names #598)

### Questions and "needs you" (#599)

- [ ] **#575, a loop without the mouse:** with two sessions asking a question, press ⌘J. The first one that needs you is selected. Tab into the list and press 1: its question is answered. With the terminal focused, 1 types "1" and the card stays open. Press ⌘J for the second, answer it, then ⌘] and ⌘[ to step between sessions.
- [ ] **#572:** with several sessions open, which one needs you is clear at a glance: it is first, with the only coloured mark (the "Needs you" pill), and the top bar's chip says "1 needs you". Clicking the chip selects it. In the strip, its dot is coloured.
- [ ] **#567:** while one session is shown, let another ask a question. Its row and the needs-you count rise; nothing is typed into or answered in its hidden terminal.
- [ ] **#569:** a question, a plan and a permission each dock within the read loop's 200 ms, not after up to 1.2 s. Time the worst of a few, from the terminal's prompt to the card, and record it on #569 beside 1.2 s.
- [ ] **#574:** a question docks at the top of the rail, under the header, and stays there while the rail scrolls. Once answered, the newest card is back at the top.
- [ ] **#540:** a reply ending with a question shows "your turn", not "needs you". A session waiting on its subagents is not counted as needing you.
- [ ] **#541:** in the auto-mode session, ask for something auto mode's classifier blocks: the row shows **needs you** and the toast fires, as in default mode.
- [ ] **#560:** run `agy` or `codex` in a studio terminal. A row named after it and its folder appears, marked "status only", with no step line. Ask for something that needs a permission: the row turns **needs you** and the toast fires; once answered and finished, it shows **your turn**.

### Focus mode, the toast and sounds (#599)

- [ ] **#573:** ⇧⌘↩ folds the list and the rail and gives the terminal the window; ⇧⌘↩ again brings them back.
- [ ] **#573:** in focus mode, let another session ask a question. A toast shows its name and the question's first line over the terminal's top-right corner, with a sound, and typing in the terminal goes on: nothing moves and no key is lost. A second block shows "+1 more" in the same toast.
- [ ] **#573:** click the toast: focus mode ends and that session is selected. Its × dismisses it without taking the terminal's focus. In focus mode, ⌘J also jumps to the toast's session and leaves focus mode.
- [ ] **#573:** a question from the session in the focused terminal shows no toast and makes no sound.
- [ ] **#556 and the 6c fix:** focused, another session that newly needs you plays one sound (Glass) as its toast shows, in any layout, and never twice. The session you are viewing plays none, and a your-turn plays none.
- [ ] **#556:** in the background, needs you plays Glass and your turn plays Tink, each only with its notification. The sounds feel right, not alarming.
- [ ] **#522:** a real "your turn" notification while you are in another app. Take a screenshot for #522.
- [ ] **#522:** with Remote Control on (`remoteControlAtStartup`), in a session the studio started: no phone push while the studio is focused, and a push while it is in the background. `~/Library/Application Support/dev.falinks.studio/present` exists only while the window is focused:

  ```sh
  ls ~/Library/Application\ Support/dev.falinks.studio/present
  ```

- [ ] **#522:** a turn driven from the phone still sends hook events: its row and card update in the studio.

### Unseen (#599)

- [ ] **#556:** a session whose turn ends while you look at another shows a filled dot and a bold name, in the list and the strip. Selecting it with the window focused clears the mark; selecting it while another app has focus does not.
- [ ] **#565 (6c fix):** let a turn end while focus mode is on, the rail is a closed overlay, or the skills browser shows: the session stays unseen. Show the rail on it: the mark clears.
- [ ] **#556:** "Mark unseen" in the rail's ⋯ menu brings the mark back until you select the session again. ⌘J with no session needing you goes to the next unseen one.

### Turns, cards and summaries (#599)

- [ ] **#518:** one card written by the app itself, with no plugin footer.
- [ ] **#520:** a finished turn's intention is its final reply's first line, not a footer.
- [ ] **#526:** after a long turn ends, its intention first shows the reply's first line, then switches once, about 10 s later, to the turn summary's first line. With `"summaries": { "on_open": true }` in `settings.json`, it stays the reply's first line until the session is opened.
- [ ] **#574:** with summaries written on open, opening a session writes its waiting turns' summaries one at a time, and not again on each redraw.
- [ ] **#525:** a table in a response renders. Clicking a link opens the browser, and the studio's window stays on the app.
- [ ] **#568:** a turn that only writes a `.DS_Store` (open its folder in Finder) gets no card.
- [ ] **#544:** renaming a session with a half-typed draft leaves the draft intact: the rename waits, and after 60 s shows a notice instead of typing. Once the draft is sent or cleared, `/rename` arrives about 3 s later. Renaming a session in "needs you" shows a notice and types nothing.
- [ ] **#475:** the phase rules match your sessions (a `ci-checks` run before any implementing counts as orienting), and "kind" (first slash command, else first skill) is the right grouping.

### Cards and answers, staged (#598)

- [ ] **#462:** with the app started with `open --stderr <file>` (to keep writer errors), a turn that changes a worktree produces a card in `cards-v2.json`.
- [ ] **#486:** a turn that ends while the app is down gets its card after the app starts.
- [ ] **#476:** one real card on a `ci-checks` run: it is ready before the check ends, and the check's exit code settles "Will `<command>` pass?". A check whose exit code does not count (such as `cargo test 2>&1 | tail`) stays not counted; a subagent's check gets no card.
- [ ] **#473:** a failed background check ends its time.
- [ ] **#477:** a live question answered from the card, and one answered in the terminal; neither kills the turn. A typed "Other" answer works. Press Esc while the answer hook waits, and record what happens.
- [ ] **#478:** a live plan and a live permission answered from the card, in auto mode too. A plan rejected in the terminal shows "Rejected" in the decision history.
- [ ] **#479:** the decision history survives an app restart.
- [ ] **#482:** "Ask about this" on a real response gets a grounded reply, and the session's transcript gains nothing.

### A run that spawns workers (#599)

Run one `/implement-spec` (or any run with workers and helpers) in the studio.

- [ ] **#535:** when each subagent finishes, its row leaves the list and its parent counts it in "N done". A failed one stays, marked failed, until you open it.
- [ ] **#535 / #574:** each turn's subagents show inside the turn that ran them, with name, prompt's first line, result, type, duration and kind.
- [ ] **#535:** open a finished subagent. Within seconds, its written summary shows on its intention line, and on the entry when you go back. Opening it again writes no new summary (no new `claude -p` run).
- [ ] **#562:** if Claude Code forks the session mid-conversation, the list keeps one row under the session's name, and it does not stay "Running" after the turn ends. Delete a row: only that row goes.

### Tasks, Places and file links (#599)

- [ ] **#576:** open Places at the bottom of the list. It lists the main checkout and each task, one row each; the sessions stay in the list above. Each row's session count matches the sessions in that folder.
- [ ] **#576:** on a real task with commits and edits, the counts match git, run in that task's folder:

  ```sh
  git status --porcelain | wc -l                                  # changed
  git rev-list --left-right --count HEAD...origin/HEAD            # ahead, behind (a task)
  git rev-list --left-right --count HEAD...@{upstream}            # ahead, behind (the main checkout)
  ```

  A clean task level with `main` shows no counts.
- [ ] **#576:** create a task from Places' field: a session starts in it and its row appears. Click a task: "New session here" starts a session in it. Remove asks first when it would lose work, then removes the worktree.
- [ ] **#557:** from a task session, clicking `path:line` in the terminal and a file link in the rail opens that worktree's file, not the main checkout's, at that line in Zed.
- [ ] **#581:** in a rail reply from a task session, inline code such as `` `src/foo.rs:42` `` looks like plain code until hovered, and a click opens that worktree's file at that line. `` `origin/main` `` is not a link.

### Splits and the panel (#598)

- [ ] **#529:** with two sessions running, select one and press ⌘D. Its pane stays and the next session shows beside it, the selected one nearer the rail. Both take input. The rail follows the selection, not the pinned pane.
- [ ] **#529:** on a wide screen (each pane at least 100 columns) they sit side by side. Narrow the window: they stack before the rail becomes an overlay. Focus mode puts them side by side again when the window has room.
- [ ] **#529:** drag the divider: both resize, and Claude Code redraws in each; it stops before a pane gets too small. Selecting the pinned session shows it alone; selecting another brings the split back. ⌘D again closes it. Dropping a file types its path into the focused pane only.
- [ ] **#542:** press ⌃` with a session selected. A band opens under the terminal with a shell in that session's folder, and takes the keys; the terminal above stays as wide as before. ⌃` again, from inside the panel too, hides it.
- [ ] **#542:** the band never covers the list or the rail. Drag its top edge: it stops at 40 % of the terminal's height. Select a session in another folder: the panel shows that folder's shell; going back shows the first shell with its scrollback. With a split open, the panel shows under both panes.
- [ ] **#542:** rebind it in `keymap.json` (`{ "bindings": { "ctrl-j": "studio::TogglePanel" } }`): the new key toggles it, and ⌃` reaches the terminal. Undo it after.

### A program that fails at start (#598)

- [ ] **#558:** make `claude` fail at start for one session. The studio starts sessions through a login shell, so put a failing script first on `PATH` in `~/.zprofile`:

  ```sh
  mkdir -p /tmp/fail-bin && printf '#!/bin/sh\necho broken >&2\nexit 7\n' > /tmp/fail-bin/claude && chmod +x /tmp/fail-bin/claude
  echo 'export PATH=/tmp/fail-bin:$PATH  # day-checks #558' >> ~/.zprofile
  ```

  Start a session from the picker. Within 2 s, the terminal's place says "claude could not start: code 7" above its last lines, with "Show output". A toast at the bottom-right names it and the code, takes no focus, and nothing moves. Then remove the line again:

  ```sh
  sed -i '' '/# day-checks #558$/d' ~/.zprofile
  ```
- [ ] **#558:** a hidden pane that fails at start leaves the shown terminal in place; only the toast appears, and its "Show output" shows the failed pane. Two failures show two notices, one under the other.
- [ ] **#565 (6c fix):** a panel shell that fails at start (for example in a deleted worktree): its notice's "Show output" opens the panel, not the main column.
- [ ] **#558:** a program that exits with code 0, or after running a while, shows no notice.

### #578's evidence, mid-day (#599)

Save everything in a new folder: `local-evidence/<date>-ui-ux-recheck/` (for example `local-evidence/2026-11-12-ui-ux-recheck/`), and add it to `local-evidence/README.md`.

- [ ] Screenshots of a normal moment (⇧⌘4, then Space to capture the window):
  - the full window, maximised;
  - half width;
  - the rail on a session with a long turn;
  - Places, open;
  - the palette (⌘⇧P);
  - focus mode with a needs-you toast showing.
- [ ] One recording (⇧⌘5, with **Options → Show Mouse Clicks** on): switch between 3 sessions, press ⌘2 to open the rail where it is an overlay, open a card, then start a new session and wait until Claude's prompt is ready.
- [ ] If you timed the five tasks in `local-evidence/2026-11-09-ui-ux/ux-expert.md`, section 8, before round 6: repeat them and write the times beside the old ones.

### The mid-day rebuild (#599; #561's check is #598's)

- [ ] **#321 / #470:** with a session mid-turn and another showing "needs you", rebuild:

  ```sh
  cd /Users/danielrispler/work/Falinks/apps/studio && npm start
  ```

  Choose "Restart into it". No session is lost, the "needs you" state is still there, and the panel's shell is back with its scrollback (#542).

- [ ] **#561 (#598):** run `agy` in a studio terminal and tell it a word to remember. Revival happens only when the terminal daemon dies, so end it as a crash would:

  ```sh
  pkill -x studio-terminal-daemon
  ```

  The terminal comes back running `agy --continue`, in the same conversation: ask for the word. Its row, once `agy` reports a status, ends in "latest conversation". If the daemon that died was older than this build, it kept no foreground program and the terminal comes back as a plain shell: run the check again.

## 5. End of the day (#599)

- [ ] **#556:** quit and reopen the app. The sessions you had seen stay plain; the unseen ones keep the mark.
- [ ] **#565 (6c fix):** an unseen session left for 2 hours goes idle and keeps its filled dot and bold name until you select it.
- [ ] **#541:** end the auto-mode session, then "Open" it from the rail's ⋯ menu: it resumes in auto mode. A session started in default mode and switched to auto with Shift+Tab resumes in default mode.
- [ ] **#555:** with Claude working in a studio pane, the daemon reports its state:

  ```sh
  (echo '{"op":"list"}'; sleep 1) | nc -U ~/Library/Application\ Support/dev.falinks.studio/terminal-daemon.sock
  ```

  That terminal shows `"state":"working"` and `"app"` set, and `turn` goes up after the turn ends.
- [ ] **#555:** quitting the app deletes `present`; a CLI `attach` that detaches leaves it.
- [ ] **#470:** after a reboot, every Claude session comes back with its conversation.
- [ ] **#560:** review the mismatch log, `~/Library/Application Support/dev.falinks.studio/mismatch.log`: each line is a Claude session whose hook state and OSC 7501 state disagreed for over 5 s. Report any pattern on #560; no file means none did.

## 6. The test-day note (#599)

Count the day against #321's "Done when":

- [ ] Every agent session and shell ran in a studio terminal, with no other terminal app open.
- [ ] The mid-day rebuild lost no running session and no "needs you" state.
- [ ] At least 5 cards appeared on real waits, and you opened each one.

Then write one note on #599, in a file, and post it:

```sh
gh issue comment 599 --body-file <note.md>
```

The note records, from #471 and #483:

- what got in the way, and the parity gaps against Zed's terminal;
- the cards that appeared, and the writer's times;
- any lost session or orphan process;
- whether intentions and step text told you what was happening, and how the estimates compared with the real times;
- questions answered from cards and from the terminal, and the decision history;
- naming, usage, "ask about this", check cards and your guesses;
- whether the studio now replaces your terminal app, and your next call, including whether Hebrew in the terminal or a compose box is needed.

Then comment on #578 that its evidence is in the folder.
