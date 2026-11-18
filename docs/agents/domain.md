# Domain Docs

## Layout

The studio, a control room for understanding agent work, keeps its vocabulary in `docs/studio/CONTEXT.md`. The engine it was split from (`danielrispler/Falinks`) has its own `CONTEXT.md`; a term defined in both means what this repo's context says here.

## Before exploring

Read `docs/studio/CONTEXT.md` and any ADRs relevant to the area you are exploring.
If they do not exist, proceed silently. The domain-modeling skill creates them when terms or decisions are resolved.
To measure the real event log, use `~/Library/Application Support/dev.falinks.studio/events.jsonl`. For what the studio itself reported (errors, warnings, panics), read the Studio log, `~/Library/Logs/dev.falinks.studio/studio.log`.

## Behaviour docs

What the app does is written in `docs/studio/behaviour/`, one file per area: sessions, rail, cards, quiz, brief, checks, terminal, storage. Read the one for your area before changing its behaviour. `docs/studio/plugin.md`, `settings.md` and `development.md` hold the setup guides and the crate and test list. The README only links them.

## Vocabulary

Use the terms defined in `docs/studio/CONTEXT.md` when naming domain concepts in issues, proposals, hypotheses, and tests. If a needed term is missing, reconsider whether it fits the project or note the gap for domain-modeling.

## ADR conflicts

If a proposal contradicts an existing ADR, explicitly identify that ADR and explain why the decision should be reopened.
