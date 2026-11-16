# The Quiz's rules

A turn has at most one **Quiz** (`CONTEXT.md`): the card's when the turn has a card, otherwise the one that comes with the turn summary. Both writers' prompts carry these rules (`quiz_rules!` in `understanding/src/lib.rs`), and code enforces the guards (`Quiz::from_value`). When a rule changes here, change the prompts and the guards with it.

## What a Quiz asks

- Why this design over the alternative, what must stay true, or what happens in a concrete case in this codebase.
- Never what the agent did, a name or syntax, or bookkeeping.
- Its stem cannot be answered from the Headline or the evidence line (the turn's files with their line counts, and its checks). The writer is given both; a stem they answer is asked about something else, or left out.
- Wrong options come from the alternative that was considered, the previous behaviour, or a common wrong model of the code.
- Never mark the right option in its text, with a sign, an emoji or a word. The options are shown in another order; the key alone says which is right.

## Guards, in code

A reply that breaks one is asked for once more. A second rejection leaves the turn with no Quiz (a card keeps its explanation).

- An option is at most 10 words.
- The right option is never the longest: it is not longer, in characters, than every wrong option. Equal lengths pass.
- Every option carries its reason (`why`) and whether it is right (`right`). Exactly one option is marked right, and the reply's key is that option. A reply whose key is not the option marked right, or that marks none or two, is rejected.

## One per turn

A turn with a card has the card's Quiz; the summary's writer is told so and returns none. A turn without one gets its Quiz with the summary. A learning question stored before the Quiz reads as a Quiz and keeps its question; its idea is dropped.

## What a turn gets

By the size of its change, which `Shape::of` reads from the turn's facts (`understanding/src/offer.rs`), or from the card's diff for an agent that reports no trees.

| The turn | Gets |
| --- | --- |
| Changed 25 lines or more, or two or more logic files | A card, with its Quiz |
| Changed 8 non-trivial lines or more, no check ended failed, and is not a test run, bookkeeping, docs or merge turn | A lone Quiz |
| Anything else | Nothing |

- A **logic file** is a changed file that is not documentation (`.md`, `.mdx`, `.txt`, `.rst`, a `docs/` folder, `README*`, `LICENSE*`), a lockfile (`*.lock`, `package-lock.json`, `pnpm-lock.yaml`, `go.sum`), generated output (`dist/`, `target/`, `node_modules/`, `generated/`, snapshots, minified files) or a fixture (`fixtures/`, `fixture/`, `testdata/`). Test files are logic files. Only logic files count towards the table's lines and files.
- A **non-trivial line** is an added or removed line of a logic file that is not blank, a comment (`//`, `#`, `/*`, `*`, `--`, `<!--`), an import (`use`, `import`, `from`, `#include`), only brackets or punctuation, or formatting only (the same text, whitespace aside, is also on the other side of the diff).
- A **test run** turn changed only test files (a `tests/` folder, `*.test.*`, `*.spec.*`, `test_*`, `*_test.*`). A **bookkeeping** or **docs** turn changed no logic file, so it counts no line and gets nothing. A **merge** turn ran `git merge` or `gh pr merge`; the table's first row still reads its size.
- A turn the table gives no card is not written a card: nothing is stored for it, and its summary carries the lone Quiz if the second row holds. A worker's card and a check's card are no turn's and keep being written.

## The cap

At most 3 Quizzes are offered in any rolling hour, across the studio, answered or not. A card's Quiz comes before a lone Quiz: a lone Quiz is offered only while fewer than 3 were offered in the last hour. A card's Quiz past the cap takes the place of the oldest unanswered lone Quiz of that hour, which loses it; when there is none, the card shows without its Quiz. The summary stays in every case. A lone Quiz's answer is recorded (`pick_quiz`) so that the cap can tell an answered Quiz from an open one.

## Never beside a decision

A turn that ends with an `ask` line, or in Needs you, has no open Quiz: its summary's Quiz and its card's Quiz are dropped.

## What the rules do not decide

How a Quiz is shown, answered and stored in the rail is other tickets' of spec #102.
