// What the server sends the page: sessions, the selection and a session's cards, and the other wire shapes, so this file imports
// nothing (spec #548, Group 4). A field the server sends that the page never reads is optional here, so the snap fixtures may leave it
// out. The wire check (`scripts/wire/`, #246) types real values of the server's against these, so a field one lacks fails `npm run check`.

/**
 * The answer hook's request (`hook/src/answer.rs`): a question or a plan at its `PreToolUse`, or a
 * permission request, which names no call (danielrispler/Falinks#478). `agent_id` is the asking subagent's, null for the
 * session's own agent.
 */
export type Ask = {
  nonce: string;
  hook_event_name: string;
  tool_name: string;
  tool_use_id: string | null;
  tool_input: Record<string, unknown>;
  agent_id: string | null;
};

/** What Ask is opened on: the core's `Anchor`. */
export type Anchor =
  | { kind: "file"; turn: number; root: string; path: string; from?: string | null }
  | { kind: "check"; agent: string | null; call: string }
  | { kind: "subagent"; agent: string }
  | { kind: "headline"; turn: number };

/** The recorded facts, in sections: `at` is Unix milliseconds when known. */
export type Facts = { sections: { label: string; facts: { at: number | null; text: string }[] }[] };

export type Hunk = { text: string; left_out: number; binary: boolean; first_line: number };

/** A check's output, as the server reads it: its exit code and the tail of what it printed. */
export type CheckOutput = { exit_code: number | null; tail: string };

/** The two files' text, as the server sends it; null: no file. */
export type SettingsFiles = { settings: string | null; keymap: string | null };

/** `detail` is a command's description, else what the step works on; `command` a command's full text. */
export type Step = { tool: string; detail: string | null; command?: string | null };
/**
 * What the transcript says of the latest turn (danielrispler/Falinks#472): the agent's latest line, the steps since it,
 * its full texts, and why the transcript cannot be read, when it cannot.
 * `context`: the input tokens of the agent's latest message; its window and share went with the status line
 * snapshots (#416).
 * `model` and `effort`: what the session runs at, null when the transcript names none (danielrispler/Falinks#517, decision 8).
 */
export type Context = { tokens: number };
type Transcript = {
  intention: string | null;
  steps_ago: number;
  responses: string[];
  unavailable: string | null;
  context: Context | null;
  branch: string | null;
  model: string | null;
  effort: string | null;
};
// What names a session (danielrispler/Falinks#480): its issue, branch, PR and worktree, and the name derived from them.
type Identity = { issue: number | null; branch: string | null; pr: number | null; worktree: string; name: string };
export type SessionState = "needs_you" | "running" | "your_turn" | "idle" | "ended";
/** When something started and ended, in Unix milliseconds; `end` is null while it runs. */
export type Span = { start: number; end: number | null };
/**
 * A check the feed lists under its turn (#268): the latest run of a check of the session's own agent, while it runs or
 * after it failed. `ended_at` is null while it runs; `typical_ms` is its command's typical duration.
 */
export type ListedCheck = {
  statement: string;
  status: CheckStatus;
  started_at: number;
  ended_at: number | null;
  typical_ms: number | null;
};
// Who spawned a subagent: the session's own agent, another subagent, or no known spawn.
type Parent = "session" | "unattached" | { agent: string };
export type Subagent = {
  id: string;
  parent: Parent;
  kind: "worker" | "helper";
  // Its spawn's description, else its short id: the core's one name for it.
  name: string;
  // The first line of its spawn's prompt, which the rail's history shows under its name (danielrispler/Falinks#535).
  asked: string | null;
  agent_type: string | null;
  // "running", then its wake-up's status, such as "completed" or "killed".
  status: string;
  started_at: number;
  last_event_at?: number;
  ended_at: number | null;
  // Each time its end was received, and each time a step of its own after an end ran it again, with the turn it ran in.
  ends?: number[];
  resumes?: [number, number | null][];
  last_step: Step | null;
  step_span: Span | null;
  result: string | null;
  // It ended with a status other than "completed", such as "killed": the core's one rule for failed.
  failed: boolean;
  opened: boolean;
  faded: boolean;
  turn: number | null;
};
// Who a session that needs Daniel waits on, and since when: the subagent who asks, or null for its
// own agent.
type Asker = { agent: string | null; since: number };
// Who sent a prompt: Daniel, a wake-up naming the task that returned, or a known task id in markup
// the studio does not know, which counts as Daniel's.
type Sender = "daniel" | { wake_up: string } | { unrecognised: string };
// `turn` is the turn it is in, null in a log read before the core kept it.
type Prompt = { at: number; sender: Sender; turn?: number | null };
// An AskUserQuestion call, as the tool input gives it, with Daniel's choice for each question by its text
// once he answers in the terminal.
type Choice = { label: string; description?: string };
// A multi-select answer joins its labels with ", "; a single-choice answer is one whole label.
type Asked = { question: string; header?: string; multiSelect?: boolean; options: Choice[] };
type Answer = "open" | "not_answered" | { answered: Record<string, string> };
export type Question = { tool_use_id: string; agent: string | null; questions: Asked[]; answer: Answer; turn: number | null };
/**
 * A turn, a block in the rail's feed (danielrispler/Falinks#574): its prompt's first line, or "worker <name> returned";
 * `id` names its card and summary once its `Stop` came; `message` is its final reply's first line.
 * How a check, or one run of it, ended (danielrispler/plotkeeper#112): "ran" is a check whose exit code does not say, and "running" one that has not ended.
 */
export type CheckStatus = "passed" | "failed" | "ran" | "running";
/**
 * A check of a turn: a suite, the list entry its commands matched, in a worktree; its last run's status, how many earlier runs
 * failed, and each run with its own command. `call` and `agent` find a run's output (`check_output`).
 */
export type Check = {
  suite: string;
  // The worktree it ran in, as its root, and whether that is not the session's own, so a row names it.
  worktree: string;
  foreign: boolean;
  status: CheckStatus;
  retries: number;
  runs: { command: string; statement: string; status: CheckStatus; exit_code: number | null; call: string; agent: string | null; started_at: number | null; ended_at?: number | null }[];
};
export type Turn = {
  started_at: number;
  ended_at: number | null;
  prompt: string;
  // Whether a wake-up opened it, so `prompt` is "worker <name> returned", never Daniel's (#675); absent reads as his.
  wake_up?: boolean;
  id: string | null;
  message: string | null;
  checks: Check[];
  // The checks its feed block lists, newest first (the core's rule, not the window's).
  listed: ListedCheck[];
  files: TurnFiles | null;
};
/**
 * A file that differs between a turn's trees: its worktree's root, its path from there (a rename's new one),
 * a rename's old path, its lines added and removed (null for a binary file), and the name of the agent whose call
 * names it or whose worktree holds it (null when none does, as for Daniel's own edit).
 */
