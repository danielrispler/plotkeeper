//! `system-map check [--json] [--root <dir>]`: checks the repository's system map.

use std::{path::PathBuf, process::Command, process::ExitCode};

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(message) => {
            eprintln!("system-map: {message}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<ExitCode, String> {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some("check") {
        return Err("usage: system-map check [--json] [--root <dir>]".into());
    }
    let mut json = false;
    let mut root = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--root" => root = Some(PathBuf::from(args.next().ok_or("--root needs a folder")?)),
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    let root = match root {
        Some(root) => root,
        None => git_top_level()?,
    };
    let report = system_map::check(&root);
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
        );
    } else {
        for problem in &report.problems {
            println!("{problem}");
        }
    }
    Ok(if report.problems.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn git_top_level() -> Result<PathBuf, String> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if !output.status.success() {
        return Err("not inside a git repository; pass --root <dir>".into());
    }
    Ok(PathBuf::from(
        String::from_utf8_lossy(&output.stdout).trim_end(),
    ))
}
