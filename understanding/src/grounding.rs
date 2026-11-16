//! The turn writer's facts and the guards that hold its reply to them (danielrispler/plotkeeper#115).
//! The prompt asks for grounded lines; these checks are in code, so a reply that ignores the prompt
//! is still held to what was recorded.

use std::collections::BTreeSet;
use studio_core::{CheckStatus, Facts, SummaryRequest};

/// A hunk's lines in the writer's input; the whole hunk still counts as material.
const HUNK_LINES: usize = 40;

/// The writer's input of all hunks, in characters.
const HUNKS_MAX: usize = 6000;

/// The most words a Headline has.
pub(crate) const HEADLINE_WORDS: usize = 16;

/// Words that claim the turn's work was fixed, passes or was verified.
const CLAIMS: [&str; 7] = [
    "fixed", "fixes", "passes", "passing", "passed", "verified", "verifies",
];

/// The facts as the writer's input holds them: files with counts, checks, and the hunks up to a cap.
pub(crate) fn facts_text(facts: &Facts) -> String {
    if facts.files.is_empty() && facts.checks.is_empty() {
        return "none recorded\n".into();
    }
    let mut text = String::from("Files the agent changed:\n");
    for f in &facts.files {
        let count = |n: Option<u64>| n.map_or("?".into(), |n| n.to_string());
        let (added, removed) = (count(f.file.added), count(f.file.removed));
        text += &format!("- {} +{added} -{removed}\n", f.file.path);
    }
    if facts.files.is_empty() {
        text += "none\n";
    }
    if facts.external > 0 {
        text += &format!("({} other files changed by someone else)\n", facts.external);
    }
    text += "Checks, each with its last run's result:\n";
    for c in &facts.checks {
        let retries = match c.retries {
            0 => String::new(),
            n => format!(", after {n} earlier failed runs"),
        };
        let command = c.runs.last().map_or("", |r| r.command.as_str());
        text += &format!("- {command}: {:?}{retries}\n", c.status);
    }
    if facts.checks.is_empty() {
        text += "none\n";
    }
    let mut hunks = String::new();
    for f in &facts.files {
        let lines: Vec<&str> = f.hunk.lines().collect();
        let cut = lines.len().saturating_sub(HUNK_LINES);
        let mut hunk = lines[..lines.len() - cut].join("\n");
        if cut > 0 {
            hunk += &format!("\n... {cut} more lines");
        }
        hunks += &format!("{hunk}\n");
    }
    let end = hunks.floor_char_boundary(HUNKS_MAX);
    text + "Hunks:\n" + &hunks[..end]
}

/// Everything a number or id in the reply may come from: the writer's input, the turn's whole
/// hunks, its line counts and its check facts.
pub(crate) fn material(request: &SummaryRequest, earlier: &str) -> String {
    let mut text = format!(
        "{}\n{}\n{}\n{earlier}\n",
        request.prompt,
        request.responses.join("\n"),
        request.intentions.join("\n")
    );
    let facts = &request.facts;
    text += &format!("{}\n", facts_text(facts));
    text += &format!("{} {}\n", facts.files.len(), facts.external);
    for f in &facts.files {
        text += &format!("{}\n{}\n", f.file.path, f.hunk);
        text += &format!("{:?} {:?}\n", f.file.added, f.file.removed);
    }
    for c in &facts.checks {
        text += &format!("{} {} {}\n", c.suite, c.retries, c.runs.len());
        for r in &c.runs {
            text += &format!("{} {:?}\n", r.command, r.exit_code);
        }
    }
    text
}

/// The numbers in `text`: each run of digits, with a fractional part when it has one.
fn numbers(text: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let bytes: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len()
            && (bytes[i].is_ascii_digit()
                || bytes[i] == ',' && bytes.get(i + 1).is_some_and(char::is_ascii_digit))
        {
            i += 1;
        }
        if bytes.get(i) == Some(&'.') && bytes.get(i + 1).is_some_and(char::is_ascii_digit) {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
        }
        found.insert(bytes[start..i].iter().filter(|c| **c != ',').collect());
    }
    found
}

/// Whether every number and id in `line` is in `material`. Numbers written as words are not
/// checked, nor a path or code name with digits that the material holds.
pub(crate) fn grounded(line: &str, material: &str) -> bool {
    let known = numbers(material);
    let lower = material.to_lowercase();
    line.split_whitespace().all(|word| {
        let token = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '#');
        if !token.chars().any(|c| c.is_ascii_digit()) {
            return true;
        }
        let hex = token.len() >= 7 && token.chars().all(|c| c.is_ascii_hexdigit());
        let sha = hex && token.chars().any(|c| c.is_ascii_alphabetic());
        let number_shaped = token.starts_with(|c: char| c.is_ascii_digit() || c == '#');
        if sha || !number_shaped {
            // A SHA, or a path or a name with digits: it has to be there as written.
            let token = word.trim_matches(|c: char| !c.is_alphanumeric() && !"/_-.".contains(c));
            let token = token.trim_matches('.');
            return lower.contains(&token.to_lowercase());
        }
        numbers(token).iter().all(|n| known.contains(n))
    })
}

/// Whether `headline` claims a fix, a pass or a verification.
fn claims(headline: &str) -> bool {
    let lower = headline.to_lowercase();
    let words = lower.split(|c: char| !c.is_alphanumeric());
    words.into_iter().any(|w| CLAIMS.contains(&w))
}

/// `headline`, prefixed "Agent says:" when it claims a fix, a pass or a verification that no check
/// of the turn backs: a check ended passed, and none ended failed.
pub(crate) fn agent_says(headline: &str, facts: &Facts) -> String {
    let passed = facts.checks.iter().any(|c| c.status == CheckStatus::Passed);
    let failed = facts.checks.iter().any(|c| c.status == CheckStatus::Failed);
    match claims(headline) && !(passed && !failed) && !headline.starts_with("Agent says:") {
        true => format!("Agent says: {headline}"),
        false => headline.into(),
    }
}

/// How many words `text` has.
pub(crate) fn words(text: &str) -> usize {
    text.split_whitespace().count()
}
