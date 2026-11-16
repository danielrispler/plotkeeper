//! What a turn gets for a Quiz (danielrispler/plotkeeper#119): a card with its Quiz, a lone Quiz, or
//! nothing, by the size of its change; the rules are in `docs/studio/question-rules.md`.

use studio_core::{CheckStatus, Facts};

/// Changed lines, in logic files, from which a turn gets a card.
const CARD_LINES: u64 = 25;
/// Logic files from which a turn gets a card.
const CARD_FILES: usize = 2;
/// Non-trivial lines from which a turn gets a lone Quiz.
const QUIZ_LINES: u64 = 8;

/// What a turn gets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Offer {
    Card,
    Quiz,
    Nothing,
}

/// A file a turn changed, as the table reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Changed {
    pub path: String,
    /// Lines added and removed.
    pub lines: u64,
    /// Of those, the non-trivial ones (`non_trivial`).
    pub non_trivial: u64,
}

/// A turn as the table reads it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shape {
    pub files: Vec<Changed>,
    /// A check ended failed.
    pub failed: bool,
    /// The turn ran `git merge` or merged a pull request.
    pub merge: bool,
}

impl Shape {
    /// The shape of a turn with `facts`, whose files carry their hunks; a file whose hunk could not
    /// be read counts all its lines as non-trivial. A turn with no files in `facts` reads `diff`,
    /// the change of its card request, which an agent that reports no trees leaves.
    pub fn of(facts: &Facts, diff: &str, merge: bool) -> Shape {
        let failed = facts.checks.iter().any(|c| c.status == CheckStatus::Failed);
        let files = match facts.files.is_empty() {
            true => of_diff(diff),
            false => facts
                .files
                .iter()
                .map(|f| {
                    let lines = f.file.added.unwrap_or(0) + f.file.removed.unwrap_or(0);
                    let non_trivial = match f.hunk.is_empty() {
                        true => lines,
                        false => non_trivial(&f.hunk),
                    };
                    Changed {
                        path: f.file.path.clone(),
                        lines,
                        non_trivial,
                    }
                })
                .collect(),
        };
        Shape {
            files,
            failed,
            merge,
        }
    }

    fn logic(&self) -> impl Iterator<Item = &Changed> {
        self.files.iter().filter(|f| is_logic(&f.path))
    }

    /// What the turn gets, by the table.
    pub fn offer(&self) -> Offer {
        let (files, lines) = self.logic().fold((0, 0), |(n, l), f| (n + 1, l + f.lines));
        if lines >= CARD_LINES || files >= CARD_FILES {
            return Offer::Card;
        }
        let non_trivial: u64 = self.logic().map(|f| f.non_trivial).sum();
        let test_run = files > 0 && self.logic().all(|f| is_test(&f.path));
        match non_trivial >= QUIZ_LINES && !self.failed && !self.merge && !test_run {
            true => Offer::Quiz,
            false => Offer::Nothing,
        }
    }
}

/// The files of a unified diff (`git diff`, one `diff --git` header per file).
fn of_diff(diff: &str) -> Vec<Changed> {
    let mut files: Vec<(String, String)> = Vec::new();
    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            let path = rest.rsplit_once(" b/").map_or(rest, |(_, b)| b);
            files.push((path.into(), String::new()));
        } else if let Some((_, text)) = files.last_mut() {
            text.push_str(line);
            text.push('\n');
        }
    }
    files
        .into_iter()
        .map(|(path, text)| Changed {
            path,
            lines: changes(&text).count() as u64,
            non_trivial: non_trivial(&text),
        })
        .collect()
}

/// The added and removed lines of a diff, with their sign.
fn changes(diff: &str) -> impl Iterator<Item = &str> {
    let header = |l: &&str| !(l.starts_with("+++") || l.starts_with("---"));
    diff.lines()
        .filter(|l| l.starts_with('+') || l.starts_with('-'))
        .filter(header)
}

/// How many changed lines of `diff` are non-trivial: not blank, a comment, an import, a lone
/// bracket, or formatting only (the line is also removed or added with different spacing).
fn non_trivial(diff: &str) -> u64 {
    let squeeze = |l: &str| -> String { l.split_whitespace().collect() };
    let (added, removed): (Vec<&str>, Vec<&str>) = changes(diff).partition(|l| l.starts_with('+'));
    let squeezed =
        |lines: &[&str]| -> Vec<String> { lines.iter().map(|l| squeeze(&l[1..])).collect() };
    let (added_sq, removed_sq) = (squeezed(&added), squeezed(&removed));
    let moved = |text: &String, other: &Vec<String>| other.contains(text);
    let count = |lines: &[&str], squeezed: &[String], other: &Vec<String>| {
        let kept = lines.iter().zip(squeezed);
        kept.filter(|(l, sq)| !trivial(&l[1..]) && !moved(sq, other))
            .count() as u64
    };
    count(&added, &added_sq, &removed_sq) + count(&removed, &removed_sq, &added_sq)
}