export type TurnFile = { root: string; path: string; from: string | null; added: number | null; removed: number | null; agent: string | null };
// What a turn changed: every file that differs between its trees, but those another call names, and the files
// a move of `HEAD` (a merge, a pull, a rebase, a checkout) brought in, which are only counted in `merged`.
// `null` while the trees are not yet diffed, and for an agent that reports no trees.
type TurnFiles = { files: TurnFile[]; merged: number };
/**
 * What the session's agent adapter reports, which decides what its view shows (danielrispler/plotkeeper#43, decision 4);
 * `agent` is its mark's name.
 */
export type Capabilities = {
  agent: string;
  asks: boolean;
  background_tasks: boolean;
  replies: boolean;
  permission_mode: boolean;
  durations: boolean;
  transcript_version: boolean;
  // Its transcript names the model, which the header's model line shows; else the model is `Session.model`.
  transcript_model: boolean;
  // It reports how full its context is.
  context: boolean;
  // It records a git tree of each worktree a turn touches, so a turn has files (danielrispler/plotkeeper#111).
  turn_trees: boolean;
  // How its transcript's lines read.
  transcript_format?: "claude" | "agy" | "unread";
};
export type Session = {
  id: string;
  capabilities: Capabilities;
  // The model its agent's hooks name (agy's), null otherwise; with `capabilities.transcript_model` it is `transcript.model`.
  model: string | null;
  // The folder it works in, from its latest event; `folder_name` is its last segment.
  cwd: string;
  folder_name: string;
  // Its project folder (spec #659): the root of the first folder it got (git's top level, else that folder), a task's worktree's
  // main checkout, set once. The list groups by it, and the map, History and the selection read it.
  project: string;
  // Daniel's name for it from the studio (danielrispler/Falinks#469); null for its folder's.
  name: string | null;
  // A few words on what it is about, written from Daniel's first prompt; null before it arrives.
  prompt_name: string | null;
  // The id `claude --resume` takes: its latest conversation's; a `/clear` changes it, not `id` (danielrispler/Falinks#474).
  resume_id: string;
  // Claude Code's permission mode from its latest event that names one, null before (danielrispler/Falinks#541).
  permission_mode: string | null;
  // When each `/clear` continued the row, in Unix ms: its "cleared" markers.
  cleared: number[];
  // Its task has landed (the core's rule): shown as a check mark once the session is at your turn, idle or ended.
  landed: boolean;
  last_step: Step | null;
  // When its first and latest events came, its latest turn's and its last step's times (danielrispler/Falinks#473).
  started_at: number;
  last_event_at: number;
  turn_span: Span | null;
  step_span: Span | null;
  // Its latest `Stop`, until its next prompt: the id a card on it has, whether it requested one, its last message's first line.
  stopped?: { card: string; requested: boolean; message: string | null } | null;
  state: SessionState;
  agents: Subagent[];
  asker: Asker | null;
  prompts: Prompt[];
  turns: Turn[];
  questions: Question[];
  // Its blocked commands, in block order.
  blocked_commands: BlockedCommand[];
  // "worker <name> returned", after a wake-up until its next step or Daniel's next prompt.
  returned: string | null;
  transcript: Transcript;
  identity: Identity;
  // Its phases and the estimate (danielrispler/Falinks#475).
  work: Work;
  // In the session list only: what its row says, built by the card store.
  line?: string;
  // Its turn finished and Daniel has not looked at it since; danielrispler/Falinks#556 sets it (danielrispler/Falinks#572 draws it).
  unseen?: boolean;
  // Its agent's process, when its hook could tell; without one an idle session shows "Idle?" (#330).
  pid?: number | null;
  // How many of its turns Daniel has seen (danielrispler/plotkeeper#124).
  seen: number;
  // How many it had seen when Daniel selected it, which "Since you looked" counts from.
  seen_at_selection?: number;
  // What happened after the turns he had seen when he selected it, from the core's `Session::since`, as `since_looked` (the agent has its own `since`): the banner's count.
  since_looked: Since;
};
/** What happened in a session since Daniel looked (danielrispler/plotkeeper#124). */
export type Since = { turns: number; files: number; passed: number; failed: number; ran: number };
// A multiple-choice question as the store kept it on a summary written before #347; `key` is the right option, counting from 0.
type Quiz = { text: string; options: { text: string; why: string }[]; key: number };
// What Daniel did with a Quiz, each with its time (danielrispler/plotkeeper#120): what the retired turn Quiz, cards, Home and the
// Retests wrote, which the store still reads (#347, #348). The window sends none.
type Tap =
  | { kind: "pick"; option: number }
  | { kind: "confidence"; sure: boolean; surface?: "home" }
  | { kind: "why"; text: string }
  | { kind: "shown" | "hunk" }
  | { kind: "skip" | "understood" | "explain_more" | "key_wrong" | "not_now" };
type QuizTap = Tap & { at: number };
/**
 * An answered question, plan or permission in the session's decision history (danielrispler/Falinks#479): the call's input,
 * Daniel's answers by question text or "allow"/"deny", when, and where he answered.
 */
export type Decision = {
  call: string;
  agent: string | null;
  tool: string;
  input: { questions?: Asked[]; plan?: string; command?: string; file_path?: string; url?: string };
  answer: Record<string, string> | "allow" | "deny";
  at: number;
  source: "card" | "terminal";
};
/**
 * A Bash command auto mode blocked (danielrispler/plotkeeper#170): the command in full, its folder, the rule that blocked it, the
 * subagent that asked (null for the session's own agent) and where it stands. Since its card went (#572) it only waits on Daniel
 * until the session moves on (`moved_on`); Allow, Decline and the retry come only from older logs.
 */
export type BlockedStatus =
  | "blocked"
  | "allowed"
  | "allowed_not_run"
  | "running"
  | "finished"
  | "running_in_background"
  | "blocked_again"
  | "declined"
  | "moved_on";
/** Whether a blocked command in `status` waits on Daniel: blocked, also again, and not moved on (the core's `BlockedStatus::waits`). */
export const blockedWaits = (status: BlockedStatus) => ["blocked", "blocked_again"].includes(status);
export type BlockedCommand = {
  call: string;
  command: string;
  cwd: string;
  reason: string;
  agent: string | null;
  at: number;
  status: BlockedStatus;
  // The retry's call id once it runs (`check_output` reads its tail), when it started, the agent whose call it is (the
  // session's own may run a subagent's command), its exit code once finished.
  run?: string | null;
  run_at?: number | null;
  run_by?: string | null;
  exit_code?: number | null;
  task?: string | null;
};
/**
 * A turn's summary (danielrispler/Falinks#526), as the store keeps it; a Quiz stored before #347 and a learning section stored
 * before danielrispler/plotkeeper#118 read as the old Quiz.
 * `headline` and `lines` are the turn writer's Headline and typed Body lines (danielrispler/plotkeeper#115); a summary stored
 * before them has neither, and its `summary` text reads as a Headline and `reported` lines.
 */
