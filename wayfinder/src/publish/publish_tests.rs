use super::*;
use serde_json::json;
const BODY: &str = "## What to build\n\nX.\n\n## Reading\n\n- None.\n\n## Touches\n\n- **Files:** `a.rs`\n- **Session:** auto mode\n- **Route:** cheap · effort medium — one check in one file\n";
#[test]
fn a_complete_body_has_no_problems() {
    assert!(body_problems(BODY).is_empty());
}
#[test]
fn a_body_names_each_section_and_line_it_lacks() {
    assert_eq!(
        body_problems("## What to build\n\nX.\n"),
        ["lacks ## Reading", "lacks ## Touches"]
    );
    let no_lines = "## Reading\n\nNone.\n\n## Touches\n\nTBD.\n\n## Notes\n\n- **Files:** `a.rs`\n- **Session:** auto mode\n";
    assert_eq!(
        body_problems(no_lines),
        [
            "lacks a **Files:** line in ## Touches",
            "lacks a **Session:** line in ## Touches",
            "lacks a **Route:** line in ## Touches"
        ]
    );
}
fn manifest() -> Value {
    json!({"parent":102,"tickets":[
        {"file":"02.md","title":"Studio: two","labels":["studio"],"blocked_by":["01.md"]},
        {"file":"01.md","title":"Studio: one","labels":["studio","ready-for-agent"]}
    ]})
}
#[test]
fn the_manifest_gives_the_parent_and_each_ticket() {
    let manifest = parse_manifest(&manifest()).unwrap();
    assert_eq!(manifest.parent, 102);
    assert_eq!(
        manifest.tickets[0],
        Ticket {
            file: "02.md".into(),
            title: "Studio: two".into(),
            labels: vec!["studio".into()],
            blocked_by: vec!["01.md".into()],
        }
    );
    assert!(manifest.tickets[1].blocked_by.is_empty());
}
#[test]
fn a_manifest_missing_a_field_names_it() {
    let mut no_parent = manifest();
    no_parent["parent"] = json!(null);
    let error = parse_manifest(&no_parent).unwrap_err().to_string();
    assert!(error.contains("parent"), "{error}");
    let mut no_title = manifest();
    no_title["tickets"][1]["title"] = json!(null);
    let error = parse_manifest(&no_title).unwrap_err().to_string();
    assert!(error.contains("01.md: missing title"), "{error}");
}
fn ticket(file: &str, blocked_by: &[&str]) -> Ticket {
    Ticket {
        file: file.into(),
        title: format!("Studio: {file}"),
        labels: vec!["studio".into()],
        blocked_by: blocked_by.iter().map(ToString::to_string).collect(),
    }
}
fn files(order: &[&Ticket]) -> Vec<String> {
    order.iter().map(|t| t.file.clone()).collect()
}
#[test]
fn blockers_come_first_and_manifest_order_holds_otherwise() {
    let manifest = Manifest {
        parent: 9,
        tickets: vec![
            ticket("c.md", &["b.md"]),
            ticket("a.md", &[]),
            ticket("b.md", &["a.md"]),
            ticket("d.md", &[]),
        ],
    };
    assert_eq!(
        files(&publish_order(&manifest).unwrap()),
        ["a.md", "b.md", "c.md", "d.md"]
    );
}
#[test]
fn a_cycle_is_an_error_naming_its_files() {
    let manifest = Manifest {
        parent: 9,
        tickets: vec![
            ticket("a.md", &[]),
            ticket("b.md", &["c.md"]),
            ticket("c.md", &["b.md"]),
            // Stuck behind the cycle, but not on it.
            ticket("d.md", &["b.md"]),
        ],
    };
    let error = publish_order(&manifest).unwrap_err().to_string();
    assert!(error.contains("cycle"), "{error}");
    assert!(error.contains("b.md, c.md"), "{error}");
    assert!(
        !error.contains("a.md") && !error.contains("d.md"),
        "{error}"
    );
}
fn labels() -> Vec<String> {
    vec!["studio".into(), "ready-for-agent".into()]
}
fn bodies(files: &[&str]) -> std::collections::HashMap<String, String> {
    files
        .iter()
        .map(|f| (f.to_string(), BODY.to_owned()))
        .collect()
}
#[test]
fn a_sound_folder_has_no_problems() {
    let manifest = parse_manifest(&manifest()).unwrap();
    assert!(publish_problems(&manifest, &bodies(&["01.md", "02.md"]), &labels()).is_empty());
    let mut shouting = manifest;
    shouting.tickets[0].labels = vec!["Studio".into()];
    assert!(publish_problems(&shouting, &bodies(&["01.md", "02.md"]), &labels()).is_empty());
}
#[test]
fn each_problem_names_its_file() {
    let manifest = Manifest {
        parent: 9,
        tickets: vec![
            ticket("a.md", &["gone.md"]),
            Ticket {
                labels: vec!["studio".into(), "wishlist".into()],
                ..ticket("b.md", &[])
            },
            ticket("c.md", &[]),
            ticket("b.md", &[]),
        ],
    };
    let mut bodies = bodies(&["a.md", "b.md"]);
    bodies.insert("a.md".into(), "## Reading\n\nNone.\n".into());
    assert_eq!(
        publish_problems(&manifest, &bodies, &labels()),
        [
            "a.md: lacks ## Touches",
            "a.md: blocker gone.md is not a ticket of tickets.json",
            "b.md: label wishlist does not exist in the repository",
            "c.md: no such file in the folder",
            "b.md: listed twice in tickets.json",
        ]
    );
}
/// The problems of one ticket `a.md` whose body is `BODY` with its Route line replaced by `route`.
fn problems_with_route(route: &str) -> Vec<String> {
    problems_with("`a.rs`", route)
}
/// The problems of one ticket `a.md` whose body is `BODY` with its Files line naming `files` and
/// its Route line replaced by `route`.
fn problems_with(files: &str, route: &str) -> Vec<String> {
    let manifest = Manifest {
        parent: 9,
        tickets: vec![ticket("a.md", &[])],
    };
    let body = BODY
        .replace(
            "- **Route:** cheap · effort medium — one check in one file\n",
            route,
        )
        .replace("- **Files:** `a.rs`", &format!("- **Files:** {files}"));
    let bodies = std::collections::HashMap::from([("a.md".to_owned(), body)]);
    publish_problems(&manifest, &bodies, &labels())
}
#[test]
fn a_body_without_a_route_is_refused() {
    assert_eq!(
        problems_with_route(""),
        ["a.md: lacks a **Route:** line in ## Touches"]
    );
}
#[test]
fn a_route_with_an_unknown_tier_or_effort_is_refused() {
    assert_eq!(
        problems_with_route("- **Route:** sonnet · effort medium — a reason\n"),
        ["a.md: **Route:** tier sonnet is not strong, medium or cheap"]
    );
    assert_eq!(
        problems_with_route("- **Route:** cheap · effort low — a reason\n"),
        ["a.md: **Route:** effort low is not medium or high"]
    );
    assert_eq!(
        problems_with_route("- **Route:** cheap, medium\n"),
        ["a.md: **Route:** reads `cheap, medium`, not `<tier> · effort <effort> — <reason>`"]
    );
}
#[test]
fn a_cheap_route_over_ui_or_three_source_files_is_refused() {
    const CHEAP: &str = "- **Route:** cheap · effort medium — one check in one file\n";
    const MEDIUM: &str = "- **Route:** medium · effort medium — presentational UI\n";
    let cases: [(&str, &str, &[&str]); 6] = [
        (
            "`server/src/a.rs`, `ui/rail.ts`, new `scripts/snap/b.mjs`, `docs/c.html`, `d.css`",
            CHEAP,
            &[
                "a.md: **Route:** cheap, but **Files:** names UI (`ui/rail.ts`, `scripts/snap/b.mjs`, `docs/c.html`, `d.css`), and UI is never cheap (issue-tracker.md, Routing, rule 3)",
                "a.md: **Route:** cheap, but **Files:** names 5 source files (`server/src/a.rs`, `ui/rail.ts`, `scripts/snap/b.mjs`, `docs/c.html`, `d.css`), over the about three of rule 5 (issue-tracker.md, Routing)",
            ],
        ),
        ("`server/src/a.rs`, `docs/ui.md`", CHEAP, &[]),
        ("`ui/rail.ts`", MEDIUM, &[]),
        (
            "`a.rs`, `b.rs`, `c.mjs`, new `d.rs`, `docs/e.md`",
            CHEAP,
            &[
                "a.md: **Route:** cheap, but **Files:** names 4 source files (`a.rs`, `b.rs`, `c.mjs`, `d.rs`), over the about three of rule 5 (issue-tracker.md, Routing)",
            ],
        ),
        // Three source files and their tests.
        (
            "`a/src/a.rs`, `a/src/b.rs`, `c.mjs`, `a/tests/it.rs`, `a/src/tests.rs`, `scripts/review/c.test.mjs`, `docs/e.md`",
            CHEAP,
            &[],
        ),
        ("`a.rs`, `b.rs`, `c.rs`, `d.rs`", MEDIUM, &[]),
    ];
    for (files, route, expected) in cases {
        assert_eq!(problems_with(files, route), expected, "{files} {route}");
    }
}
#[test]
fn a_high_effort_whose_reason_names_no_risk_is_refused() {
    assert_eq!(
        problems_with_route("- **Route:** medium · effort high — five files and a new module\n"),
        [
            "a.md: **Route:** effort high, but its reason names no risk (persistence, stored, queue, concurrency, wire, redraw, layout, handler, trust boundary, untested edge case); effort is medium otherwise (issue-tracker.md, Routing)"
        ]
    );
    for reason in [
        "a Stored type gains a field",
        "the event queues",
        "two handlers across a redraw",
        "crosses the trust boundary",
        "an untested edge case in the parser",
    ] {
        let route = format!("- **Route:** strong · effort high — {reason}\n");
        assert!(problems_with_route(&route).is_empty(), "{reason}");
    }
}
/// A ticket of `checked_folder`: its file, its blockers, its Files line and its Route line.
type Row<'a> = (&'a str, &'a [&'a str], &'a str, &'a str);
/// The problems of a folder whose tickets are `rows`, each body `BODY` with its Files and Route
/// lines replaced and `extra` added to its What to build.
fn checked_folder(rows: &[Row], extra: &str) -> Vec<String> {
    let manifest = Manifest {
        parent: 9,
        tickets: rows
            .iter()
            .map(|(file, blocked_by, _, _)| ticket(file, blocked_by))
            .collect(),
    };
    let bodies = rows
        .iter()
        .map(|(file, _, files, route)| {
            let body = BODY
                .replace("X.\n", &format!("X.{extra}\n"))
                .replace("- **Files:** `a.rs`", &format!("- **Files:** {files}"))
                .replace("cheap · effort medium — one check in one file", route);
            (file.to_string(), body)
        })
        .collect();
    publish_problems(&manifest, &bodies, &labels())
}
#[test]
fn a_shared_source_file_needs_a_blocker_path_either_way() {
    const MEDIUM: &str = "medium · effort medium — a reason";
    let cases: [(&str, &[Row], &str, &[&str]); 5] = [
        (
            "two tickets naming one source file with no edge",
            &[
                ("a.md", &[], "`x.rs`, `docs/a.md`", MEDIUM),
                ("b.md", &[], "`x.rs`, `docs/a.md`", MEDIUM),
            ],
            "",
            &[
                "a.md, b.md: both name `x.rs` in **Files:**, with no blocker path between them in either direction",
            ],
        ),
        (
            "a folder holding the other ticket's file, no edge",
            &[
                ("a.md", &[], "`wayfinder/`", MEDIUM),
                ("b.md", &[], "`wayfinder/src/publish.rs`", MEDIUM),
                ("c.md", &[], "`server/`", MEDIUM),
            ],
            "",
            &[
                "a.md, b.md: both name `wayfinder/src/publish.rs` in **Files:**, with no blocker path between them in either direction",
            ],
        ),
        (
            "a shared file with an edge from the second to the first",
            &[
                ("a.md", &[], "`x.rs`", MEDIUM),
                ("b.md", &["a.md"], "`x.rs`", MEDIUM),
            ],
            "",
            &[],
        ),
        (
            "a shared file with an edge from the first to the second, through a third",
            &[
                ("a.md", &["c.md"], "`x.rs`", MEDIUM),
                ("b.md", &[], "`x.rs`", MEDIUM),
                ("c.md", &["b.md"], "`y.rs`", MEDIUM),
            ],
            "",
            &[],
        ),
        (
            "two tickets both naming system-map.json and a Markdown file",
            &[
                ("a.md", &[], "`system-map.json`, `README.md`", MEDIUM),
                ("b.md", &[], "`system-map.json`, `README.md`", MEDIUM),
            ],
            "",
            &[],
        ),
    ];
    for (case, rows, extra, expected) in cases {
        assert_eq!(checked_folder(rows, extra), expected, "{case}");
    }
}
#[test]
fn a_body_that_says_to_run_a_global_install_is_refused() {
    const MEDIUM: &str = "medium · effort medium — a reason";
    let cases: [(&str, &[Row], &str, &[&str]); 5] = [
        (
            "a body that says to run cargo install",
            &[("a.md", &[], "`a.rs`", MEDIUM)],
            " Install it with `cargo install --locked cargo-llvm-cov`.",
            &[
                "a.md: says to run `cargo install`, a global install auto mode blocks: hand Daniel the install before /implement-spec starts",
            ],
        ),
        (
            "a body that says to run npm i -g and brew install",
            &[("a.md", &[], "`a.rs`", MEDIUM)],
            " Run `npm i -g x`, then `brew install y`.",
            &[
                "a.md: says to run `npm i -g`, a global install auto mode blocks: hand Daniel the install before /implement-spec starts",
                "a.md: says to run `brew install`, a global install auto mode blocks: hand Daniel the install before /implement-spec starts",
            ],
        ),
        (
            "a body that says to run npm install -g",
            &[("a.md", &[], "`a.rs`", MEDIUM)],
            " Run `npm install -g x`.",
            &[
                "a.md: says to run `npm install -g`, a global install auto mode blocks: hand Daniel the install before /implement-spec starts",
            ],
        ),
        (
            "a body that says to run npm install --global",
            &[("a.md", &[], "`a.rs`", MEDIUM)],
            " Run `npm install --global x`.",
            &[
                "a.md: says to run `npm install --global`, a global install auto mode blocks: hand Daniel the install before /implement-spec starts",
            ],
        ),
        (
            "a body that says to run rustup component add",
            &[("a.md", &[], "`a.rs`", MEDIUM)],
            " Run `rustup component add llvm-tools`.",
            &[
                "a.md: says to run `rustup component add`, a global install auto mode blocks: hand Daniel the install before /implement-spec starts",
            ],
        ),
    ];
    for (case, rows, extra, expected) in cases {
        assert_eq!(checked_folder(rows, extra), expected, "{case}");
    }
}
#[test]
fn a_cycle_is_a_problem() {
    let manifest = Manifest {
        parent: 9,
        tickets: vec![ticket("a.md", &["b.md"]), ticket("b.md", &["a.md"])],
    };
    assert_eq!(
        publish_problems(&manifest, &bodies(&["a.md", "b.md"]), &labels()),
        ["blockers form a cycle: a.md, b.md"]
    );
}
#[test]
fn a_first_run_creates_links_and_adds_edges_ticket_by_ticket() {
    let (a, b) = (ticket("a.md", &[]), ticket("b.md", &["a.md"]));
    assert_eq!(
        publish_steps(&[&a, &b], &json!({})),
        [
            Step::Create(&a),
            Step::Link(&a),
            Step::Create(&b),
            Step::Link(&b),
            Step::Edge(&b, "a.md"),
        ]
    );
}
#[test]
fn a_second_run_does_only_what_is_missing() {
    let (a, b, c) = (
        ticket("a.md", &[]),
        ticket("b.md", &["a.md"]),
        ticket("c.md", &["a.md", "b.md"]),
    );
    let published = json!({
        "a.md": {"number": 10, "linked": true, "edges": []},
        "b.md": {"number": 11, "linked": true, "edges": ["a.md"]},
        "c.md": {"number": 12, "linked": false, "edges": ["b.md"]},
    });
    assert_eq!(
        publish_steps(&[&a, &b, &c], &published),
        [Step::Link(&c), Step::Edge(&c, "a.md")]
    );
}
