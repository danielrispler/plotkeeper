//! The answer hook's command (#461, decision 16; see `studio_hook::answer`). It prints a decision on
//! stdout only when Daniel answers from the studio card; otherwise it prints nothing, so the terminal
//! asks as today. It always exits 0.

use std::{env, error::Error, io::Read, time::Duration};
use studio_hook::answer;

fn main() {
    match run() {
        Ok(Some(out)) => println!("{out}"),
        Ok(None) => {}
        Err(e) => eprintln!("studio-answer: {e}"),
    }
}

fn run() -> Result<Option<serde_json::Value>, Box<dyn Error>> {
    let mut payload = Vec::new();
    std::io::stdin().read_to_end(&mut payload)?;
    let payload = serde_json::from_slice(&payload)?;
    let data = studio_hook::data_dir(&env::home_dir().ok_or("no home directory")?);
    // Tests shorten the 120 seconds.
    let timeout = env::var("STUDIO_ANSWER_TIMEOUT_MS")
        .ok()
        .and_then(|ms| ms.parse().ok());
    let timeout = Duration::from_millis(timeout.unwrap_or(answer::TIMEOUT_MS));
    Ok(answer::run(&payload, &data, timeout)?)
}