export type SummaryLine = { kind: "why" | "reported" | "open" | "ask"; text: string };
// The server sends the summary as the store keeps it: also the request it was written from, when it arrived and the
// turn's hunks, which the window never reads.
type SummaryRequest = {
  session_id: string;
  id: string;
  prompt: string;
  responses: string[];
  intentions: string[];
  card: string | null;
  facts: { files: TurnFile[]; checks: Check[]; external: number };
  wanted: boolean;
};
export type TurnSummary = {
  request?: SummaryRequest;
  added_at?: number;
  hunk?: string;
  // The Quiz of a summary stored before #347, which the window no longer shows; a new summary has none.
  summary: { summary: string; headline?: string | null; lines?: SummaryLine[]; quiz?: Quiz | null };
  taps?: QuizTap[];
  // The self-test's arm (#130), kept in the store.
  arm?: "quiz_first" | "body_only" | null;
};
/**
 * The Brief's slots (danielrispler/plotkeeper#122): the Goal a model wrote, each answered question with Daniel's reply, and
 * the Open lines the server computed. `brief` is a brief stored as prose, which shows while there is no Goal.
 */
export type OpenItem = {
  kind: "question" | "check" | "unchecked" | "open" | "ask" | "unpushed" | "pr";
  text: string;
};
// A line of "Design so far" (danielrispler/plotkeeper#123): a decision with its reason, and the id of the turn or card it came from.
type DesignLine = { text: string; cite: string };
/** The selected session's brief, decision history and the risk notes of its blocked commands. */
export type SessionCards = {
  brief: string | null;
  goal: string | null;
  answers: { question: string; reply: string }[];
  open: OpenItem[];
  design: DesignLine[];
  decisions: Decision[];
  // What each blocked command's card shows of its risk note, by the card's call id (danielrispler/plotkeeper#298).
  risk_notes: RiskView[];
  // The note of each Bash decision in the history, by the decision's call; the window compares its `at` with the answer's (danielrispler/plotkeeper#299).
  decision_notes: RiskView[];
};
/**
 * A blocked command's risk note as the server joins it to its card: on its way, not coming (a timeout, an unreadable reply or
 * no card writer), or arrived. The server also sends when it arrived, which only the decision history reads.
 */
export type RiskLevel = "none" | "low" | "medium" | "high" | "very_high" | "not_sure";
/** The levels as the decision history says them. */
export const riskLevels: Record<RiskLevel, string> = { none: "None", low: "Low", medium: "Medium", high: "High", very_high: "Very high", not_sure: "Not sure" };
export type RiskView =
  | { call: string; state: "reading" }
  | { call: string; state: "failed" }
  | { call: string; state: "arrived"; level: RiskLevel; line: string; at?: number };
/**
 * What a call asks Daniel, by its tool, as the answer hook's `Kind`: a question, a plan, or any other
 * tool's permission.
 */
export type AskKind = "question" | "plan" | "permission";
export const askKind = (tool: string): AskKind =>
  tool === "AskUserQuestion" ? "question" : tool === "ExitPlanMode" ? "plan" : "permission";
/**
 * An entry of a subagent's log, as the core reads it from the log file, and what one read returned: the entries
 * after the offset asked for, and the offset read to.
 */
export type LogEntry =
  | { kind: "prompt"; text: string }
  | { kind: "reply"; text: string }
  | { kind: "call"; id: string; tool: string; detail: string | null }
  | { kind: "result"; id: string; text: string; failed: boolean; cut: boolean };
export type SubagentLog = { entries: LogEntry[]; offset: number };
/** A log as drawn: each call with its result, null while it runs. */
export type LogResult = { text: string; failed: boolean; cut: boolean };
export type LogRow =
  | { kind: "prompt"; text: string }
  | { kind: "reply"; text: string }
  | LogCall;
export type LogCall = { kind: "call"; id: string; tool: string; detail: string | null; result: LogResult | null };

/**
 * The selection is the core's: the session Daniel selected and the subagent of it he opened, whose
 * view then shows its own line.
 */
export type Selection = { session: Session; agent: string | null };

export const stateLabels: Record<SessionState, string> = {
  needs_you: "Needs you",
  running: "Running",
  your_turn: "Your turn",
  idle: "Idle",
  ended: "Ended",
};

/** A phase of a session's work on a ticket, in the usual order (danielrispler/Falinks#475). */
export type Phase =
  | "orienting"
  | "designing"
  | "implementing"
  | "checking"
  | "reviewing"
  | "opening_pr"
  | "waiting_on_ci"
  | "merged";

/**
 * A session's work: its kind (`implement`), its phases with when each started (Unix ms) and after how
 * much of the agent's time, and the estimate set as its latest phase started: the range of the
 * remaining agent time in ms, null for "no estimate", and whether it waits on Daniel now.
 */
export type Work = {
  kind: string | null;
  phases: { phase: Phase; at: number; agent_ms: number }[];
  agent_ms: number;
  estimate: { range: [number, number] | null; depends_on_you: boolean };
};

// The Map view's payload (`server/src/map.rs`, `MapState`): one project's map as read from `origin`'s default branch (or its checkout, or a
// pending PR, #753), and Daniel's own
// positions and tiers for it. A feature of the view adds its field here and in `MapState` there; `server/tests/wire.rs` holds
// the two to agree (#333).
// What landed since Daniel caught up and what he has read of each PR's brief (`fog.rs`); `UserMap.fog`, which the window does not read.
type FogState = {
  first: string | null;
  processed: string | null;
  parts: Record<string, FogEntry[]>;
  reads: Record<string, { lit: string[][]; chapters: Record<string, { head: string; at: number }> }>;
};
/**
 * One commit Daniel has not caught up on, on one part: the PR its subject names, and why it fogs the part (`fog::Change`: "added",
 * "paths", `law <id>`, `link <id>`, `flow <id>`, a changed word such as "caption", and `N files`); an entry filed before #495 has neither.
 */
