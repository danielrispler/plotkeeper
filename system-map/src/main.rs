//! `system-map check [--json] [--root <dir>] [--commit <rev>]` checks the repository's system map, or one commit's;
//! `system-map touched [--json] [--root <dir>] [path...]` names the parts some paths touch;
//! `system-map imports [--json] [--root <dir>]` counts the imports inside and across parts.

use std::{
    io::Write,
    path::{Path, PathBuf},
    process::ExitCode,
    time::Duration,
};

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
    let command = args.next();
    if !matches!(command.as_deref(), Some("check" | "touched" | "imports")) {
        return Err(
            "usage: system-map check [--json] [--root <dir>] [--commit <rev>] | system-map touched [--json] [--root <dir>] [path...] | system-map imports [--json] [--root <dir>]"
                .into(),
        );
    }
    let touched = command.as_deref() == Some("touched");
    let mut json = false;
    let mut root = None;
    let mut commit = None;
    let mut paths = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--root" => root = Some(PathBuf::from(args.next().ok_or("--root needs a folder")?)),
            "--commit" if command.as_deref() == Some("check") => {
                commit = Some(args.next().ok_or("--commit needs a commit")?);
            }
            other if touched && !other.starts_with("--") => paths.push(other.to_string()),
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    let root = match root {
        Some(root) => root,
        None => git_top_level()?,
    };
    if touched {
        return run_touched(&root, paths, json);
    }
    if command.as_deref() == Some("imports") {
        return run_imports(&root, json);
    }
    let report = match &commit {
        Some(commit) => system_map::check_at(&root, commit),
        None => system_map::check(&root),
    };
    print_check(
        &report,
        json,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
    )
    .map_err(|e| e.to_string())?;
    Ok(if report.problems.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// Prints what `check` found: its notes on `err`, so `out` keeps exactly the problems and the pass line, or the JSON, and
/// never a failure.
fn print_check(
    report: &system_map::Report,
    json: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> std::io::Result<()> {
    for name in &report.suggested {
        writeln!(
            err,
            "note: {name} could read this repository; add it to extractors"
        )?;
    }
    for note in report.unsure_notes() {
        writeln!(err, "note: {note}")?;
    }
    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(report)?)?;
    } else {
        for problem in &report.problems {
            writeln!(out, "{problem}")?;
        }
        if report.problems.is_empty() {
            writeln!(out, "{}", pass_line(&report.counts))?;
        }
    }
    Ok(())
}

/// The one line a pass prints, such as `ok: 7 parts own 581 files (212 ignored); 18 links: 9 import (9 proven, 0 unproven),
/// 5 http, 3 file, 1 spawn; 12 laws: 9 check, 3 trust`.
fn pass_line(counts: &system_map::Counts) -> String {
    let n =
        |count: usize, noun: &str| format!("{count} {noun}{}", if count == 1 { "" } else { "s" });
    format!(
        "ok: {} own {} ({} ignored); {}: {} import ({} proven, {} unproven), {} http, {} file, {} spawn; {}: {} check, {} trust",
        n(counts.parts, "part"),
        n(counts.files, "file"),
        counts.ignored,
        n(counts.links.total, "link"),
        counts.links.import,
        counts.links.proven,
        counts.links.unproven,
        counts.links.http,
        counts.links.file,
        counts.links.spawn,
        n(counts.laws.check + counts.laws.trust, "law"),
        counts.laws.check,
        counts.laws.trust
    )
}

