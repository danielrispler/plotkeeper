//! The answer hook's command (danielrispler/Falinks#461, decision 16; see `studio_hook::answer`). It prints a decision on
//! stdout only when Daniel answers from the plotkeeper card; otherwise it prints nothing, so the terminal
//! asks as today. It always exits 0.

use std::{env, error::Error, io::Read, time::Duration};
use studio_hook::answer;

fn main() {
    match run() {
        Ok(Some(out)) => println!("{out}"),
        Ok(None) => {}
        Err(e) => eprintln!("plotkeeper-answer: {e}"),
    }
}

fn run() -> Result<Option<serde_json::Value>, Box<dyn Error>> {
    let mut payload = Vec::new();
    std::io::stdin().read_to_end(&mut payload)?;
    let payload: serde_json::Value = serde_json::from_slice(&payload)?;
    // Another harness's payload, which reached this entry because that harness runs Claude Code's plugin hooks too,
    // raises no card here: the harness asks as it does without plotkeeper (spec #320).
    let mut args = env::args().skip(1);
    let agent = args
        .by_ref()
        .find(|a| a == "--agent")
        .and_then(|_| args.next());
    if let Some(object) = payload.as_object()
        && let Some(harness) = studio_hook::foreign(agent.as_deref(), object)
    {
        eprintln!(
            "plotkeeper-answer: let {harness}'s {} event through on the {} entry",
            payload["hook_event_name"].as_str().unwrap_or("-"),
            agent.as_deref().unwrap_or("-"),
        );
        return Ok(None);
    }
    let data = studio_hook::hook_data_dir(&env::home_dir().ok_or("no home directory")?);
    // Tests shorten the 120 seconds.
    let timeout = env::var("PLOTKEEPER_ANSWER_TIMEOUT_MS")
        .ok()
        .and_then(|ms| ms.parse().ok());
    let timeout = Duration::from_millis(timeout.unwrap_or(answer::TIMEOUT_MS));
    Ok(answer::run(&payload, &data, timeout)?)
}
