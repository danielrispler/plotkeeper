// What the shell sends the window: sessions, the selection and a session's cards.

// `detail` is a command's description, else what the step works on; `command` a command's full text.
export type Step = { tool: string; detail: string | null; command?: string | null };
// What the transcript says of the latest turn (#472): the agent's latest line, the steps since it,
// its full texts, and why the transcript cannot be read, when it cannot.
// `context`: the input tokens of the agent's latest message out of its model's window (#480).
export type Context = { tokens: number; window: number };
export type Transcript = {
  intention: string | null;
  steps_ago: number;
  responses: string[];
  unavailable: string | null;
  context: Context | null;
  branch: string | null;
};
// What names a session (#480): its issue, branch, PR and worktree, and the name derived from them.
export type Identity = { issue: number | null; branch: string | null; pr: number | null; worktree: string; name: string };
export type SessionState = "needs_you" | "running" | "your_turn" | "idle" | "ended";
// When something started and ended, in Unix milliseconds; `end` is null while it runs.
export type Span = { start: number; end: number | null };
// A check that runs or ran, oldest first: a pass or fail line once it ends. `passed` is null when its
// exit code is masked, and while it runs. `typical_ms` is its typical duration when it started (#473).
export type CheckLine = {
  command: string;
  passed: boolean | null;
  started_at: number;
  ended_at: number | null;
  typical_ms: number | null;
};
// Who spawned a subagent: the session's own agent, another subagent, or no known spawn.
export type Parent = "session" | "unattached" | { agent: string };
export type Subagent = {
  id: string;
  parent: Parent;
  kind: "worker" | "helper";
  // Its spawn's description, else its short id: the core's one name for it.
  name: string;
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
export type Question = { tool_use_id: string; agent: string | null; questions: Asked[]; answer: Answer };
export type Session = {
  id: string;
  // The folder it works in, from its latest event; `folder_name` is its last segment.
  cwd: string;
  folder_name: string;
  // The studio terminal it runs in, its `STUDIO_TERMINAL_ID`; null when it runs outside the studio.
  terminal: string | null;
  // Daniel's name for it from the studio (#469); null for its folder's.
  name: string | null;
  // The id `claude --resume` takes: its latest conversation's; a `/clear` changes it, not `id` (#474).
  resume_id: string;
  // When each `/clear` continued the row, in Unix ms: its "cleared" markers.
  cleared: number[];
  // Closing its terminal asks first: it is in a turn or has background tasks (#469).
  asks_before_closing: boolean;
  last_step: Step | null;
  // When its first and latest events came, its latest turn's and its last step's times (#473).
  started_at: number;
  last_event_at: number;
  turn_span: Span | null;
  step_span: Span | null;
  state: SessionState;
  checks: CheckLine[];
  agents: Subagent[];
  asker: Asker | null;
  prompts: Prompt[];
  questions: Question[];
  // "worker <name> returned", after a wake-up until its next step or Daniel's next prompt.
  returned: string | null;
  transcript: Transcript;
  identity: Identity;
  // Its phases and the estimate (#475).
  work: Work;
  // In the session list only: what its row says, built by the card store.
  line?: string;
};
// The shell sends a card's request without its diff and base, which the pane never reads. `agent`
// is the worker whose card it is; `changed_by` holds the files other subagents edited in its
// worktree, by subagent.
export type CardRequest = {
  session_id: string;
  id: string;
  agent: string | null;
  changed_by: Record<string, string[]>;
};
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
// Daniel's tap on a quiz, graded by the writer's key.
export type Guess = { option: number; right: boolean; at: number };
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
};
// The selected session's brief, and its cards, split at 30 minutes by the shell's tick, each oldest
// first.
// An answered question, plan or permission in the session's decision history (#479): the call's input,
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
export type SessionCards = { brief: string | null; recent: StoredCard[]; older: StoredCard[]; decisions: Decision[] };
// What a call asks Daniel, by its tool, as the answer hook's `Kind`: a question, a plan, or any other
// tool's permission.
export type AskKind = "question" | "plan" | "permission";
export const askKind = (tool: string): AskKind =>
  tool === "AskUserQuestion" ? "question" : tool === "ExitPlanMode" ? "plan" : "permission";
// The selection is the core's: the session Daniel selected and the subagent of it he opened, whose
// view then shows its own line.
export type Selection = { session: Session; agent: string | null };

// Where a session can start (#467): the main checkout and the tasks under `.claude/worktrees/`, with
// the count of `agent-*` worktrees hidden. `branch` is null at a detached HEAD.
export type Place = { path: string; branch: string | null };
export type Places = { main: Place; tasks: Place[]; hidden: number };
export type Losses = { uncommitted: string[]; unpushed: string[] };

export const stateLabels: Record<SessionState, string> = {
  needs_you: "Needs you",
  running: "Running",
  your_turn: "Your turn",
  idle: "Idle",
  ended: "Ended",
};

// A phase of a session's work on a ticket, in the usual order (#475).
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
