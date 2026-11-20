# The Quiz

**The Quiz** is the one multiple-choice question a turn can have, on a card or alone, under one set of rules in `docs/studio/question-rules.md`, which the prompts and the code's guards follow. It asks why this design over the alternative, what must stay true, or what happens in a concrete case in this codebase; never what the agent did, a name, syntax or bookkeeping, and nothing the Headline or the evidence line answers.

In code, an option is at most 10 words, the right one is never the longest, every option carries its reason, and a reply whose key contradicts its marks is asked for once more, then dropped. By the size of its change a turn gets a card with its Quiz (25 lines or more, or two or more logic files), a lone Quiz on its summary (8 non-trivial lines or more, no check ended failed, and not a test run, bookkeeping, docs or merge turn), or nothing; at most 3 Quizzes, not counting a check card's, are offered in any rolling hour, a card's before a lone one. A turn that ends with an `ask` line, or in Needs you, has no open Quiz. One function decides what a change is offered, for the card and for the summary alike, and a worker's card must pass the first row or is not written (`docs/studio/question-rules.md`); each decision is on record in `offers.jsonl`.

The newest turn's Quiz is open in the rail: one click on an option reveals the reasons of every option and the Body lines held back, in place, and "Show the rest anyway" reveals the same with no pick. Under the revealed answer, "I get why", "Not yet — explain more" (the writer's longer explanation, once) and "Key wrong" are optional. Every pick, skip, reveal and chip is stored with its time, with the turn's summary in `summaries.json` or, for a card's Quiz, with the card, and never shown as a raw score (see "Numbers that inform").

## A card's Quiz

A card opens with its Quiz (see "The Quiz"). Tap an option: on any card but a check's, the card says at once whether you were right, shows under each option why it is right or wrong, then the explanation. If you think the writer's key is wrong, tap "the key is wrong"; the flag is kept on the card. Your guesses are recorded, and shown only as "Numbers that inform" allows. When the writer's Quiz breaks a guard or does not parse, it is asked for once more; when the second fails too, the card shows its explanation with no Quiz, and the app logs one line in the Studio log.

## The trials

**The trials** of spec #102 test whether the Quiz helps. Retest: a week after a Quiz was offered, the studio writes one fresh Quiz on the same decision, and Home's learning slot serves it, one at a time, and nowhere else: a pick reveals the answer, "Not now" sets it aside until tomorrow, a quiet "Next" follows an answer when another is due within the day's cap of 3, and "Done for today" shows when the cap is reached. The slot also serves a fresh Quiz, one of a turn or of a worker's card that ended at most 30 minutes ago and that Daniel has not picked, skipped or set aside today, newest first: the first Retest of the day comes before it, then a fresh Quiz, then the other Retests up to the cap, and a fresh Quiz is served even after the cap. A pick on it is the same pick as in the rail, which shows it answered, and its reveal is the encoding of a pretest. The slot adds no offers: fresh Quizzes stay under the 3-an-hour limit. Home shows no backlog count. Confidence and "why?" in the rail: before the pick, "Sure" or "Not sure"; after it, a one-line "In a few words, why?" that Enter skips, shown beside the writer's reason, never graded and never read by a model. Both show on the Quiz of "now" only in a two-week trial that `"quiz_trial": "YYYY-MM-DD"` in `settings.json` starts on that day. Home has no "Sure" row and no "why?" line: one optional "not sure" toggle sits in the card beside "Not now", off by default, so a confident answer is one click, and every Home pick writes the confidence tap before the pick (`sure` false when the toggle was on) with its surface set to Home. A confidence tap stored without a surface is the rail's. Home's reveal shows the date of the turn the Quiz is of, since a Retest may describe code that has changed since, and "Key wrong", which writes the key-wrong tap.

From the reveal on Home, "See the code" leads from the explanation to the code it was about: it selects the Quiz's session (an ended one too, as a row of History does), opens the Quiz's turn in the rail, and opens that turn's first file's hunk when the turn changed any file, else the turn alone. A Retest links to the turn of its first Quiz. The Home key (⌘⇧H) brings you back, since it remembers the session you left Home for. Under the reveal the same hunk shows in place, opened once, which you can fold.

Self-test: `"quiz_self_test": "YYYY-MM-DD"` starts three weeks in which each turn that has a Quiz is assigned at random to quiz-first (as above) or body-only (its Body open and no Quiz, though the Quiz is kept for its retest). Read the result with

    cargo run -p studio-understanding --bin self_test

It prints, per arm, the turns, the retests answered and how many were right, the turns whose hunk was opened, and the median time to pick.

## Numbers that inform

The app may show a number that informs: measured against Daniel's own past and shown beside the explanation. Accuracy shows as calibration, the unsure picks against the rest, never as raw percent-correct, and only once each of the two groups holds about 20 of Home's answers, with its uncertainty shown. Streaks and a live percent-correct are allowed only as reversible trials with a stop rule set beforehand. No number is shown yet.

## The weekly rates

The weekly command prints the rates below; it is unchanged. Once a week, print the rates of the cards that arrived in the last 7 days:

    cargo run --bin weekly

- thinking rate: of the quizzes you answered, the share you answered 3 s or more after they showed;
- surprise rate: of your graded guesses, the share that were wrong by the writer's key or, on a check's card, by the check's result;
- ratings: how many of the week's cards you rated, by level;
- reasons: how many ratings gave each reason, and how many cards you flagged "the key is wrong", from the quiz or the rating.

Then the drift in Claude Code over the same 7 days, from the event log and `claude-versions.txt`:

- unrecognised wake-ups: the prompts that named a known task id in markup the studio does not know (see [Wake-ups](cards.md#wake-ups));
- one line per version change, with its date.

A rate with nothing to count prints `n/a`. To record your one-line verdict on the questions, add it after `--`:

    cargo run --bin weekly -- the questions made me stop twice, the rest I clicked through

This appends the rates and your verdict to `weekly.txt` beside the event log. If you mostly click through the questions, or call them a chore, they go and plain cards stay. The thresholds are starting guesses.
