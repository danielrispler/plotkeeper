# Round 6: Daniel's final checks

Every check in round 6 (#565) that needs Daniel, collected in one place. He does them all at once at the end, on the new build. A ticket that adds a by-hand check adds a section here.

## #567: one selection everywhere

- [ ] Start 3 sessions in the studio. Switch between them by clicking rows, then by Tab and Enter on the rows. The terminal, its title bar and the rail never show two different sessions at once, and the rail never draws an in-between layout.
- [ ] Select a session that runs in another terminal (started outside the studio). The terminal's place says "Runs in another terminal. Open here resumes it." with an "Open here" button, not the login shell. "Open here" resumes it in a new pane, and the rail stays on it.
- [ ] Start a new session from the picker. Until Claude Code reports its start, the rail shows no other session; then the rail and the list select the new one.
- [ ] While one session is shown, let another ask a question. Its row and the needs-you count rise; nothing is typed into or answered in its hidden terminal.
