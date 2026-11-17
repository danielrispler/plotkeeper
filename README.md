# plotkeeper

A control room for coding agents that explains what your prompts changed.

![Studio: the story rail on the left, a Claude Code terminal in the middle, the session list on the right](docs/studio/assets/readme/hero.png)

plotkeeper is a desktop app for macOS that sits beside your agent sessions and keeps your understanding of the code in step with what the agents change. The app it builds is called Studio. It reads Claude Code sessions in full and agy sessions with fewer details, and it shows the status of other agents' sessions, such as Codex.

## What you get

- **Know what your prompt changed.** Each turn ends with a one-sentence Headline, and a change big enough gets a card that explains it. [The story rail](docs/studio/behaviour/rail.md) and [cards](docs/studio/behaviour/cards.md) hold both.

  ![A finished turn: its Headline and its evidence line](docs/studio/assets/readme/turn.png)

- **Facts kept apart from the model's words.** Files, line counts and check results come from hooks and git. Code on a card is read from git, never retyped by a model. A claimed fix with no passed check is marked "Agent says:". [The rail](docs/studio/behaviour/rail.md) shows the evidence line.
- **One question that makes you think.** A [Quiz](docs/studio/behaviour/quiz.md) asks about the change before it tells you the answer. It is never scored, and you get at most three an hour.

  ![A Quiz after a pick, with the reasons for each option](docs/studio/assets/readme/quiz.png)

- **Pick up where you left off.** "Where we were" and [the Brief](docs/studio/behaviour/brief.md) sum up an idle session: the last Headline, what changed since you looked, and what is still open.

  ![Where we were](docs/studio/assets/readme/where-we-were.png)

- **See who needs you.** [Every session is in one list](docs/studio/behaviour/sessions.md), only "needs you" is in colour, and you can answer an agent's question from its card.
- **Terminals that outlive the app.** A [restart, rebuild or crash](docs/studio/behaviour/terminal.md) loses none of them.
- **Your subscription, your machine.** The [card writer](docs/studio/behaviour/cards.md) runs `claude -p` on your own subscription, with no tools and no MCP servers.

## Quick start

You need macOS, Node, the Rust toolchain and Claude Code. `claude` must be on the `PATH` of your login shell: a login shell reads `~/.zprofile`, not `~/.zshrc`.

    git clone https://github.com/danielrispler/plotkeeper
    cd plotkeeper
    npm ci
    npm start

`npm start` builds the app, which takes a while the first time, and opens it. Click "New session".

Sessions Studio starts load its plugin on their own. For a `claude` you start elsewhere, see [Load the plugin](docs/studio/plugin.md).

## Status

Early: one user so far. macOS only, and no download yet, so you build from source. Issues are welcome; pull requests are not reviewed yet.

## Why

I build with coding agents, and they move faster than I understand what they build. I made plotkeeper to close that gap while the work happens, not in a long review after it. If you want a multiplexer for many agents, herdr and T3 Code do that well; plotkeeper is for understanding what the agents changed.

## Documentation

- [Sessions](docs/studio/behaviour/sessions.md), [the story rail](docs/studio/behaviour/rail.md), [cards](docs/studio/behaviour/cards.md), [the Quiz](docs/studio/behaviour/quiz.md), [the Brief](docs/studio/behaviour/brief.md), [checks](docs/studio/behaviour/checks.md), [the terminal pane](docs/studio/behaviour/terminal.md), [storage](docs/studio/behaviour/storage.md)
- [Load the plugin](docs/studio/plugin.md), [settings files](docs/studio/settings.md), [development](docs/studio/development.md)

## Licence

[MIT](LICENSE).