export type FogEntry = { commit: string; strength: Fog; pr: number | null; why: string[] };
/** The map's camera (#383): a world point `p` shows at `p * scale + (x, y)` in the map area. */
export type Camera = { scale: number; x: number; y: number };
// `view`: the project's saved camera, none until it was first moved.
// When Daniel last engaged with each part, and the baseline (`staleness.rs`); the window reads the bands, not this.
type Staleness = { since: number | null; parts: Record<string, { at: number; kind: "know" | "refresher" | "catch-up" | "chapter" | "other" }> };
export type UserMap = { positions: Record<string, [number, number]>; tiers: Record<string, string>; fog?: FogState; view?: Camera | null; staleness?: Staleness };
export type MapState = {
  project: string;
  // "not-git": the project's folder is in no git repository, so there is no map to read or draft.
  status: "loaded" | "none" | "unreadable" | "not-git";
  // The text of `system-map.json` when it loaded.
  source: string | null;
  // Where the map was read from (#753): the default branch, else the main checkout's file, else the accepted map waiting on its draft PR;
  // null when no source has one.
  from: "default-branch" | "checkout" | "pending-pr" | null;
  // The draft PR a `pending-pr` map waits on, and whether it closed unmerged.
  pending: { number: number; closed: boolean } | null;
  // For a `checkout` map that reads, the problems `system-map check` finds in the working tree; empty for the others.
  problems: string[];
  reference: string | null;
  commit: string | null;
  note: string | null;
  user: UserMap;
  // How strongly each part shows fog under Daniel's effective tiers; a part not listed is clear.
  fog: Record<string, Fog>;
  // CI on the default branch as last read through `gh` (`ci_laws.rs`); null until a read has finished.
  ci: Ci | null;
  // The PRs ready for review (open, not a draft), as last asked (`prs.rs`); the Map view's tray lists them.
  ready: ReadyPr[];
  // The map items the part sheet says are not yet ratified (`ratify.rs`).
  ratify: PendingItem[];
  // The project's background runs and the pause line (`runner.rs`).
  runs: Runs;
  // What each fogged part's sheet offers (`part_catch_ups.rs`): its part catch-up, or the one PR brief chapter that lights it.
  catch_ups: Record<string, CatchUpOffer>;
  // The staleness band (1 to 3) of each Core part not engaged with for 3, 6 or 12 weeks (`staleness.rs`); a part not listed is not stale.
  stale: Record<string, number>;
  // What each stale part's sheet says (`staleness::lines`, #495): the band, and when Daniel last engaged with the part, or "not engaged
  // since tracking began on <date>".
  stale_why: Record<string, string>;
  // The project's pre-flight (`pre_flight.rs`, #394): the parts a piece of work will likely cross; null when none is run.
  pre_flight?: PreFlight | null;
  // The project's micro-worlds (`micro_worlds.rs`, #399) by subject key (`flow:<id>`, `figure:<pr>:<chapter>:<step>`), and whether a run can
  // build one (`microWorlds.dshVisual` is set); absent in a payload from before them.
  micro_worlds?: MicroWorlds;
  // The patches of the project's back a law runs (`back_a_law.rs`, #398): being proven, rejected with why, or proven with the test patch
  // and the proof; a past payload has none.
  law_patches?: LawPatch[];
};
/**
 * A micro-world as the map shows it: `building` while its run is queued, running or checked, `ready` with a checked page, `failed` with why
 * (`line`) and the prompt to copy. `page`: a checked page is stored to open; `stale`: a part on it changed since it was built.
 */
export type MicroWorldView = { phase: "building" | "ready" | "failed"; page: boolean; stale: boolean; line: string | null; prompt: string | null };
export type MicroWorlds = { on: boolean; worlds: Record<string, MicroWorldView> };
/** What "Explore this" did: started a run, or gave the prompt to copy and why. */
export type Explored = { started: boolean; line: string | null; prompt: string | null };
/**
 * A back a law run's patch: its run's id, the law, where it stands, and for a proven one the test patch and the checks' output with the
 * test alone (passing) and with the plant (failing on the new test).
 */
export type LawPatch = {
  id: string;
  project: string;
  law: string;
  state: "proving" | "rejected" | "proven" | "opened" | "out-of-date";
  line: string | null;
  at: number;
  diff: string | null;
  proof: { test: string; file: string; passed: string; failed: string } | null;
  // The draft PR Open draft PR opened (#400), and its address; a past payload has none.
  pr?: number | null;
  url?: string | null;
  // A new law's sentence as Daniel confirmed it, and the PR whose Guard with a law started the run (#401).
  text?: string | null;
  guards?: number | null;
};
/** What Open draft PR did: a draft PR opened, or the patch no longer applies to the default branch. */
export type LawPatchOpened = { kind: "pr"; number: number; url: string } | { kind: "out-of-date" };
/** What a click on Back a law did: a run started, or refused with the prompt to copy instead. */
export type BackLawStarted = { kind: "run"; id: string } | { kind: "refused"; line: string; prompt: string };
/**
 * A fogged part's offer (#391): a part catch-up, `ready` when one is stored for its current fog list, `writing` while one is written, `failed`
 * when the last write for it failed; or the chapter of the one PR's brief that lights the part. `outline`, with `ready`: what the sheet's
 * catch-up card shows of the stored one (#636), never its gist. `older`, while none is stored for the current list: the stored one written
 * for an earlier list whose every entry is still fogged (#638), its outline and the fog entries it covers (`fog`, their commits).
 */
export type CatchUpOffer =
  | { kind: "catch-up"; ready: boolean; writing: boolean; failed: boolean; stage?: CatchUpStage; outline?: CatchUpOutline; older?: { outline: CatchUpOutline; fog: string[] } }
  | { kind: "chapter"; pr: number; chapter: number };
/** A stored part catch-up's title and, per step of its one chapter, the step's kind (`k`) and headline (`h`), as `part_catch_ups::Outline`. */
export type CatchUpOutline = { title: string; steps: { k: string; h: string }[] };
/** Which call of a part catch-up's write runs now (#450): the writer, the claim check, or the retry rewriting `claims` flagged items. */
export type CatchUpStage = { kind: "writing" } | { kind: "checking" } | { kind: "fixing"; claims: number };
/**
 * A part catch-up as `part_catch_up` returns it (`understanding/src/part_catch_up.rs`): the story in the PR brief's shape and the fog list it covers.
 * Over more than a few fog entries it carries `summary`: the line saying it summarises the main shifts, and the full fog list, oldest first.
 * `body`: always empty in a summary's list; the window never reads it.
 */
export type FogListEntry = { commit: string; pr: number | null; title: string; body?: string };
export type StoredCatchUp = {
  version: number;
  part: string;
  fog: string[];
  written_at: number;
  model: string;
  partial: boolean;
  brief: PrBrief;
  summary?: { line: string; entries: FogListEntry[] };
};
/** What `part_catch_up` answers: the catch-up, or that its write has started. */
export type CatchUpOpened = StoredCatchUp | { writing: true };
/**
 * A pre-flight (#394): the issue it started from (none for free text or parts picked by hand), the issue's title or the text, the part
 * ids, and whether the writer named them or Daniel picked them.
 */
