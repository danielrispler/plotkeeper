//! The studio core (danielrispler/Falinks#328, module 3): plain Rust with no I/O but the transcript reader
//! (`transcript.rs`), which reads the sessions' transcripts. It takes hook events one at a time, in
//! event log order, and keeps the state the app shows. App start replays the whole log through
//! `replay`; live events go through `apply`, which updates state the same way and also returns what
//! the shell must do.

mod agy;
mod ask;
mod checks;
mod claude;
mod codex;
mod copilot;
mod cursor;
mod event;
mod ledger;
mod notifications;
mod output;
mod qwen;
mod saved;
mod session;
mod studio;
mod transcript;
mod turn_files;
mod work;
pub use agy::{CAPABILITIES as AGY_CAPABILITIES, Drift as AgyDrift, DriftKind as AgyDriftKind};
pub use ask::{Anchor, AskFact, AskFacts, AskRecorded, AskSection};
pub use checks::{
    Check, CheckLists, Listed, Output as CheckOutput, Run as CheckRun, Status as CheckStatus,
    output as check_output,
};
pub use claude::CAPABILITIES as CLAUDE_CAPABILITIES;
pub use codex::CAPABILITIES as CODEX_CAPABILITIES;
pub use copilot::CAPABILITIES as COPILOT_CAPABILITIES;
pub use cursor::CAPABILITIES as CURSOR_CAPABILITIES;
pub use event::{Capabilities, TranscriptFormat};
pub use ledger::{Bytes, Details, Ledger, Outcome, Row};
pub use notifications::{Due, Notifications, Sound};
pub use output::{
    Asker, Base, CardRequest, CatchUp, Change, DELETE, FactFile, Facts, Fingerprint, INTERRUPTED,
    LiveFacts, NameRequest, Output, PROMPT_NAME, Purpose, RENAME, STUDIO_ANSWER, SummaryRequest,
    wants_summary,
};
pub use qwen::CAPABILITIES as QWEN_CAPABILITIES;
pub use session::{
    Agent, Answer, BlockedCommand, BlockedStatus, Decision, Identity, Kind, Parent, Process,
    Prompt, Question, Revive, Sender, Session, SinceSeen, Source, Span, State, Status, Step,
    Stopped, Subagent, Terminal, Turn,
};
pub use studio::{Selection, Studio, TaskFacts, TaskQuery};
pub use transcript::{Context, LogEntry, SubagentLog, SubagentLogFile, Transcript};
pub use turn_files::{
    Numstat, RootDiff, TurnDiff, TurnFile, TurnFiles, TurnFilesSnapshot, TurnTrees,
    compute_turn_files,
};
pub use work::{Estimate, MIN_HISTORY, Phase, PhaseStart, Work, range};
