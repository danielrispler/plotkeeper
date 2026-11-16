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

## What the rules do not decide

When a turn gets a card, a lone Quiz or nothing, the cap on open Quizzes and the Quiz on a turn that asks Daniel something are other tickets of spec #102.
