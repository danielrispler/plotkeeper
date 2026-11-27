# Domain Docs

## Layout

The studio, a control room for understanding agent work, keeps its vocabulary in `docs/studio/CONTEXT.md`. The engine it was split from (`danielrispler/Falinks`) has its own `CONTEXT.md`; a term defined in both means what this repo's context says here.

## Before exploring

Read `docs/studio/CONTEXT.md` and any ADRs relevant to the area you are exploring.
If they do not exist, proceed silently. The domain-modeling skill creates them when terms or decisions are resolved.
To measure the real event log, use `~/Library/Application Support/plotkeeper/events.jsonl` on macOS, and on Linux `$XDG_DATA_HOME/plotkeeper/events.jsonl`, else `~/.local/share/plotkeeper/events.jsonl`. plotkeeper writes what it reports itself (errors, warnings, panics, and the page's log lines) to the stderr of the terminal it was started in; it keeps no log file.

## Behaviour docs

What the app does is written in `docs/studio/behaviour/`, one file per area: sessions, rail, cards, brief, checks, terminal, storage, map, pr-briefs, background-runs. Read the one for your area before changing its behaviour. `docs/studio/plugin.md`, `settings.md` and `development.md` hold the setup guides and the crate and test list. The README only links them.

## Vocabulary

Use the terms defined in `docs/studio/CONTEXT.md` when naming domain concepts in issues, proposals, hypotheses, and tests. If a needed term is missing, reconsider whether it fits the project or note the gap for domain-modeling.

## ADR conflicts

If a proposal contradicts an existing ADR, explicitly identify that ADR and explain why the decision should be reopened.
