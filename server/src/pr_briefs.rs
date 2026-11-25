//! The PR briefs of ready PRs (spec danielrispler/plotkeeper#334, ticket #339). When a PR becomes ready the server writes its
//! brief and stores it per PR and head; a new head marks it stale, and it is rewritten when opened or once two consecutive
//! lookups see the same head. The lookups are `prs.rs`'s (Landed's triggers, never a timer).
//!
//! `write_brief` is the one place the server calls the understanding crate's `write_pr_brief`, so the claim check and the retry
//! of ticket #338 take its place here.

mod commands;
mod push;
mod queue;
mod stale;
#[cfg(test)]
mod tests;
mod view;
mod write;

pub use commands::*;
pub use push::*;
pub use queue::*;
pub use stale::*;
pub use view::*;
pub use write::*;
