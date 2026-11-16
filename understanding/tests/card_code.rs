//! A card's code comes from git (danielrispler/plotkeeper#121): the writer names a path and a line range, and the card
//! shows those lines of the turn's end tree, in a real temporary repository.

use std::{collections::BTreeMap, fs, path::Path, process::Command};
use studio_core::CardRequest;
use studio_understanding::{Card, CardWriter, write_card};

struct Replying(String);

impl CardWriter for Replying {
    fn reply(&self, _: &CardRequest, _: &[String]) -> Result<String, String> {
        Ok(self.0.clone())
    }

    fn more(&self, _: &CardRequest, _: &Card) -> Result<String, String> {
        Err("no".into())
    }
}

fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args([
            "-c",
            "user.name=Studio",
            "-c",
            "user.email=studio@example.com",
        ])
        .args(args)
        .current_dir(repo)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}");
    String::from_utf8(out.stdout).unwrap().trim().into()
}

/// A repository whose end tree has `lib.rs` of five lines, and the request that names it.
fn request() -> (tempfile::TempDir, CardRequest) {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git(repo, &["init", "-q"]);
    fs::write(repo.join("lib.rs"), "one\ntwo\nthree\nfour\nfive\n").unwrap();
    git(repo, &["add", "."]);
    git(repo, &["commit", "-qm", "Start"]);
    let tree = git(repo, &["rev-parse", "HEAD^{tree}"]);
    // The file changes after the turn ended: the card reads the tree, not the worktree.
    fs::write(repo.join("lib.rs"), "changed\n").unwrap();
    let ends = BTreeMap::from([(repo.to_str().unwrap().to_owned(), tree)]);
    let request = CardRequest {
        id: "s1@1".into(),
        change: Some("a diff".into()),
        ends,
        ..CardRequest::default()
    };
    (dir, request)
}

fn reply(ranges: &str, code: &str) -> String {
    format!(
        r#"{{"summary": "s", "explanation": "e", "paths": ["lib.rs"], "ranges": {ranges}, {code}"why_it_matters": "w",
        "not_checked": "n", "architecture": "structural: none"}}"#
    )
}

fn written(reply: String, request: &CardRequest) -> Card {
    write_card(&Replying(reply), request, &[], |_| {}).unwrap()
}

#[test]
fn a_named_range_shows_exactly_those_lines_of_the_end_tree() {
    let (_dir, request) = request();
    let reply = reply(r#"[{"path": "lib.rs", "from": 2, "to": 3}]"#, "");
    assert_eq!(written(reply, &request).code, "lib.rs:2-3\ntwo\nthree");
}

#[test]
fn a_range_past_the_end_or_a_path_not_in_the_tree_drops_that_block_and_keeps_the_card() {
    let (_dir, request) = request();
    let ranges = r#"[{"path": "lib.rs", "from": 4, "to": 9}, {"path": "gone.rs", "from": 1, "to": 1},
        {"path": "lib.rs", "from": 5, "to": 5}]"#;
    let card = written(reply(ranges, ""), &request);
    assert_eq!(card.code, "lib.rs:5-5\nfive");
    assert_eq!(card.explanation, "e");

    let none = written(
        reply(r#"[{"path": "lib.rs", "from": 6, "to": 6}]"#, ""),
        &request,
    );
    assert_eq!((none.code.as_str(), none.explanation.as_str()), ("", "e"));
}

#[test]
fn code_the_writer_retyped_is_not_kept() {
    let (_dir, request) = request();
    let card = written(reply("[]", r#""code": "made up", "#), &request);
    assert_eq!(card.code, "");
}

#[test]
fn a_card_stored_before_ranges_keeps_its_code() {
    let stored = r#"{"explanation": "e", "paths": ["lib.rs"], "code": "+new"}"#;
    let card: Card = serde_json::from_str(stored).unwrap();
    assert_eq!(card.code, "+new");
    assert!(card.ranges.is_empty());
}