export type PreFlight = { issue: number | null; title: string; parts: string[]; predicted: boolean; at: number };
/** An open issue a pre-flight can start from. */
export type OpenIssue = { number: number; title: string };
/** A matched brief's line (#394): the parts the pre-flight expected against the parts the PR touched. */
export type Expected = { expected: string[]; touched: string[] };
// A background run (`server/src/runner.rs`): a headless agent run Daniel started with a click, queued one at a time per project. `at` is when
// it was asked for, started or ended, by its state. An ended run says how it ended in `line`, lists the tool calls it was denied, and
// carries its `prompt` to copy on every end but a clean one. `paused` is why every queue waits, null while they run.
type RunEnd = "done" | "denied" | "failed" | "timed-out" | "died" | "setup-failed" | "not-logged-in" | "rate-limited";
export type RunView = { id: string; kind: "back-law" | "micro-world" | "map-review" | "first-map"; state: "queued" | "running" | "ended"; at: number; outcome: RunEnd | null; line: string | null; denials: string[]; prompt: string | null };
export type Runs = { paused: string | null; runs: RunView[] };
/** Ratifying (`ratify.rs`): an item of a PR is a map change (kind part, actor, link, law or flow) or a decision the agent declared. */
export type PendingItem = { pr: number; kind: string; id: string };
// `evidence` (#499): on a link the PR adds, the import sites behind it, stored on its brief when it was written, read at `head`.
// `mark` (#643): the id of the mark the row lights on the Map diff (`part:<id>`, `actor:<id>`, `law:<id>`, `line:<end>|<end>`); none for a decision or a flow.
export type RatifyRow = { pr: number; kind: string; id: string; text: string; change: string; status: "ratified" | "accepted" | "overturned" | "not-yet"; detail: RatifyDetail | null; evidence?: LinkSites; mark?: string };
// The Map diff (`map_diff.rs`, #640): today's map with a PR's added and removed items in it (`source`, version 2 JSON), and each one's tag.
// A line is keyed by its two ends in sorted order. `pr_map_diff` answers null when the PR changed nothing on the map.
export type MapTag = "added" | "removed" | "edited";
export type MapDiff = { pr: number; base: string; head: string; source: string; parts: Record<string, MapTag>; actors: Record<string, MapTag>; lines: { ends: [string, string]; change: MapTag }[]; laws: Record<string, MapTag> };
// Where an import stands (`pr_brief.rs`, `ImportSite`): in the importing file, at its 1-based line when one is known.
export type ImportSite = { file: string; line?: number };
export type LinkSites = { head: string; sites: ImportSite[] };
// An import site's code (`ratify_site`), read from the commit: its lines numbered from `first`, and the site's line to mark when it has one.
export type SiteSnippet = { file: string; line?: number; first: number; lines: string[] };
// Old beside new for a part or a link: a text on each side, null on the side that has no item.
type RatifyWords = { before: string | null; after: string | null };
type RatifyHeld = { name: string; text: RatifyWords; paths_added: string[]; paths_removed: string[] };
export type RatifyDetail =
  | { of: "part"; name: RatifyWords; purpose: RatifyWords; caption: RatifyWords; holds: RatifyHeld[]; paths_added: string[]; paths_removed: string[] }
  | { of: "link"; kind: RatifyWords; reason: RatifyWords; direction?: RatifyWords };
// A PR head's Map text review (`map_review.rs`, #642), null when the head has no item to review. `idle`: not started (a PR not by Daniel's
// login waits for "Review map text", `map_review_start`); `failed` has `line` and offers "Run again", the same command. `items` holds
// the head's items that have a verdict, by id: `part:<id>/name|purpose|caption`, `held:<part>/<n>` (n from 1, its place in `holds`),
// `link:<id>/reason`, `law:<id>/text`. Any verdict but `supported` is `flagged`.
export type MapReviewItem = { id: string; verdict: "supported" | "overreach" | "contradicted" | "unreadable"; why: string; cites: string[]; flagged: boolean };
// A flagged still-true item (#687): unchanged text of a part whose files the PR changed, judged false at the head. `name` and `text` are
// the head's map's (the part's name and "purpose" or "caption", the held item's name, the link's ends, the law's id), read by the
// server. Ratify decides it as an item of kind "still-true" whose id is `ratify_id`; `decided`: Daniel accepted or overturned it.
export type MapReviewFlag = { id: string; verdict: "overreach" | "contradicted" | "unreadable"; why: string; cites: string[]; name: string; text: string; ratify_id: string; decided: boolean };
// `left_out`: how many of the head's changed map items the review left out of its prompt, for an id it does not take (over 40 characters).
// `items`, `checked` and `flagged` are of changed items only; `still_true` holds the flagged still-true items, never a supported one. A
// head whose only items are still-true ones is null but when its review failed or has a flag; its `changed` is false, and the page shows
// of it only a failure, with "Run again", and the flags not decided.
export type MapReview = { head: string; state: "idle" | "reviewing" | "done" | "failed"; items: MapReviewItem[]; checked: number; flagged: number; line: string | null; left_out: number; still_true: MapReviewFlag[]; changed: boolean };
// What a ready PR's brief is doing (`pr_briefs.rs`): a newer head was pushed (stale), being written now, and the chapters' titles, or the one story's.
// `note` (#477, part B): why the brief at this head is what it is after a push: the read carried over, or it was rewritten.
// `since` (#497): the title of the "Since you read" story stored for this head, which the tray opens first; `since_chapters`, its chapters' titles when it has more than one, one tray row each.
// `short`: the stored brief is a short one (#644), made by code from the Map diff of a PR that fogs nothing.
type BriefInfo = { stale: boolean; writing: boolean; chapters: string[]; quiet: boolean; read?: boolean; failed?: boolean; note?: string; since?: string; since_chapters?: string[]; short?: boolean };
export type ReadyPr = { number: number; title: string; head: string; branch?: string; brief: BriefInfo | null };
// A PR brief as the writer made it and `pr_brief` returns it (`understanding/src/pr_brief.rs`). A step's `flags` are what the checks found wrong with it
// that the one rewrite did not mend: it shows "not confirmed from the facts".
type BriefCite = { from: string; quote: string };
export type BriefFigure = { kind: string; frame: string | null; items: { label: string; tone: string | null; bad: boolean | null }[]; before: string[]; after: string[] };
export type BriefCheck = { kind: string; q: string; opts: string[]; key: unknown; yes: string; no: string; flow: string | null; test: string | null };
export type BriefStep = {
  k: string; h: string; b: string; on: string[]; hops: [string, boolean][]; law: string | null; covered_by: string | null; list: string[];
  risks: { part: string; trigger: string; check: string }[]; fig: BriefFigure | null; check: BriefCheck | null; cites: BriefCite[]; flags: string[];
};
/**
 * `gist` (#397): the story in one sentence, on a chapter that lights a Core part and on a part catch-up; absent or empty in older briefs.
 */
