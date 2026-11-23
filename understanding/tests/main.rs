//! The crate's one test binary: each file beside this one is a module of it.

mod ask;
mod brief;
mod common;
mod contract;
mod decisions;
mod drift;
mod fingerprint;
mod http_writer;
mod pr_brief;
mod pre_flight;
#[path = "../../scripts/wire/samples.rs"]
mod samples;
mod saved_state;
mod store;
mod stored_fixtures;
mod turn_trees;
mod turn_writing;
mod wire;
mod writer;
