# Round 6: Daniel's final checks

Every check in round 6 (#565) that needs Daniel, collected in one place. He does them all at once at the end: he upgrades to the new build, uses it, and goes through this list. A ticket that adds a by-hand check adds a section here.

## #562: one conversation shows as 4 rows in the session list

- [ ] Open a new Claude terminal, run `claude`, and pick an earlier session with `/resume`. Do it twice. The list shows that session as one row, with its history, and no extra "Falinks" row with "No step yet".
- [ ] When Claude Code forks a session mid-conversation (its `SessionStart` comes `from fork`), the list keeps one row, under the session's name, and it does not stay "Running" after the turn ends.
- [ ] Delete that row. Only that row goes.
