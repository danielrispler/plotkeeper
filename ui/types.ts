// What the shell sends the window: sessions, the selection and a session's cards.

// `detail` is a command's description, else what the step works on; `command` a command's full text.
export type Step = { tool: string; detail: string | null; command?: string | null };
// What the transcript says of the latest turn (danielrispler/Falinks#472): the agent's latest line, the steps since it,
// its full texts, and why the transcript cannot be read, when it cannot.
// `context`: as Claude Code's status line measures it, from its snapshot: the tokens, the window,
// the percent used and left; with no snapshot, the transcript's tokens only (danielrispler/Falinks#517, decisions 1 and 2; danielrispler/Falinks#519).
// `model` and `effort`: what the session runs at, null when the transcript names none (danielrispler/Falinks#517, decision 8).
export type Context = { tokens: number; window: number | null; share: number | null; left: number | null };
export type Transcript = {
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
export type Identity = { issue: number | null; branch: string | null; pr: number | null; worktree: string; name: string };
export type SessionState = "needs_you" | "running" | "your_turn" | "idle" | "ended";
// When something started and ended, in Unix milliseconds; `end` is null while it runs.
export type Span = { start: number; end: number | null };
// A check that runs or ran, oldest first: a pass or fail line once it ends. `passed` is null when its
// exit code is masked, and while it runs. `typical_ms` is its typical duration when it started (danielrispler/Falinks#473).
export type CheckLine = {
  command: string;
  passed: boolean | null;
  started_at: number;
  ended_at: number | null;
  typical_ms: number | null;
  // The turn it started in, an index into `Session.turns` (danielrispler/Falinks#574).
  turn: number | null;
};
// Who spawned a subagent: the session's own agent, another subagent, or no known spawn.
export type Parent = "session" | "unattached" | { agent: string };
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
  ended_at: number | null;
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
export type Asker = { agent: string | null; since: number };
// Who sent a prompt: Daniel, a wake-up naming the task that returned, or a known task id in markup
// the studio does not know, which counts as Daniel's.
export type Sender = "daniel" | { wake_up: string } | { unrecognised: string };
export type Prompt = { at: number; sender: Sender };
// An AskUserQuestion call, as the tool input gives it, with Daniel's choice for each question by its text
// once he answers in the terminal.
export type Choice = { label: string; description?: string };
// A multi-select answer joins its labels with ", "; a single-choice answer is one whole label.
export type Asked = { question: string; header?: string; multiSelect?: boolean; options: Choice[] };
export type Answer = "open" | "not_answered" | { answered: Record<string, string> };
export type Question = { tool_use_id: string; agent: string | null; questions: Asked[]; answer: Answer; turn: number | null };
// A turn, a block in the rail's feed (danielrispler/Falinks#574): its prompt's first line, or "worker <name> returned";
// `id` names its card and summary once its `Stop` came; `message` is its final reply's first line.
// How a check, or one run of it, ended (danielrispler/plotkeeper#112): "ran" is a check whose exit code does not say.
export type CheckStatus = "passed" | "failed" | "ran";
// A check suite of a turn: the list entry its commands matched, its last run's status, how many earlier runs
// failed, and each run with its own command. `call` and `agent` find a run's output (`check_output`).
export type Check = {
  suite: string;
  status: CheckStatus;
  retries: number;
  runs: { command: string; status: CheckStatus; exit_code: number | null; call: string; agent: string | null }[];
};
export type Turn = {
  started_at: number;
  ended_at: number | null;
  prompt: string;
  id: string | null;
  message: string | null;
  checks: Check[];
  files: TurnFiles | null;
};
// A file the agent or its workers changed in a turn: its worktree's root, its path from there (a rename's new one),
// a rename's old path, and its lines added and removed (null for a binary file).
export type TurnFile = { root: string; path: string; from: string | null; added: number | null; removed: number | null };
// What a turn changed: the files its ledger says its agent or workers touched, and how many other files differ.
// `null` while the trees are not yet diffed, and for an agent that reports no trees.
export type TurnFiles = { files: TurnFile[]; external: number };
// What the session's agent adapter reports, which decides what its view shows (danielrispler/plotkeeper#43, decision 4);
// `agent` is its mark's name, empty for a status-only row.
export type Capabilities = {
  agent: string;
  asks: boolean;
  background_tasks: boolean;
  replies: boolean;
  permission_mode: boolean;
  durations: boolean;
  transcript_version: boolean;
  // It records a git tree of each worktree a turn touches, so a turn has files (danielrispler/plotkeeper#111).
  turn_trees: boolean;
};
export type Session = {
  id: string;
  capabilities: Capabilities;
  // The model its agent's hooks name (agy's), null otherwise; Claude's is `transcript.model`.
  model: string | null;
  // The folder it works in, from its latest event; `folder_name` is its last segment.
  cwd: string;
  folder_name: string;
  // The studio terminal it runs in, its `STUDIO_TERMINAL_ID`; null when it runs outside the studio.
  terminal: string | null;
  // Another agent than Claude Code, from its terminal's OSC 7501 status only (danielrispler/Falinks#560): no intention,
  // card, brief, estimate or decisions. Its id is its terminal's.
  status_only: boolean;
  // A status-only row of a plain terminal New terminal started, while no agent in it reports a status.
  shell: boolean;
  // Daniel's name for it from the studio (danielrispler/Falinks#469); null for its folder's.
  name: string | null;
  // A few words on what it is about, written from Daniel's first prompt; null before it arrives.
  prompt_name: string | null;
  // The id `claude --resume` takes: its latest conversation's; a `/clear` changes it, not `id` (danielrispler/Falinks#474).
  resume_id: string;
  // Claude Code's permission mode from its latest event that names one, null before (danielrispler/Falinks#541).
  permission_mode: string | null;
  // The mode it started in, from its first event that names one, which a resume keeps (danielrispler/Falinks#541).
  started_mode: string | null;
  // When each `/clear` continued the row, in Unix ms: its "cleared" markers.
  cleared: number[];
  // Closing its terminal asks first: it is in a turn or has background tasks (danielrispler/Falinks#469).
  asks_before_closing: boolean;
  // Its task has landed (the core's rule): shown as a check mark once the session is at your turn, idle or ended.
  landed: boolean;
  last_step: Step | null;
  // When its first and latest events came, its latest turn's and its last step's times (danielrispler/Falinks#473).
  started_at: number;
  last_event_at: number;
  turn_span: Span | null;
  step_span: Span | null;
  state: SessionState;
  checks: CheckLine[];
  agents: Subagent[];
  asker: Asker | null;
  prompts: Prompt[];
  turns: Turn[];
  questions: Question[];
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
  // How many of its turns Daniel has seen (danielrispler/plotkeeper#124).
  seen: number;
  // What happened after the turns he had seen when he selected it, from the core's `Session::since`, as `since_looked` (the agent has its own `since`): the banner's count.
  since_looked: Since;
};
// What happened in a session since Daniel looked (danielrispler/plotkeeper#124).
export type Since = { turns: number; files: number; passed: number; failed: number; ran: number };
// The shell sends a card's request without its diff and base, which the pane never reads. `agent`
// is the worker whose card it is; `changed_by` holds the files other subagents edited in its
// worktree, by subagent. `check` is the command of the check whose start requested the card (danielrispler/Falinks#476).
export type CardRequest = {
  session_id: string;
  id: string;
  agent: string | null;
  changed_by: Record<string, string[]>;
  check: string | null;
};
// A check's result, which settles the guess on its card; null while the check runs.
export type CheckResult = "passed" | "failed" | "not_counted";
// A card's multiple-choice question; `key` is the right option, counting from 0.
export type Quiz = { text: string; options: { text: string; why: string }[]; key: number };
// `architecture` is null when the change is not structural.
export type Card = {
  summary: string;
  explanation: string;
  paths: string[];
  code: string;
  why_it_matters: string;
  not_checked: string;
  architecture: string | null;
  quiz: Quiz | null;
};
// Daniel's tap on a quiz, graded by the writer's key; on a check's card by its result, so `right`
// is null until the result settles it, and stays null when the result does not say (danielrispler/Falinks#476).
export type Guess = { option: number; right: boolean | null; at: number };
// Daniel's rating of a card's explanation. His fifth reason, "the key is wrong", is `key_wrong`, the
// one record the quiz's flag also sets.
export type Level = "good" | "fair" | "poor";
export type Reason = "unclear" | "wrong" | "too_shallow" | "too_easy";
export type Rating = { level: Level; reasons: Reason[] };
// The store also records when the quiz showed; the pane never shows it, nor any rate. `more` is the
// longer explanation from "explain more".
export type StoredCard = {
  request: CardRequest;
  card: Card;
  guess: Guess | null;
  key_wrong: boolean;
  more: string | null;
  rating: Rating | null;
  check_result: CheckResult | null;
  taps?: QuizTap[];
};
// What Daniel did with a Quiz besides a card's pick (danielrispler/plotkeeper#120), each with its time: his pick (a lone Quiz's), "Show
// the rest anyway", and the three chips.
// The trial's two (#129): how sure he was before the pick, and one optional line on why after it.
export type Tap =
  | { kind: "pick"; option: number }
  | { kind: "confidence"; sure: boolean }
  | { kind: "why"; text: string }
  | { kind: "shown" | "hunk" }
  | { kind: "skip" | "understood" | "explain_more" | "key_wrong" };
export type QuizTap = Tap & { at: number };
// A fresh Quiz on the decision of an earlier one, a week after it, which Home offers (#129). `id` names it for `quiz_tap`.
export type Retest = { id: string; of: string; session_id: string; added_at: number; quiz: Quiz | null; taps: QuizTap[] };
// The selected session's brief, and its cards, split at 30 minutes by the shell's tick, each oldest
// first.
// An answered question, plan or permission in the session's decision history (danielrispler/Falinks#479): the call's input,
// Daniel's answers by question text or "allow"/"deny", when, and where he answered.
export type Decision = {
  call: string;
  agent: string | null;
  tool: string;
  input: { questions?: Asked[]; plan?: string; command?: string; file_path?: string; url?: string };
  answer: Record<string, string> | "allow" | "deny";
  at: number;
  source: "card" | "terminal";
};
// A turn's summary (danielrispler/Falinks#526) and, unless the turn has a card, its Quiz, as the store keeps it; a
// learning section stored before danielrispler/plotkeeper#118 reads as its question.
// `headline` and `lines` are the turn writer's Headline and typed Body lines (danielrispler/plotkeeper#115); a summary stored
// before them has neither, and its `summary` text reads as a Headline and `reported` lines.
export type SummaryLine = { kind: "why" | "reported" | "open" | "ask"; text: string };
export type TurnSummary = {
  summary: { summary: string; headline?: string | null; lines?: SummaryLine[]; quiz: Quiz | null };
  taps?: QuizTap[];
  // The self-test's arm (#130): body-only shows the Body open and no Quiz; absent is quiz-first.
  arm?: "quiz_first" | "body_only" | null;
};
// The Brief's slots (danielrispler/plotkeeper#122): the Goal a model wrote, each answered question with Daniel's reply, and
// the Open lines the shell computed. `brief` is a brief stored as prose, which shows while there is no Goal.
export type OpenItem = {
  kind: "question" | "check" | "unchecked" | "open" | "ask" | "unpushed" | "pr";
  text: string;
};
// A line of "Design so far" (danielrispler/plotkeeper#123): a decision with its reason, and the id of the turn or card it came from.
export type DesignLine = { text: string; cite: string };
export type SessionCards = {
  brief: string | null;
  goal: string | null;
  answers: { question: string; reply: string }[];
  open: OpenItem[];
  design: DesignLine[];
  recent: StoredCard[];
  older: StoredCard[];
  decisions: Decision[];
};
// What a call asks Daniel, by its tool, as the answer hook's `Kind`: a question, a plan, or any other
// tool's permission.
export type AskKind = "question" | "plan" | "permission";
export const askKind = (tool: string): AskKind =>
  tool === "AskUserQuestion" ? "question" : tool === "ExitPlanMode" ? "plan" : "permission";
// An entry of a subagent's log, as the core reads it from the log file, and what one read returned: the entries
// after the offset asked for, and the offset read to.
export type LogEntry =
  | { kind: "prompt"; text: string }
  | { kind: "reply"; text: string }
  | { kind: "call"; id: string; tool: string; detail: string | null }
  | { kind: "result"; id: string; text: string; failed: boolean; cut: boolean };
export type SubagentLog = { entries: LogEntry[]; offset: number };
// A log as drawn: each call with its result, null while it runs.
export type LogResult = { text: string; failed: boolean; cut: boolean };
export type LogRow =
  | { kind: "prompt"; text: string }
  | { kind: "reply"; text: string }
  | LogCall;
export type LogCall = { kind: "call"; id: string; tool: string; detail: string | null; result: LogResult | null };

// The selection is the core's: the session Daniel selected and the subagent of it he opened, whose
// view then shows its own line.
export type Selection = { session: Session; agent: string | null };

// Where a session can start (danielrispler/Falinks#467): the main checkout and the tasks under `.claude/worktrees/`, with
// the count of `agent-*` worktrees hidden. `branch` is null at a detached HEAD.
export type Workspace = { path: string; branch: string | null };
export type Workspaces = { main: Workspace; tasks: Workspace[]; hidden: number };
// A workspace's git state (#26): its branch, null at a detached HEAD; commits ahead of and behind its upstream, null with
// none; changed files; commits behind origin's default branch, null when level with it or it is unknown; whether the
// branch is the default branch; whether a rebase onto it would conflict; and the operation in progress, null with none.
export type GitState = { branch: string | null; upstream: [number, number] | null; changed: number; main_behind: number | null; on_default: boolean; would_conflict: boolean; would_overwrite?: boolean; operation: string | null };
export type Losses = { uncommitted: string[]; unpushed: string[] };

export const stateLabels: Record<SessionState, string> = {
  needs_you: "Needs you",
  running: "Running",
  your_turn: "Your turn",
  idle: "Idle",
  ended: "Ended",
};

// A phase of a session's work on a ticket, in the usual order (danielrispler/Falinks#475).
export type Phase =
  | "orienting"
  | "designing"
  | "implementing"
  | "checking"
  | "reviewing"
  | "opening_pr"
  | "waiting_on_ci"
  | "merged";

// A session's work: its kind (`implement`), its phases with when each started (Unix ms) and after how
// much of the agent's time, and the estimate set as its latest phase started: the range of the
// remaining agent time in ms, null for "no estimate", and whether it waits on Daniel now.
export type Work = {
  kind: string | null;
  phases: { phase: Phase; at: number; agent_ms: number }[];
  agent_ms: number;
  estimate: { range: [number, number] | null; depends_on_you: boolean };
};

// A branch the main checkout can switch to (#95): `remote` when only `origin` has it, `task` the folder of the task
// whose worktree holds it, `current` the main checkout's own.
export type Branch = { name: string; remote: boolean; task: string | null; current: boolean };