/// `touched`: paths from the arguments, or one per line on standard input when there are none.
fn run_touched(root: &Path, mut paths: Vec<String>, json: bool) -> Result<ExitCode, String> {
    if paths.is_empty() {
        for line in std::io::stdin().lines() {
            let line = line.map_err(|e| format!("cannot read standard input: {e}"))?;
            if !line.is_empty() {
                paths.push(line);
            }
        }
    }
    let found = match system_map::touched(root, &paths) {
        Ok(found) => found,
        Err(problems) => {
            for problem in problems {
                eprintln!("{problem}");
            }
            return Ok(ExitCode::from(2));
        }
    };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&found).map_err(|e| e.to_string())?
        );
    } else {
        for part in &found.parts {
            println!("part {}: {}", part.id, part.files.join(", "));
        }
        if !found.ignored.is_empty() {
            println!("ignored: {}", found.ignored.join(", "));
        }
        if !found.unowned.is_empty() {
            println!("unowned: {}", found.unowned.join(", "));
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// `imports`: exits 0 on any counts, and 2 with the problems of a map that does not load.
fn run_imports(root: &Path, json: bool) -> Result<ExitCode, String> {
    let found = match system_map::imports(root) {
        Ok(found) => found,
        Err(problems) => {
            for problem in problems {
                eprintln!("{problem}");
            }
            return Ok(ExitCode::from(2));
        }
    };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&found).map_err(|e| e.to_string())?
        );
        return Ok(ExitCode::SUCCESS);
    }
    for problem in &found.problems {
        println!("note: {problem}");
    }
    println!(
        "{} imports between owned files: {} inside parts, {} across ({:.1}% raw, {:.1}% weighted)",
        found.total.raw,
        found.inside.raw,
        found.across.raw,
        found.across_share.raw,
        found.across_share.weighted
    );
    let rule = format!("degree at least 3 x the mean of {:.2}", found.mean_degree);
    if found.hubs.is_empty() {
        println!("hubs: none ({rule})");
    } else {
        println!("hubs ({rule}):");
    }
    for hub in &found.hubs {
        println!(
            "  {} ({}): degree {}, {} in, {} out",
            hub.file, hub.part, hub.degree, hub.fan_in, hub.fan_out
        );
    }
    println!("inside:");
    for part in &found.parts {
        println!(
            "  {}: {} (weighted {:.2})",
            part.part, part.count.raw, part.count.weighted
        );
    }
    println!(
        "across:{}",
        if found.between.is_empty() {
            " none"
        } else {
            ""
        }
    );
    for pair in &found.between {
        println!(
            "  {} -> {}: {} (weighted {:.2})",
            pair.from, pair.to, pair.count.raw, pair.count.weighted
        );
    }
    println!(
        "both ways:{}",
        if found.both_ways.is_empty() {
            " none"
        } else {
            ""
        }
    );
    for [a, b] in &found.both_ways {
        println!("  {a} <-> {b}");
    }
    Ok(ExitCode::SUCCESS)
}

/// The longest finding the repository's root may take.
const GIT_LIMIT: Duration = Duration::from_secs(30);

fn git_top_level() -> Result<PathBuf, String> {
    use studio_workbench::run::{RunError, git, run_until};
    let mut git = git(Path::new("."));
    let found = run_until(git.args(["rev-parse", "--show-toplevel"]), GIT_LIMIT);
    let output = found.map_err(|e| match e {
        RunError::Failed(_) => "not inside a git repository; pass --root <dir>".to_string(),
        e => format!("cannot run git: {e}"),
    })?;
    Ok(PathBuf::from(
        String::from_utf8_lossy(&output.stdout).trim_end(),
    ))
}

#[cfg(test)]
mod tests {
    use system_map::{PartFiles, Report, Unsure};

    use super::print_check;

    /// A report whose part `p` owns `a.x`, unsure of `example.com/x` with candidates in `p`, `q` and `r`.
    fn unsure_report() -> Report {
        let part = |id: &str, files: &[&str]| PartFiles {
            id: id.into(),
            files: files.iter().map(ToString::to_string).collect(),
        };
        Report {
            parts: vec![
                part("p", &["a.x", "a2.x"]),
                part("q", &["b.x"]),
                part("r", &["c.x"]),
            ],
            problems: vec!["a problem".into()],
            unsure: vec![Unsure {
                file: "a.x".into(),
                name: "example.com/x".into(),
                reason: "declared twice".into(),
                candidates: vec!["a2.x".into(), "b.x".into(), "c.x".into()],
            }],
            ..Report::default()
        }
    }

    /// What `print_check` writes to standard output and to standard error.
    fn printed(report: &Report, json: bool) -> (String, String) {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        print_check(report, json, &mut out, &mut err).unwrap();
        (
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        )
    }

    #[test]
    fn an_unsure_file_is_noted_on_standard_error_and_not_standard_output() {
        let (out, err) = printed(&unsure_report(), false);
        assert_eq!(
            err,
            "note: a.x is unsure of example.com/x; its links to q, r pass as unproven\n"
        );
        assert_eq!(out, "a problem\n");
    }

    #[test]
    fn json_lists_the_unsure_files_with_their_candidates() {
        let (out, err) = printed(&unsure_report(), true);
        let value: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            value["unsure"],
            serde_json::json!([{
                "file": "a.x",
                "name": "example.com/x",
                "reason": "declared twice",
                "candidates": ["a2.x", "b.x", "c.x"]
            }])
        );
        assert!(err.contains("note: a.x is unsure of"), "{err}");
    }
}
