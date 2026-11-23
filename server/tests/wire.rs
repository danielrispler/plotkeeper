//! The wire's agreement check (#246, #333) for the Map view's payload: `MapState` and the `UserState` the map's commands
//! return, written into `scripts/wire/server.ts` (`scripts/wire/samples.rs`). The values are built whole, every status held
//! once and every optional field filled once and left empty once, so a field added to `MapState` fails to compile here until
//! it has a sample; still, an optional field no sample fills is not checked.

#[path = "../../scripts/wire/samples.rs"]
mod samples;

use samples::{Sample, json, write};
use std::collections::BTreeMap;
use studio_server::{
    ci_laws::{self, Ci},
    fog::{ChapterRead, Entry, FogState, PrRead, Strength},
    map::{MapState, Status, UserState, View},
    pr_briefs::BriefInfo,
    prs::Ready,
    ratify::Pending,
};

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

/// Daniel's state with every field filled: positions, tiers, fog with a brief partly read, and a saved view.
fn user() -> UserState {
    let reads = BTreeMap::from([(
        334,
        PrRead {
            lit: vec![vec!["server".into(), "window".into()], vec!["core".into()]],
            chapters: BTreeMap::from([(
                0,
                ChapterRead {
                    head: COMMIT.into(),
                    at: 1_761_743_990_000,
                },
            )]),
        },
    )]);
    UserState {
        positions: BTreeMap::from([("server".into(), [620.0, 190.5])]),
        tiers: BTreeMap::from([("core".into(), "core".into())]),
        fog: FogState {
            first: Some(COMMIT.into()),
            processed: Some(COMMIT.into()),
            parts: BTreeMap::from([(
                "server".into(),
                vec![Entry {
                    commit: COMMIT.into(),
                    strength: Strength::Thick,
                }],
            )]),
            reads,
        },
        view: Some(View {
            scale: 0.75,
            x: -120.5,
            y: 40.0,
        }),
    }
}

/// CI in each of its statuses, the broken one with every field filled.
fn cis() -> Vec<Ci> {
    use ci_laws::Status::*;
    let bare = |status| Ci {
        status,
        run_url: None,
        commit: None,
        earlier: false,
        parts: Vec::new(),
        laws: Vec::new(),
        tail: String::new(),
        note: None,
    };
    let broken = Ci {
        status: Broken,
        run_url: Some("https://github.com/o/r/actions/runs/1".into()),
        commit: Some(COMMIT.into()),
        earlier: true,
        parts: vec!["server".into()],
        laws: vec!["wire-types-agree".into()],
        tail: "FAIL wire\nSummary 1 failed".into(),
        note: None,
    };
    let unknown = Ci {
        note: Some("gh is not installed".into()),
        ..bare(Unknown)
    };
    vec![
        unknown,
        bare(Passed),
        broken,
        bare(NotRun),
        bare(Unreadable),
    ]
}

/// A ready PR with a brief, and one without.
fn ready() -> Vec<Ready> {
    let brief = BriefInfo {
        stale: true,
        writing: false,
        chapters: vec!["The map's payload".into()],
        read: false,
        quiet: false,
        failed: true,
    };
    vec![
        Ready {
            number: 334,
            title: "PR briefs".into(),
            head: COMMIT.into(),
            branch: "issue-334-pr-briefs".into(),
            brief: Some(brief),
        },
        Ready {
            number: 336,
            title: "Ready for review tray".into(),
            head: COMMIT.into(),
            branch: "issue-336-tray".into(),
            brief: None,
        },
    ]
}

/// The map left empty: no default branch, so nothing else is known.
fn empty(status: Status) -> MapState {
    MapState {
        project: "/repo".into(),
        status,
        source: None,
        reference: None,
        commit: None,
        note: None,
        user: UserState::default(),
        fog: BTreeMap::new(),
        ci: None,
        ready: Vec::new(),
        ratify: Vec::new(),
    }
}

// @law: wire-types-agree
#[test]
fn the_wire_samples_of_the_map_view_are_written_for_tsc() {
    let mut samples: Vec<Sample> = vec![
        ("MapState", json(empty(Status::None))),
        (
            "MapState",
            json(MapState {
                note: Some("system-map.json does not read: expected a value".into()),
                reference: Some("origin/main".into()),
                commit: Some(COMMIT.into()),
                ..empty(Status::Unreadable)
            }),
        ),
    ];
    for ci in cis() {
        samples.push((
            "MapState",
            json(MapState {
                source: Some("{ \"parts\": [] }".into()),
                reference: Some("main".into()),
                commit: Some(COMMIT.into()),
                note: Some("origin/HEAD is not set, so this is the local main".into()),
                user: user(),
                fog: BTreeMap::from([
                    ("server".into(), Strength::Thick),
                    ("core".into(), Strength::Thin),
                ]),
                ci: Some(ci),
                ready: ready(),
                ratify: vec![Pending {
                    pr: 306,
                    kind: "law".into(),
                    id: "shell-notice".into(),
                }],
                ..empty(Status::Loaded)
            }),
        ));
    }
    samples.push(("UserMap", json(user())));
    samples.push(("UserMap", json(UserState::default())));
    write(
        env!("CARGO_MANIFEST_DIR"),
        "server",
        &["MapState", "UserMap"],
        &samples,
    );
}

// @law: wire-types-agree
#[test]
fn the_wire_samples_of_the_writer_section_are_written_for_tsc() {
    use studio_server::{WriterTest, Writers};
    let mut writers = studio_understanding::found_writers(std::path::Path::new("/no/login-shell"));
    writers[3].found = true;
    let samples: Vec<Sample> = vec![
        (
            "Writers",
            json(Writers {
                writers,
                summaries_on: false,
                briefs: true,
                keys: vec![studio_understanding::WriterKind::OpenAI],
                base_url: Some("http://localhost:11434/v1".into()),
            }),
        ),
        (
            "WriterTest",
            json(WriterTest {
                outcome: "passed",
                line: "Passed: `agy` answered. Summaries are written with it.".into(),
                settings: Some("{ \"summaries\": { \"writer\": \"agy\" } }".into()),
            }),
        ),
        (
            "WriterTest",
            json(WriterTest {
                outcome: "not_logged_in",
                line: "`claude` is not logged in.".into(),
                settings: None,
            }),
        ),
    ];
    write(
        env!("CARGO_MANIFEST_DIR"),
        "server-writers",
        &["Writers", "WriterTest"],
        &samples,
    );
}