/// A line that is blank, a comment, an import or only brackets and punctuation.
fn trivial(line: &str) -> bool {
    let line = line.trim();
    const COMMENTS: [&str; 6] = ["//", "#", "/*", "*", "--", "<!--"];
    const IMPORTS: [&str; 6] = [
        "use ", "import ", "from ", "#include", "pub use ", "export {",
    ];
    line.chars().all(|c| !c.is_alphanumeric())
        || COMMENTS.iter().any(|c| line.starts_with(c))
        || IMPORTS.iter().any(|i| line.starts_with(i))
}

/// Whether `path` is a logic file: not documentation, a lockfile, generated output or a fixture.
pub fn is_logic(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    let segment = |s: &[&str]| path.split('/').any(|p| s.contains(&p));
    let ends = |e: &[&str]| e.iter().any(|e| name.ends_with(e));
    let docs = ends(&[".md", ".mdx", ".txt", ".rst"])
        || segment(&["docs"])
        || name.starts_with("README")
        || name.starts_with("LICENSE");
    let lock = ends(&[".lock", "package-lock.json", "pnpm-lock.yaml", "go.sum"]);
    let generated = segment(&[
        "dist",
        "target",
        "node_modules",
        "generated",
        "__snapshots__",
    ]) || ends(&[".min.js", ".snap", ".pb.go", ".generated.ts"]);
    let fixture = segment(&["fixtures", "fixture", "testdata"]);
    !(docs || lock || generated || fixture)
}

/// Whether `path` is a test file.
fn is_test(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    path.split('/')
        .any(|p| matches!(p, "tests" | "test" | "__tests__"))
        || name.contains(".test.")
        || name.contains(".spec.")
        || name.starts_with("test_")
        || name
            .split('.')
            .next()
            .is_some_and(|stem| stem.ends_with("_test"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, lines: u64, non_trivial: u64) -> Changed {
        Changed {
            path: path.into(),
            lines,
            non_trivial,
        }
    }

    fn shape(files: Vec<Changed>) -> Shape {
        Shape {
            files,
            ..Shape::default()
        }
    }

    #[test]
    fn a_card_at_25_lines_or_two_logic_files() {
        let one = |lines| shape(vec![file("a.rs", lines, lines)]).offer();
        assert_eq!(one(24), Offer::Quiz);
        assert_eq!(one(25), Offer::Card);
        let two = shape(vec![file("a.rs", 3, 3), file("b.rs", 3, 3)]).offer();
        assert_eq!(two, Offer::Card);
        // A doc beside a logic file is one logic file.
        let doc = shape(vec![file("a.rs", 9, 9), file("docs/x.md", 90, 90)]).offer();
        assert_eq!(doc, Offer::Quiz);
    }

    #[test]
    fn a_lone_quiz_at_8_non_trivial_lines() {
        let one = |n| shape(vec![file("a.rs", 12, n)]).offer();
        assert_eq!(one(7), Offer::Nothing);
        assert_eq!(one(8), Offer::Quiz);
    }

    #[test]
    fn a_failed_check_a_merge_or_a_test_run_gets_no_lone_quiz() {
        let base = shape(vec![file("a.rs", 12, 12)]);
        let failed = Shape {
            failed: true,
            ..base.clone()
        };
        let merge = Shape {
            merge: true,
            ..base.clone()
        };
        let tests = shape(vec![file("tests/a.rs", 12, 12)]);
        let spec = shape(vec![file("ui/a.test.mjs", 12, 12)]);
        for s in [failed, merge, tests, spec] {
            assert_eq!(s.offer(), Offer::Nothing);
        }
        assert_eq!(base.offer(), Offer::Quiz);
    }

    #[test]
    fn docs_lockfiles_fixtures_and_generated_output_are_not_logic() {
        for path in [
            "README.md",
            "docs/studio/a.rs",
            "Cargo.lock",
            "package-lock.json",
            "core/tests/fixtures/run.json",
            "ui/dist/main.js",
        ] {
            assert!(!is_logic(path), "{path}");
            assert_eq!(shape(vec![file(path, 90, 90)]).offer(), Offer::Nothing);
        }
        assert!(is_logic("core/src/lib.rs") && is_logic("core/tests/a.rs"));
    }

    #[test]
    fn blank_comment_import_and_formatting_lines_are_trivial() {
        let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1,3 +1,9 @@\n\
                    +use std::fs;\n+\n+// a note\n+}\n+let a = 1;\n-let   b = 2;\n+let b = 2;\n+let c = 3;\n";
        let files = of_diff(diff);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].lines, 8);
        // `let a`, `let c`: the reindented `let b` is formatting only.
        assert_eq!(files[0].non_trivial, 2);
    }
}
