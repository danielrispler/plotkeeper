# READMEs of the benchmark tools: what a showcase README holds

Research for a showcase README (the follow-up to #137), 17 November 2026. Daniel's ask: "the README is the showcase file for the project", so read the benchmark tools' READMEs "and be inspired".

Studio terms follow `docs/studio/CONTEXT.md`. Each tool's own words appear only in quotes.

## How this was read

- **READMEs.** Each repo's default-branch `README.md`, raw, through `gh api repos/<repo>/readme`. Read in full: herdr, herdr-app, T3 Code, Zed. cmux: lines 1 to 174 and every heading. workmux: lines 1 to 135, 205 to 252 and every heading.
- **Numbers.** Stars from `gh api repos/<repo>`, words and lines from `wc`, both on 17 November 2026. "Media" counts images and videos that are not badges.
- **Copies.** The six files are in `local-evidence/2026-11-17-readme-showcase/readmes/`.
- **Limits.** AgentsRoom and Cursor are closed source and have no README; their showcase is a website, which was not read. No image or video was viewed, so nothing here judges how a screenshot looks. Stars show how many people a README reached, not that the README is why.

## 1. The six READMEs

| Tool | Repo at commit | Stars | Words | Lines | Media |
| --- | --- | --- | --- | --- | --- |
| herdr (TUI) | `herdrdev/herdr@ee5bfb5` | 51,498 | 474 | 85 | 2 (logo, demo video) |
| herdr-app | `herdrdev/herdr-app@321c9a8` | 3,071 | 1,235 | 148 | 0 |
| cmux | `manaflow-ai/cmux@6809172` | 28,809 | 3,757 | 546 | 8 (hero, one per feature) |
| workmux | `raine/workmux@d8e416a` | 3,053 | 14,535 | 2,963 | 9 |
| T3 Code | `pingdotgg/t3code@1345800` | 63,391 | 648 | 108 | 2 (logo, hero) |
| Zed | `zed-industries/zed@e5be960` | 95,198 | 298 | 48 | 0 |
| **Studio today** | `danielrispler/plotkeeper@7cafdc1` | | 9,504 | 247 | 0 |

The order of each, top to bottom:

- **herdr.** Logo; a row of links (site, install, quick start, docs); badges; a demo video; the tagline; eight benefit bullets, each ending in a link to its doc; Install (one `curl` line, then `herdr`); Docs (one row of links); Thanks; Agent instructions; Development (five lines); License.
- **herdr-app.** One sentence on what it is; Install; then sections named for what the reader does ("Find your work", "Work on another machine", "Make it yours"); Updates and privacy; For agents; Build from source.
- **cmux.** Name and a one-line description; a download button; a hero screenshot; a demo video link; Features, six rows of a table, each a heading, one sentence and a screenshot; Install; "Why cmux?", the author's story in the first person; "The Zen of cmux"; Documentation; Keyboard Shortcuts (eight tables); Session restore; FAQ (18 questions); Contributing; License.
- **workmux.** Logo; tagline; a row of links; two sentences and a "Philosophy" line; a screenshot; "Why workmux?" (five benefits); Features (13 bullets); "What people say" (11 quotes); Installation; Quick start (four steps); then the whole configuration and command reference, from line 253 to the end: 13,453 of its 14,535 words.
- **T3 Code.** Logo; name; tagline; a row of links (download, iOS, Android, web, docs, Discord); a hero image; two paragraphs on what it does and which agents it runs; "What you get" (seven bullets); Install; "Wait, what are you selling me?"; Documentation (six links and "All docs"); Contributing; License.
- **Zed.** Two badges; one sentence; Installation (a link to the site); Developing Zed (three links); Contributing; Licensing. Its showcase is zed.dev.

## 2. What they share

**1. The first screen answers "what is it" before anything else.** All six open with the name and one sentence that names the category, in 5 to 14 words:

- T3 Code: "The open-source control plane for coding agents."
- herdr: "the runtime your coding agents live on."
- cmux: "A Ghostty-based macOS terminal with vertical tabs and notifications for AI coding agents"
- workmux: "Parallel development in tmux* with git worktrees"
- herdr-app: "Herdr is a desktop app for terminals and coding agents."
- Zed: "a high-performance, multiplayer code editor"

None names an issue, a spec or a module in its opening.

**2. A picture comes before the first heading.** herdr (a video), cmux, workmux and T3 Code show the product above the first section. herdr-app and Zed show none; both have a website that does.

**3. Benefits are a short list, each in the reader's terms.** herdr has 8, T3 Code 7, workmux 5, cmux 6. Each opens with a bold phrase that states an outcome, then one or two sentences: "never hunt for the stuck one", "Your subscriptions.", "One-command cleanup." herdr links every bullet to the doc that explains it.

**4. The READMEs that link out are the short ones.** herdr (474 words), T3 Code (648) and Zed (298) keep reference in docs and give the README a list of links to it. workmux keeps its reference in the README, and 93% of the file is that reference. cmux keeps its shortcut tables and an 18-question FAQ inline.

**5. Install is one command or one download, near the top.** All but Zed give it in the README; Zed links to it. Building from source is last (herdr, herdr-app) or a link (T3 Code, Zed).

**6. Four say why it exists, in the author's voice.** cmux: "I run a lot of Claude Code and Codex sessions in parallel…", then what was wrong with the tools he had. T3 Code: "Wait, what are you selling me?" answered with "Nothing." workmux: "Why workmux?" and a Philosophy line. herdr folds the why into its bullets.

**7. Three place themselves against other tools.** T3 Code names what inspired it ("the Codex desktop app, Conductor, Claude Desktop and Cursor Glass") and says none "met our bar". workmux: "instead of yet another agentic GUI that won't exist next year". cmux: "most of them were Electron/Tauri apps and the performance bugged me".

**8. They speak to "you".** No README names its author as the user.

**9. herdr and herdr-app have a section for agents.** herdr: "if you are an ai agent helping with this repository, read `AGENTS.md`". herdr-app: `herdr-app --skill`.

**10. Proof from others is for tools with users.** workmux quotes 11 people; cmux shows its star history. T3 Code, herdr and Zed show badges only.

## 3. Where the studio's README differs today

Read at `7cafdc1`:

- It opens with "The control room's app (map danielrispler/Falinks#266, specs danielrispler/Falinks#328 and danielrispler/Falinks#369), for macOS." The category is named, but for a reader who knows the map.
- Its first heading, Build, is at line 34, after 33 lines of behaviour and a list of crates.
- It has no picture. The repo tracks no screenshot; the only images are app icons.
- It speaks of Daniel ("ranked by what Daniel needs") and its settings example holds paths under his home folder.
- Its title is "Studio"; the repo is `plotkeeper`; the app's identifier is `dev.falinks.studio`.
- Install is a build from source (`npm ci`, `npm run tauri build`). The repo has no release and no licence file.
- It says the writer "writes the card in Hebrew, right to left". The writer's prompt says "Write in English" (`understanding/src/lib.rs:1102`), and the glossary's **Explanation language** is English (danielrispler/Falinks#463). The README is stale here.
- It never says what the understanding layer is for. Cards, the Quiz, the evidence line and the Brief are described as mechanisms, not as what a reader gets.

## 4. What the studio could take

Proposals for the grilling, not decisions.

1. **An opening of one screen**: the name, one sentence that names the category and what is its own, a picture, then 5 to 7 benefits in the reader's terms (patterns 1 to 3).
2. **Lead with the understanding layer.** Every benchmark tool sells status, worktrees and terminals that survive. Only the studio explains the change and asks a question about it. The benchmark READMEs lead with what is theirs alone (T3 Code: phone and subscriptions; cmux: notification rings; workmux: worktree lifecycle).
3. **Each benefit links to its behaviour doc**, as herdr's bullets do. #137 creates those docs under `docs/studio/behaviour/`, so the showcase comes after it.
4. **A short "Why" in Daniel's voice** (pattern 6): the project advancing faster than his understanding of it, which `docs/agents/learning.md` already names.
5. **Pictures from the fixtures.** `npm run snap` and `npm run snap:rail` render the rail from fixtures with a stubbed Tauri API, so a README picture need not show a real session.
6. **Say plainly what it is not yet**: macOS only, built from source, needs Claude Code, one user so far. T3 Code and herdr-app state their limits in the install section.
7. **Leave out** quotes, star history, translations and a FAQ (pattern 10): they need users first.