export type BriefChapter = { behaviour: string; parts: string[]; title: string; steps: BriefStep[]; flags: string[]; gist?: string; gist_flags?: string[] };
/** `also`: the writer's Also touched lines of an older brief; `also_groups` (#489): code's groups by kind, absent in an older brief. */
export type PrBrief = {
  title: string; overview: BriefStep[]; chapters: BriefChapter[]; also: { part: string; line: string; flags: string[] }[]; also_groups?: AlsoGroup[];
  unseen: string[]; decisions: { id: string | null; text: string; flags: string[] }[]; flags: string[];
};
/** One Also touched group (`pr_brief.rs`), in the order shown, its files sorted by path; `lines` added and removed. */
export type AlsoKind = "no-part" | "tests" | "config" | "docs" | "ignored" | "other";
type TouchedFile = { path: string; lines: number; part?: string; ignored?: boolean; laws?: string[] };
export type AlsoGroup = { kind: AlsoKind; files: TouchedFile[]; lines: number };
// `guard`: Guard with a law's offer (#401), the parts a bug fix touched; none when the PR offers none, or in a brief from before it.
// `shape`: what code decided the brief's shape would be; the window never reads it.
type BriefShape = { chapters: boolean; required: string[]; also: string[]; overview: boolean; flows_changed: string[] };
// `partial_facts` (#477, #491): what the brief was written without, absent when nothing was missing or in a brief from before it.
// `link_evidence` (#499): the import sites behind each link the PR adds, read from its head; Ratify reads it through `pr_ratify`.
// `map_problems` (#641): the map check's problems at its head, which Ratify's head lists; absent when the check passed.
type StoredBrief = {
  version: number; pr: number; head: string; written_at: number; stale: boolean; model: string; shape?: BriefShape; brief: PrBrief; guard?: Guard | null;
  partial_facts?: PartialFacts; link_evidence?: { link: string; sites: ImportSite[] }[]; map_problems?: string[];
  // A short brief (#644): one step made by code, "The map in this PR", then Ratify.
  short?: boolean;
};
// The cut diff's files none of whose hunks were sent and those sent in part, with its bytes sent and not sent; the claim check's error; the
// extensions of the diff's source languages no extractor of the map reads (`understanding/src/pr_brief.rs`, `PartialFacts`).
type DiffCut = { files_cut: string[]; files_partial: string[]; bytes_sent: number; bytes_cut: number };
export type PartialFacts = { cut?: DiffCut; claim_error?: string; no_extractor?: string[] };
export type Guard = { parts: string[] };
/** A new law (#401): drafted by the writer from a PR's facts, confirmed or edited by Daniel, then added with its test by a run. */
export type NewLaw = { id: string; on: string; text: string };
/**
 * `partial`: the writer did not see the whole diff.
 * `expected`: the PR matched the project's pre-flight (#394).
 * `since` (#497): this is a "Since you read" story, of the push from that head to its own.
 * What main changed since an open PR's merge base that bears on its brief (#494): laws, links and flows at a lit part, lit parts whose
 * `paths` changed, and changed files of lit parts and root build manifests.
 */
export type StaleBase = { laws: string[]; links: string[]; flows: string[]; paths: string[]; files: string[] };
export type BriefView = { brief: StoredBrief; partial: boolean; expected?: Expected; stale_base?: StaleBase; since?: string };
/** What `pr_brief` answers: the brief, or that its write has started (it comes through `map-landed`). */
export type BriefOpened = BriefView | { writing: true };
export type Fog = "thin" | "thick";
// What a finished run of the default branch's CI says of the laws: `unknown` (nothing to read), `passed`, `broken` (a test that backs a
// law failed), `not-run` (it failed, but not in a test that backs a law), `unreadable` (it failed and its log cannot be read).
type CiStatus = "unknown" | "passed" | "broken" | "not-run" | "unreadable";
export type Ci = {
  status: CiStatus;
  run_url: string | null;
  commit: string | null;
  // A newer run is still going, so this result is for an earlier commit.
  earlier: boolean;
  // The parts that own a broken law's test; only with `broken`.
  parts: string[];
  laws: string[];
  tail: string;
  note: string | null;
  // When that run finished, as GitHub gives it (ISO 8601, UTC); null in an answer kept before #495.
  at: string | null;
};
/** A tagged test shown by "See the proof": its file and tag line, and the code from the tag on, starting at line `first`. */
export type Proof = { file: string; line: number; test: string | null; first: number; snippet: string };

/**
 * One commit of a part's history strip, oldest first; `fogged` is a commit Daniel has not caught up on.
 * `brief`: the PR whose stored brief this commit offers (`pr_briefs.rs`), when it has one.
 */
export type HistoryEntry = { commit: string; short: string; label: string; fogged: boolean; brief: number | null };
/** The ready PRs of a session's project that a brief waits for (`briefs_waiting`). */
export type WaitingBriefs = { project: string; numbers: number[] };
/** The map as of a commit (a scrub), and a commit's files with their diffs (the drill-down). */
export type HistoricalMap = { commit: string; short: string; label: string; source: string };
export type CommitFiles = { commit: string; short: string; label: string; url: string | null; files: { path: string; diff: string }[] };
/**
 * What `pr_diff` is asked (`map.rs`): `parts`, only the files these parts own; `paths`, only these files; every file the PR changed when both are absent; `base`, a since story's head, which the diff runs from (#497).
 */
export type PrDiffArgs = { project: string; number: number; head: string; title: string; parts?: string[]; paths?: string[]; base?: string };

/**
 * A project of the saved list (`server/src/projects.rs`): its main checkout, whether its folder was moved or deleted, and whether it is
 * under one of the system's temporary folders (spec #659).
 */
export type SavedProject = { path: string; gone: boolean; temporary: boolean };

/**
 * The free scan of a git repository with no map (`server/src/map_scan.rs`, #753): the skeleton map with the computed edges as `import` links, as
 * text; each part's tracked file count; the languages a reader reads and those none reads yet; whether `claude` was found; the folders that
 * look like copies, each ticked while the skeleton ignores it; the code files and tests no part owns; and the subfolders each part that can
 * split would split into (#761).
 */
