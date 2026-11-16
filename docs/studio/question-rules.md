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

## The retest

A week after a Quiz was offered (answered, skipped or never opened), the studio writes one fresh Quiz on the same decision (`write_retests` in `understanding/src/retest.rs`) and offers it on Home, in a "Retest" section and nowhere else. Nothing interrupts for it.

- The writer reads the turn's stored hunk (a card's diff, or the hunks kept with the turn's summary), the first Quiz's question and the reason of its key. It is told to ask from another side, not to reword the first Quiz.
- The reply passes the guards above, and is asked for once more on a rejection. A second rejection keeps no Quiz, and the first Quiz is not retested again. A writer that gives no reply is tried again at the next hourly pass.
- A Quiz gets one retest, and one offered less than seven days ago gets none. A card's Quiz on a check ("Will it pass?") gets none.
- The retest's pick, confidence tap and "why?" line are stored with their times in `retests.json`.

## The trial of confidence and "why?"

Two optional steps, to learn whether his answers mean something:

- Before the pick, one tap: "Sure" or "Not sure".
- After the pick and before the reveal, one line, "In a few words, why?", which Enter skips. The line is not graded and no model reads it: it shows beside the writer's reason for the option he picked.

Both show on every retest. On a Quiz of "now" they show only during a two-week trial, which the setting `"quiz_trial": "YYYY-MM-DD"` in `settings.json` starts on that day; after 14 days they stop showing there. Both are stored with the pick (`confidence` and `why` taps). A typed answer graded by a model stays out of scope.

## What the rules do not decide

When a turn gets a card, a lone Quiz or nothing, the cap on open Quizzes and the Quiz on a turn that asks Daniel something are other tickets of spec #102.
