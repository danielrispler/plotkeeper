# plotkeeper

A control room for coding agents that explains what your prompts changed.

![plotkeeper: the system map of a project, with the session list on the right](docs/studio/assets/readme/hero.png)

plotkeeper is a local page for macOS that sits beside your agent sessions and keeps your understanding of the code in step with what the agents change. It records the sessions you start in your own terminals and editors through their hooks: Claude Code in full, Codex, agy and Cursor (its IDE and its CLI) with fewer details. Copilot, Qwen Code, OpenCode and Cline are coming in v2.

## What you get

- **Know what your prompt changed.** Each turn ends with a one-sentence Headline, and a ready PR gets a brief that tells you what it changes. [The story rail](docs/studio/behaviour/rail.md) and [PR briefs](docs/studio/behaviour/pr-briefs.md) hold both.

  ![A finished turn: its Headline and its evidence line](docs/studio/assets/readme/turn.png)

- **Facts kept apart from the model's words.** Files, line counts and check results come from hooks and git, and code in a brief is quoted from the diff, never retyped by a model. On [the rail](docs/studio/behaviour/rail.md), a claimed fix with no passed check is marked "Agent says:".
- **Pick up where you left off.** "Where we were" and [the Brief](docs/studio/behaviour/brief.md) sum up an idle session: the last Headline, what changed since you looked, and what is still open.

  ![Where we were](docs/studio/assets/readme/where-we-were.png)

- **See who needs you.** [Every session is in one list](docs/studio/behaviour/sessions.md), only "needs you" is in colour, and you can answer an agent's question from the rail.
- **Your subscription, your machine.** The [writer](docs/studio/behaviour/pr-briefs.md#the-writers-limits) runs `claude -p` on your own subscription, with no tools and no MCP servers.

## Quick start

You need macOS and Node. In any terminal, run:

    npx plotkeeper

The first time, it lists the agents it found and what wiring each would change, then asks once: `Wire these? [Y/n]`. Yes wires them and opens the page; for Codex, run `/hooks` in Codex once to trust plotkeeper's hooks.

No starts it anyway, and the page says it is not recording. Your answer is kept: later starts ask only about an agent you installed since. Ctrl+C stops plotkeeper.

Summaries are written by `claude -p` by default, so `claude` must be on your login shell's `PATH` (`~/.zprofile`); the page's settings pick another writer.

- **Daily use:** `npm i -g plotkeeper`, then `plotkeeper`. Offline, `npx plotkeeper` waits about 70 seconds for the registry, so use the global install or `npx --prefer-offline plotkeeper`.
- **By hand:** `plotkeeper install` wires every agent found, declined ones included; `plotkeeper install --yes` is the same for scripts. `plotkeeper uninstall` undoes it and forgets your answers ([install and uninstall](docs/studio/plugin.md)).
- **From source**, with the Rust toolchain: clone the repository, then `npm ci` and `npm start`.

plotkeeper starts no sessions; it records yours. On your click only, it starts a [background run](docs/studio/behaviour/background-runs.md): one `claude -p` on a copy of your project, to back a law of your system map with a test or explain a mechanism.

Claude Code's sandbox keeps it to writing in that copy and a temp folder, with no network beyond loopback, no commit, push or `gh`, and no reading of plotkeeper's data, Claude Code's login or the usual credential folders. It can still read other files in your home folder.

Its prompt holds only text plotkeeper wrote or you confirmed, never an issue's or a PR's. What it makes becomes a draft PR only when you press Open draft PR.

## Status

Early: one user so far. macOS only, on Apple silicon and Intel. Issues are welcome; pull requests are not reviewed yet.

## Why

Coding agents write code faster than I can read their diffs. I was losing track of how my own codebase worked, and every session ended in a long, painful review. I built plotkeeper so I could follow along turn by turn, while the code is being written. If you want to run many agents side by side, herdr and T3 Code are good at that. plotkeeper is for understanding what they changed.

## Documentation

- [Sessions](docs/studio/behaviour/sessions.md), [the story rail](docs/studio/behaviour/rail.md), [blocked commands and wake-ups](docs/studio/behaviour/cards.md), [the Brief](docs/studio/behaviour/brief.md), [checks](docs/studio/behaviour/checks.md), [storage](docs/studio/behaviour/storage.md)
- [The system map](docs/studio/behaviour/map.md), [background runs](docs/studio/behaviour/background-runs.md)
- [Install and uninstall](docs/studio/plugin.md), [settings files](docs/studio/settings.md), [development](docs/studio/development.md)

## Licence

[MIT](LICENSE).