export type MapScan = {
  map: string;
  files: Record<string, number>;
  read_languages: string[];
  unread_languages: string[];
  claude: boolean;
  ignore_candidates: { path: string; ticked: boolean }[];
  loose_files: string[];
  loose_tests: string[];
  splits: Record<string, string[]>;
};

/** One change of the skeleton on the scan screen (`Edit` in `server/src/map_scan.rs`, #761); `part: null` gives the files a new part. */
export type ScanEdit =
  | { kind: "ignore"; folder: string; ignored: boolean }
  | { kind: "home"; files: string[]; part: string | null }
  | { kind: "merge"; part: string; into: string }
  | { kind: "split"; part: string };

/**
 * First map's Map draft (`server/src/map_draft.rs`, #762): `map`, the skeleton with each part's latest written text and the person's
 * edits, as text; each writer group (and each Redraft this part) by index, which `map_draft_retry` takes, with its state and why it
 * failed; the reviewer's verdict on each part's purpose as `map` shows it, by part id; what the writers left out; and `ready`, true once
 * every group is done, for Accept; and `path`, the `system-map.json` Accept writes.
 */
export type MapDraftState = "writing" | "checking" | "done" | "writer-failed" | "reviewer-failed";
export type MapDraft = {
  map: string;
  groups: { parts: string[]; state: MapDraftState; why: string | null; redraft: boolean }[];
  verdicts: Record<string, MapReviewItem>;
  not_written: { what: string; why: string }[];
  ready: boolean;
  path: string;
};
/**
 * What Accept's Write did (`Accepted` in `server/src/map_draft.rs`, #764): wrote the map to `path`, uncommitted, or wrote nothing since a file
 * there does not read, for `problem`; Accept again with `replace: seen` replaces that file as it was asked about.
 */
export type MapAccepted = { kind: "written"; path: string } | { kind: "replace"; path: string; problem: string; seen: string };

/** What the page asks at start (#527): plotkeeper's package version, for the toast after an update (`whats-new.ts`). */
export type About = { version: string };
/** Why a project header's quiet dot lights (`map.rs`, #386): fog on a Core part, a broken law, a PR brief waiting. */
export type MarkWhy = "fog" | "broken" | "brief";
export type ProjectMark = { project: string; why: MarkWhy[] };

/**
 * The writer section (spec #320): the writers the login shell's `command -v` finds (`understanding/src/writer.rs`), whether
 * summaries are on (off until a writer passes Test for a user who arrived with none) and whether PR briefs can be written,
 * which needs `claude`; and a Test's outcome, with `settings.json`'s new text when the pass saved the choice. An API-key
 * writer is found when it can be called: `anthropic` when its key is set, `openai` (any OpenAI-compatible API, #367) when its
 * key or its base URL is. `keys` lists the API-key writers whose key is set, which is all the page learns of a key; `base_url`
 * is the OpenAI-compatible writer's, none for OpenAI's own.
 */
export type WriterKind = "anthropic" | "openai" | "claude" | "agy" | "codex" | "copilot" | "cursor";
type FoundWriter = { kind: WriterKind; name: string; found: boolean; models: string[]; efforts: string[] };
// `key_store`: where the server keeps the API keys, as the page names it, such as "the macOS Keychain" (#510).
export type Writers = { writers: FoundWriter[]; summaries_on: boolean; briefs: boolean; keys: WriterKind[]; base_url: string | null; key_store: string };
type WriterOutcome = "passed" | "not_logged_in" | "quota" | "timeout" | "not_installed" | "empty" | "other";
export type WriterTest = { outcome: WriterOutcome; line: string; settings: string | null };

/** Why the session didn't touch a name (`ask_why_not`): its recorded facts, and the writer's possible reasons when it has any. */
export type WhyNot = { facts: Facts; reasons: string | null };

