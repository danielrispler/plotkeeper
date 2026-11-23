//! `plotkeeper-run <run's folder>`: the launcher of one background run (spec danielrispler/plotkeeper#378, ticket #390).
//! plotkeeper starts it, in a process group of its own, and it outlives plotkeeper: it runs the run's setup command and
//! its harness, keeps the run's time limit, and writes the final status (`studio_server::runner::launch`). Not run by hand.

use std::{path::Path, process::ExitCode};

fn main() -> ExitCode {
    let Some(dir) = std::env::args_os().nth(1) else {
        eprintln!("usage: plotkeeper-run <run's folder>");
        return ExitCode::FAILURE;
    };
    match studio_server::runner::launch(Path::new(&dir)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("plotkeeper-run: {e}");
            ExitCode::FAILURE
        }
    }
}
