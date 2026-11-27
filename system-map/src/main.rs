//! `system-map check [--json] [--root <dir>] [--commit <rev>]` checks the repository's system map, or one commit's;
//! `system-map touched [--json] [--root <dir>] [path...]` names the parts some paths touch;
//! `system-map imports [--json] [--root <dir>]` counts the imports inside and across parts.

use std::process::ExitCode;

fn main() -> ExitCode {
    system_map::cli::main(
        "system-map",
        std::env::args().skip(1).collect(),
        &["check", "touched", "imports"],
    )
}