// Every command the window invokes (`server/src/api.rs`): its arguments, by the names the page writes them in camel case, and what the
// server replies, `void` where the window ignores the reply, `null` where the server may answer none. An argument the server reads as
// an `Option` may be left out. `invoke` takes only these names with these arguments, and the wire check (`server/tests/wire.rs`, #554)
// fails when a command's argument names differ from its handler's. The snap fixtures (`scripts/snap/`) are typed against the replies,
// and `scripts/snap/base.ts` answers every command that is not `void`, so a fixture that lacks a command or a field fails `tsc` (#144).
type None = Record<string, never>;
type Project = { project: string };
type InPart = { project: string; part: string };
type InPr = { project: string; number: number };
type Expecting = { expected?: string | null };
export interface Commands {
  agent_summary: { args: { agentId: string }; reply: string | null };
  alive: { args: None; reply: void };
  about: { args: None; reply: About };
  answer: { args: { session: string; nonce: string; answers?: Record<string, string | string[]> | null; decision?: "allow" | "deny" | null }; reply: void };
  ask: { args: { session: string; item: string; question: string }; reply: string };
  ask_anchor: { args: { session: string; anchor: Anchor; what: string; question: string }; reply: string };
  ask_facts: { args: { session: string; anchor: Anchor }; reply: Facts | null };
  ask_why_not: { args: { session: string; turn: number; name: string }; reply: WhyNot };
  cards: { args: None; reply: SessionCards };
  check_lists: { args: { lists: Record<string, string[]> }; reply: void };
  check_output: { args: { session: string; agent?: string | null; call: string }; reply: CheckOutput | null };
  delete_session: { args: { id: string }; reply: void };
  failed_check_name: { args: { tail: string; suite: string; worktree: string }; reply: string };
  map_place: { args: Project & { groups: Record<string, string[]> }; reply: Record<string, string[]> };
  map_save_positions: { args: Project & { positions: Record<string, [number, number]> }; reply: UserMap };
  map_save_view: { args: Project & { view: Camera }; reply: void };
  map_set_tier: { args: InPart & { tier?: string | null }; reply: UserMap };
  map_know: { args: InPart; reply: UserMap };
  map_refresher: { args: InPart; reply: UserMap };
  map_history: { args: InPart; reply: HistoryEntry[] };
  map_at: { args: Project & { commit: string }; reply: HistoricalMap };
  map_commit: { args: Project & { commit: string }; reply: CommitFiles };
  map_marks: { args: { projects: string[] }; reply: ProjectMark[] };
  stop_log: { args: { kind: StopKind; project?: string | null; day: string; pr?: number | null }; reply: null };
  stop_facts: { args: Project; reply: StopFacts };
  stop_dismiss: { args: { projects: string[]; modules: string[]; day: string }; reply: null };
  story_bar: {
    args: Project & {
      kind: "explain-back-given" | "explain-back-marked";
      day: string;
      about: { pr: number; chapter: number } | { part: string };
      line?: string | null;
      mark?: "got-it" | "partly" | "missed" | null;
    };
    reply: void;
  };
  map_proof: { args: Project & { law: string }; reply: Proof[] };
  map_state: { args: Project & { fetch: boolean }; reply: MapState };
  map_scan: { args: Project; reply: MapScan };
  map_scan_edit: { args: Project & { map: string; edit: ScanEdit }; reply: MapScan };
  map_draft: { args: Project; reply: MapDraft | null };
  map_draft_start: { args: Project & { skeleton?: string | null }; reply: MapDraft };
  map_draft_retry: { args: Project & { group: number }; reply: MapDraft };
  map_draft_redraft: { args: InPart; reply: MapDraft };
  map_draft_edit: { args: InPart & { name?: string | null; caption?: string | null; purpose?: string | null; tier?: string | null }; reply: MapDraft };
  map_draft_discard: { args: Project; reply: void };
  map_draft_accept: { args: Project & { replace?: string | null }; reply: MapAccepted };
  pr_brief: { args: InPr & { full?: boolean | null }; reply: BriefOpened | null };
  // `older` (#638): "Read the one you have", the stored catch-up of an earlier fog list while it still holds.
  part_catch_up: { args: InPart & { older?: boolean | null }; reply: CatchUpOpened | null };
  part_catch_up_done: { args: InPart & { fog: string[] }; reply: void };
  pr_diff: { args: PrDiffArgs; reply: CommitFiles };
  pre_flight_issues: { args: Project; reply: OpenIssue[] };
  pre_flight_predict: { args: Project & { issue?: number | null; text?: string | null }; reply: PreFlight };
  pre_flight_pick: { args: Project & { parts: string[] }; reply: PreFlight };
  pre_flight_clear: { args: Project; reply: null };
  micro_world_explore: { args: Project & { subject: { kind: "flow"; id: string } | { kind: "figure"; pr: number; chapter: number; step: number } }; reply: Explored };
  micro_world_page: { args: Project & { key: string }; reply: string };
  brief_chapter_done: { args: InPr & { chapter: number; head: string }; reply: void };
  brief_since_done: { args: InPr & { head: string }; reply: void };
  briefs_waiting: { args: { project: string }; reply: WaitingBriefs | null };
  pr_ratify: { args: InPr; reply: RatifyRow[] };
  pr_map_review: { args: InPr; reply: MapReview | null };
  map_review_start: { args: InPr; reply: MapReview | null };
  pr_map_diff: { args: InPr; reply: MapDiff | null };
  ratify_site: { args: Project & { head: string; file: string; line?: number | null }; reply: SiteSnippet };
  projects: { args: None; reply: SavedProject[] };
  remove_project: { args: { path: string }; reply: SavedProject[] };
  ratify_set: { args: Project & { pr: number; kind: string; id: string; state: "accepted" | "overturned" | "pending"; text: string }; reply: void };
  mark_unseen: { args: { id: string }; reply: void };
  open: { args: { agentId: string }; reply: Selection | null };
  rail_shown: { args: { shown: boolean }; reply: void };
  refresh_landed_tasks: { args: None; reply: void };
  remove_api_key: { args: { writer: WriterKind } & Expecting; reply: string | null };
  rename: { args: { id: string; name: string }; reply: void };
  run_cancel: { args: { id: string }; reply: void };
  back_a_law: { args: Project & { law: string; guards?: number | null }; reply: BackLawStarted };
  law_patch_discard: { args: Project & { id: string }; reply: void };
  law_patch_open: { args: Project & { id: string }; reply: LawPatchOpened };
  new_law: { args: Project & { law: NewLaw; guards?: number | null }; reply: BackLawStarted };
  guard_draft: { args: InPr; reply: NewLaw };
  runs_resume: { args: None; reply: void };
  select: { args: { id: string | null }; reply: Selection | null };
  selected: { args: None; reply: Selection | null };
  sessions: { args: None; reply: Session[] };
  set_api_key: { args: { writer: WriterKind; key?: string | null; baseUrl?: string | null } & Expecting; reply: string | null };
  set_explanations: { args: { on: boolean } & Expecting; reply: string };
  settings_files: { args: None; reply: SettingsFiles };
  subagent_log: { args: { session: string; agentId: string; offset: number }; reply: SubagentLog | null };
  summary_settings: { args: { model: string; effort: string; writer: WriterKind; onOpen: boolean }; reply: void };
  test_writer: { args: { writer: WriterKind; model: string; effort: string } & Expecting; reply: WriterTest };
  turn_file_hunk: { args: { session: string; turn: number; root: string; path: string; from?: string | null; all?: boolean | null }; reply: Hunk | null };
  turn_reply: { args: { id: string; index: number }; reply: string | null };
  turn_summary: { args: { id: string }; reply: TurnSummary | null };
  watch: { args: { session?: string | null }; reply: Ask | null };
  wired: { args: None; reply: boolean };
  writer_notice: { args: None; reply: string | null };
  writers: { args: None; reply: Writers };
}
export type Command = keyof Commands;
/** What command `C` takes, by the page's camel-case names. */
export type Args<C extends Command> = Commands[C]["args"];
/** What the server replies to command `C`. */
export type Reply<C extends Command> = Commands[C]["reply"];

/** What the server counted of Daniel's use (`stop_rules.rs`), for the one-time review: days, merged PRs and the uses of each module. */
export type StopKind = "session" | "map-open" | "map-action" | "scrub" | "proof" | "know" | "brief-offered" | "brief-opened" | "chapter-done" | "check-answered" | "ratify" | "unconfirmed-seen" | "explain-back-given" | "explain-back-marked" | "refresher-entered" | "stale-entered" | "catch-up-offered" | "catch-up-opened" | "catch-up-done" | "pre-flight-run" | "micro-world-opened" | "back-law-started" | "guard-offered" | "guard-started";
export type StopFacts = {
  active_days: number;
  merged_prs: number;
  // Active days with at least one entered action on the map.
  engaged_days: number;
  scrubs: number;
  proofs: number;
  knows: number;
  // The modules whose review was dismissed (an older log's project-wide dismissal names the modules of its time).
  dismissed: string[];
  briefs_offered: number;
  briefs_opened: number;
  chapters_done: number;
  // Every kind's count for the project, for the table's bars and offer-count exceptions.
  // `dismissed` counts too: a review dismissed through `stop_dismiss`, never through `logStop`.
  counts: Partial<Record<StopKind | "dismissed", number>>;
  // The local day of the project's first event, where a calendar wait starts at the latest; null when it has none.
  began: string | null;
  // When the project's staleness baselines were set (unix seconds): its first map shown with the staleness build (#392); null before.
  stale_since: number | null;
};
