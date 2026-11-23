# Install and uninstall

plotkeeper records an agent's sessions through hooks, which run on each of its events. Wire them once, from any terminal:

    plotkeeper install

Install works at user level, never in a repository, so your git trees stay clean. It prints each step, and a harness it does not find on the `PATH` is skipped with one line:

- It copies `plotkeeper-hook` and `plotkeeper-answer`, from beside the `plotkeeper` executable, into the data folder's `bin/` (`~/Library/Application Support/plotkeeper/bin/`). Every hook names that folder, in quotes since the path has a space, never a checkout or an npm folder, and each `plotkeeper` start brings the copies up to the binaries beside it (#356), so an upgrade reaches the hooks at the next start.
- **Claude Code**: it renders `plugin/` with that path into `plugins/claude/` in the data folder, as a local marketplace of one plugin, and runs `claude plugin marketplace add` on it and `claude plugin install plotkeeper@plotkeeper`. `claude` writes `enabledPlugins` and `extraKnownMarketplaces` into `~/.claude/settings.json`; plotkeeper never opens that file (AGENTS.md, Hard rules), and puts no hook in any `settings.json`. The plugin shows as `plotkeeper@plotkeeper` in `claude plugin list`.
- **agy**: it renders `plugin-agy/` with the path of `plotkeeper-hook` into `plugins/agy/plotkeeper/` in the data folder, and installs it with `agy plugin validate` and `agy plugin install`, which copies it into `~/.gemini/config/plugins/plotkeeper/`. A copy already there is backed up first.
- **Codex**: Codex has no command for its hooks, so install adds plotkeeper's entries to its user-level hook file, `~/.codex/hooks.json`, after a backup, beside any hooks of your own there, and makes the file when there is none. They are rendered from `plugin-codex/hooks.json`: on eight events (`SessionStart`, `SessionEnd`, `UserPromptSubmit`, `PreToolUse`, `PermissionRequest`, `PostToolUse`, `Stop`, `Interrupt`) Codex runs `plotkeeper-hook --agent codex`. Codex runs a hook only once you have trusted it, so install ends with that step: run `/hooks` in Codex once and trust plotkeeper's hooks. The file holds no credentials (Codex's hook docs give it no field for a key or a token); install never opens `~/.codex/config.toml`, which can. While installed, the file's keys are written in sorted order.
- **Copilot** (its CLI, and VS Code's agent, which shares its hooks): it renders `plugin-copilot/` with that path into `plugins/copilot/plotkeeper/` in the data folder and runs `copilot plugin install` on it, which copies it into Copilot's own plugin folder. On seven events (`sessionStart`, `sessionEnd`, `userPromptSubmitted`, `preToolUse`, `postToolUse`, `postToolUseFailure`, `agentStop`) Copilot runs `plotkeeper-hook --agent copilot --event <name>`: these camelCase payloads name no event, so each entry names its own. plotkeeper opens no file under `~/.copilot`, where Copilot keeps its settings and may keep its login; `copilot` writes what it keeps of the install. The plugin shows as `plotkeeper` in `copilot plugin list`. Whether VS Code's agent loads plugins the CLI installed is not documented; wave 1's trial (#369) checks it.
- **The old setup**: so that no event is recorded twice, it removes what the archived desktop app set up, after a backup. In `~/.zshrc`, from each `export CLAUDE_CODE_PLUGIN_DIRS=` line, the entries that end in `plotkeeper/checkout/plugin` or `dev.falinks.studio/checkout/plugin` (the data folder's link into a checkout, by its name before #322); a line left with no entry goes, and so does the app's comment above it. With agy found, agy's old plugin `studio` is uninstalled.

The backups and a record of what install did are in `install/` in the data folder. A second install keeps the first backups and replaces its own plugins. Run it again after you change `plugin/`, `plugin-agy/`, `plugin-codex/` or `plugin-copilot/`: the agents run the copies install made.

To take it all back:

    plotkeeper uninstall

Uninstall puts `~/.codex/hooks.json` back as it was (or removes it, when install made it); a file you changed after install keeps your changes and loses only plotkeeper's entries. It runs `copilot plugin uninstall plotkeeper`, `agy plugin uninstall plotkeeper` and `claude plugin uninstall plotkeeper@plotkeeper` and `claude plugin marketplace remove plotkeeper` for what install added, installs the agy plugins it backed up again, puts `~/.zshrc` back as it was, and removes the data folder's `bin/`, `plugins/` and `install/`, and nothing else: the event log and the stores stay. A `~/.zshrc` you changed after install is left as it is, and uninstall names the backup to take the lines from.

## What Codex's hooks record

Codex's payloads are near Claude Code's, read by `core/src/codex.rs`: its prompts, tool calls (`Bash` as commands, `apply_patch` as edits, `mcp__…` as external tools), permission requests, turn ends and interrupts, and the model and permission mode each event names. Its transcript is not read, so a Codex session shows no context share, and no turn trees are recorded for it, so its turns list no files. Its subagents are not wired yet, and its questions and permission prompts stay in Codex's own terminal. The adapter is built from Codex's hook docs; a real run has not checked it yet.

## What Copilot's hooks record

Copilot's payloads come in two casings, by the casing of the event name an entry is wired to: camelCase (`sessionId`, `toolName`, `toolArgs` as a JSON string), which install wires, and snake_case (`hook_event_name`, `session_id`, `tool_input`, with Claude Code's tool names), for PascalCase names. `core/src/copilot.rs` reads both: prompts, tool calls (`bash` as commands, `edit` and `create` as edits, by `path`) and failed calls, and turn ends. Copilot's payloads have no call id, so a call's start and end are paired by its tool and arguments; no turn id, model, reply text or permission mode, so a Copilot session shows none of those; its permission requests are not wired, since their payload is not documented, and stay in Copilot. Its transcript is not read and no turn trees are recorded for it. The adapter is built from Copilot's hooks reference; a real run has not checked it yet.

In VS Code, the agent's hooks run under VS Code's own processes, which outlive any session. When the process the hook would record is inside VS Code's app (`/Visual Studio Code.app/` or `/Visual Studio Code - Insiders.app/` in its path, as `ps` names it), the hook records none, so the session is never ended by the process check and is marked "idle?" after the quiet period. Copilot's CLI in a terminal, VS Code's included, is its own process and is checked like any agent.

Copilot reads `.claude/settings.json` too; plotkeeper puts no hook there, so Claude Code's entry never records a Copilot event. If Copilot turns out to run Claude Code's plugin hooks, the hook drops a Copilot payload sent to Claude Code's entry, once wave 1's trial (#369) has found what marks one (`copilot_payload` in `hook/src/lib.rs`; none is documented).

## What the Claude Code plugin does

On thirteen hook events it runs `plotkeeper-hook --agent claude`, which appends the event to the event log. Answering from the page is Claude Code only: `plotkeeper-answer` holds a question, a plan's approval or a permission until you answer on the page.

Two hooks carry a blocked command (spec #170). On `PermissionDenied` for Bash, `plotkeeper-answer` leaves a request for the card and waits up to 600 s for Allow or Decline (timeout 610). On `PreToolUse` for Bash it claims the one-time pass Allow left and returns `"ask"`, or returns at once with no output when no pass matches. `PermissionDenied` is also logged by `plotkeeper-hook` like every other event.
