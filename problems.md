# Agent Control Room: Problems and Research Directions

## Goal

This document records the problems I hit when working with coding agents, before deciding on any solutions. The general idea: a desktop app on top of Falinks, a serious and professional control room where you understand what the agents are doing. Every solution will be chosen only after research into what actually works for humans.

## Problems

The problems fall into three themes. The sharpest pain is in the second one: understanding changes.

### Blind during a run

- I don't know what the agents are doing right now, or which phase each one is in.
- I'm not sure whether a run is progressing or stuck.
- I miss the moment an agent is waiting for my approval or decision.
- Waiting is boring, and there is nothing to do while I wait.

### Comprehension debt

- After a run, I don't understand what changed.
- Reviewing the change is hard, and I don't know whether to trust it.
- I forget what happened in previous runs.
- I feel the project is moving faster than my understanding of it.
- **The sharpest pain:** existing tools for understanding changes produce long HTML or MD pages. They arrive at the end, when my brain is already tired from the work. They feel like an exam, and they block the work, because I don't want to continue until I've understood.

### Working well

- It's hard to write a good task for an agent.
- I don't know whether I'm working well, or whether I'm improving.
- Too many windows: terminals, editor, GitHub and chats.
- For every specific problem there is already a solution: a skill, a plugin or an explanation. My problem is knowing what to use when, how to combine it all correctly, and whether I'm doing it well.

## Hypotheses to test

These directions came up in conversation. None of them is decided, and each needs research to confirm or refute it.

- **Move learning into waiting time:** a small explanation while the change happens, instead of a long page at the end. This could solve the boredom and the comprehension debt together.
- **Small and visual before text:** a card of a few lines with a visual, not a page.
- **Non-blocking:** what I haven't understood yet piles up on the side like an inbox, and I can keep working.
- **Guessing, not testing:** light questions like "what do you think will happen?", with no score to fail.
- **Evidence, not promises:** seeing what was checked, which checks passed and what was refused, so I know whether to trust the result.
- **A game that rewards understanding, not volume:** maybe a calibration rating or a chess-style rating, so it doesn't encourage producing more code.
- **Serious control-room style:** mission, briefing, comms, telemetry, alerts and debrief, with a subtle layer of personal progression.
- **Learning content while waiting:** suggest YouTube videos or other learning material related to the current change or to my interests.

## What to research

Before building, learn what is already known about people who supervise automated systems, learn while tired, and receive alerts. Most of these fields have been studied for years in aviation, medicine and education. Some of the research can be handed to an agent, but the key sources are worth reading or watching myself.

| Field | My question | Where to start |
| --- | --- | --- |
| Situational awareness | How do you show a person what is happening in an automated system without overwhelming them? | Mica Endsley's work on aviation and air traffic control |
| Supervising automation | Why is it hard to supervise a system that does the work for you? | Lisanne Bainbridge, "Ironies of Automation" (1983) |
| Alarm fatigue | How do you alert people without them learning to ignore alerts? | Research on alarm fatigue in medicine and aviation |
| Interruptions and attention | When does a notification help, and when does it break focus? | Gloria Mark, *Attention Span*; Mark Weiser's idea of calm technology |
| Cognitive load | How do you explain a change in pieces a tired brain can take in? | John Sweller's cognitive load theory; worked examples |
| Learning that sticks | How do you learn without it feeling like an exam? | Research on retrieval practice and spaced repetition; *Make It Stick* |
| Program comprehension and code review | How do developers understand code they didn't write, and when does review stop being effective? | Research on program comprehension and on code review |
| Comprehension debt and AI | What is known about how AI harms understanding, and how to prevent it? | [Addy Osmani on comprehension debt](https://addyosmani.com/blog/comprehension-debt/); Anthropic's research on AI and skill formation; research on the illusion of understanding from AI summaries |
| Gamification and motivation | When does a game encourage learning, and when does it ruin it? | Self-Determination Theory; the overjustification effect; Goodhart's law |
| Forecast calibration | How do I measure whether my guesses are improving? | Philip Tetlock, *Superforecasting*; the Brier score |
| Existing tools | What does each tool do well, and what does it do badly? | Notes from an hour of play with Pixel Agents, agents-observe, Emdash and Antigravity |

## Next steps

- [ ] Spend an hour with the existing tools, and note what worked and what bothered me.
- [ ] Use /wayfinder to turn the research table into investigation tickets, and update the hypotheses as findings come in: what held up and what didn't.
- [ ] Write "a day with the app" based on the problems and the findings.
