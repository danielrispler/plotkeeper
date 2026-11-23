# Install and uninstall

plotkeeper records an agent's sessions through hooks, which run on each of its events. Wire them once, from any terminal:

    plotkeeper install

Install works at user level, never in a repository, so your git trees stay clean. It prints each step, and a harness it does not find on the `PATH` is skipped with one line:

- It copies `plotkeeper-hook` and `plotkeeper-answer`, from beside the `plotkeeper` executable, into the data folder's `bin/` (`~/Library/Application Support/plotkeeper/bin/`). Every hook names that folder, in quotes since the path has a space, never a checkout or an npm folder, and each `plotkeeper` start brings the copies up to the binaries beside it (#356), so an upgrade reaches the hooks at the next start.
- **Claude Code**: it renders `plugin/` with that path into `plugins/claude/` in the data folder, as a local marketplace of one plugin, and runs `claude plugin marketplace add` on it and `claude plugin install plotkeeper@plotkeeper`. `claude` writes `enabledPlugins` and `extraKnownMarketplaces` into `~/.claude/settings.json`; plotkeeper never opens that file (AGENTS.md, Hard rules), and puts no hook in any `settings.json`. The plugin shows as `plotkeeper@plotkeeper` in `claude plugin list`.
- **agy**: it renders `plugin-agy/` with the path of `plotkeeper-hook` into `plugins/agy/plotkeeper/` in the data folder, and installs it with `agy plugin validate` and `agy plugin install`, which copies it into `~/.gemini/config/plugins/plotkeeper/`. A copy already there is backed up first.
- **The old setup**: so that no event is recorded twice, it removes what the archived desktop app set up, after a backup. In `~/.zshrc`, from each `export CLAUDE_CODE_PLUGIN_DIRS=` line, the entries that end in `plotkeeper/checkout/plugin` or `dev.falinks.studio/checkout/plugin` (the data folder's link into a checkout, by its name before #322); a line left with no entry goes, and so does the app's comment above it. With agy found, agy's old plugin `studio` is uninstalled.

The backups and a record of what install did are in `install/` in the data folder. A second install keeps the first backups and replaces its own plugins. Run it again after you change `plugin/` or `plugin-agy/`: the agents run the copies install made.

To take it all back:

    plotkeeper uninstall

Uninstall runs `agy plugin uninstall plotkeeper` and `claude plugin uninstall plotkeeper@plotkeeper` and `claude plugin marketplace remove plotkeeper` for what install added, installs the agy plugins it backed up again, puts `~/.zshrc` back as it was, and removes the data folder's `bin/`, `plugins/` and `install/`, and nothing else: the event log and the stores stay. A `~/.zshrc` you changed after install is left as it is, and uninstall names the backup to take the lines from.

## What the Claude Code plugin does

On thirteen hook events it runs `plotkeeper-hook --agent claude`, which appends the event to the event log. Answering from the page is Claude Code only: `plotkeeper-answer` holds a question, a plan's approval or a permission until you answer on the page.

Two hooks carry a blocked command (spec #170). On `PermissionDenied` for Bash, `plotkeeper-answer` leaves a request for the card and waits up to 600 s for Allow or Decline (timeout 610). On `PreToolUse` for Bash it claims the one-time pass Allow left and returns `"ask"`, or returns at once with no output when no pass matches. `PermissionDenied` is also logged by `plotkeeper-hook` like every other event.
