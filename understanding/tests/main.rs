//! The crate's one test binary: each file beside this one is a module of it.

mod ask;
mod backfill;
mod brief;
mod card_code;
mod check_cards;
mod common;
mod decisions;
mod drift;
mod fingerprint;
mod offer;
mod pr_brief;
#[path = "../../scripts/wire/samples.rs"]
mod samples;
mod saved_state;
mod self_test;
mod store;
mod stored_fixtures;
mod turn_trees;
mod turn_writing;
mod week;
mod wire;
mod writer;
