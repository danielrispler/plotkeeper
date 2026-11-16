//! Prints the self-test's table (danielrispler/plotkeeper#130): per arm, the turns, the retests answered and right, the turns
//! whose hunk was opened and the median time to pick, from the card store beside the event log.

use studio_understanding::{CardStore, self_test};

fn main() -> std::io::Result<()> {
    let log = studio_hook::event_log(&std::env::home_dir().expect("a home directory"));
    let store = CardStore::open_beside(&log)?;
    print!("{}", self_test(store.summaries(), store.retests()));
    Ok(())
}
