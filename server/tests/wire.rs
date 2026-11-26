//! The wire's agreement check (#246, #333) for the Map view's payload: `MapState`, the `UserState` the map's commands
//! return and a project header's `Mark`, written into `scripts/wire/server.ts` (`scripts/wire/samples.rs`). The values are
//! built whole, every status held once and every optional field filled once and left empty once, so a field added to
//! `MapState` fails to compile here until it has a sample; still, an optional field no sample fills is not checked.

#[path = "../../scripts/wire/samples.rs"]
mod samples;

use samples::{Sample, json, write};
use std::collections::BTreeMap;
use studio_server::{
    back_a_law::{LawPatch, PatchState, Proof},
    ci_laws::{self, Ci},
    fog::{ChapterRead, Entry, FogState, PrRead, Strength},
    map::{MapState, Mark, MarkWhy, Status, UserState, View},
    micro_worlds::{Phase, View as WorldView, WorldsState},
    pr_briefs::BriefInfo,
    prs::Ready,
    ratify::Pending,
    runner::{End, Kind, RunState, RunView, Runs},
    staleness::{Activity, Staleness, Touch},
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
                vec![
                    Entry {
                        commit: COMMIT.into(),
                        strength: Strength::Thick,
                        pr: Some(334),
                        why: vec!["law wire-types-agree".into(), "paths".into()],
                    },
                    Entry {
                        commit: COMMIT.into(),
                        strength: Strength::Thin,
                        pr: None,
                        why: Vec::new(),
                    },
                ],
            )]),
            reads,
        },
        view: Some(View {
            scale: 0.75,
            x: -120.5,
            y: 40.0,
        }),
        staleness: Staleness {
            since: Some(1_761_743_990),
            parts: BTreeMap::from([(
                "server".into(),
                Touch {
                    at: 1_761_744_000,
                    kind: Activity::Refresher,
                },
            )]),
        },
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
        at: None,
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
        at: Some("2026-11-24T10:00:00Z".into()),
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
        note: Some("Rebased onto main, nothing new in the PR".into()),
        since: Some("What changed since you read".into()),
        since_chapters: vec!["The read".into(), "The fog".into()],
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
        runs: Runs::default(),
        catch_ups: BTreeMap::new(),
        stale: BTreeMap::new(),
        stale_why: BTreeMap::new(),
        pre_flight: None,
        micro_worlds: WorldsState::default(),
        law_patches: Vec::new(),
    }
}

/// A micro-world in each phase: building over an older page, ready and out of date, and failed with its prompt.
fn worlds() -> WorldsState {
    let view = |phase, page, stale, failed: bool| WorldView {
        phase,
        page,
        stale,
        line: failed.then(|| "verify.mjs failed:\nFAIL no horizontal overflow @390/light".into()),
        prompt: failed.then(|| "In the repository at /repo, build a micro-world".into()),
    };
    WorldsState {
        on: true,
        worlds: BTreeMap::from([
            (
                "flow:turn".into(),
                view(Phase::Building, true, false, false),
            ),
            ("flow:look".into(), view(Phase::Ready, true, true, false)),
            (
                "figure:334:0:1".into(),
                view(Phase::Failed, false, false, true),
            ),
        ]),
    }
}

/// A back a law run's patch in each state: being proven, rejected, proven with its diff and proof, out of date, and opened
/// as a draft PR; and a new law's, started from a brief's Guard with a law (#401).
fn law_patches() -> Vec<LawPatch> {
    let patch = |id: &str, state, line: Option<&str>| LawPatch {
        id: id.into(),
        project: "/repo".into(),
        law: "core-order".into(),
        state,
        line: line.map(String::from),
        at: 1_761_743_990_000,
        diff: None,
        proof: None,
        pr: None,
        url: None,
        text: None,
        guards: None,
    };
    vec![
        patch("19a2f-1", PatchState::Proving, None),
        patch(
            "19a2f-2",
            PatchState::Rejected,
            Some("The test patch touches src/a.rs"),
        ),
        LawPatch {
            diff: Some("diff --git a/tests/a.rs b/tests/a.rs\n".into()),
            proof: Some(Proof {
                test: "events_apply_in_order".into(),
                file: "core/tests/order.rs".into(),
                passed: "PASS [ 0.01s] studio-core::it order::events_apply_in_order".into(),
                failed: "FAIL [ 0.01s] studio-core::it order::events_apply_in_order".into(),
            }),
            ..patch("19a2f-3", PatchState::Proven, None)
        },
        patch("19a2f-5", PatchState::OutOfDate, None),
        LawPatch {
            pr: Some(412),
            url: Some("https://github.com/someone/app/pull/412".into()),
            ..patch("19a2f-6", PatchState::Opened, None)
        },
        LawPatch {
            law: "core-keeps-order".into(),
            text: Some("Events apply in the order they were made.".into()),
            guards: Some(334),
            ..patch("19a2f-7", PatchState::Proving, None)
        },
    ]
}

/// The background runs paused, with a run in each state, of each kind and with each end.
fn runs() -> Runs {
    let run = |state, kind, outcome: Option<End>| RunView {
        id: "19a2f-4d2-0".into(),
        kind,
        state,
        at: 1_761_743_990_000,
        outcome,
        line: outcome.map(|_| "Finished, but 1 tool call was denied".into()),
        denials: outcome.map_or_else(Vec::new, |_| vec!["Bash: git push".into()]),
        prompt: outcome.map(|_| "Write a test that backs the law.".into()),
    };
    let ends = [
        End::Done,
        End::Denied,
        End::Failed,
        End::TimedOut,
        End::Died,
        End::SetupFailed,
        End::NotLoggedIn,
        End::RateLimited,
    ];
    let mut list = vec![
        run(RunState::Running, Kind::BackLaw, None),
        run(RunState::Queued, Kind::MicroWorld, None),
        run(RunState::Queued, Kind::MapReview, None),
    ];
    list.extend(ends.map(|end| run(RunState::Ended, Kind::BackLaw, Some(end))));
    Runs {
        paused: Some("Claude hit its usage or rate limit".into()),
        runs: list,
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
                runs: runs(),
                catch_ups: BTreeMap::from([
                    (
                        "server".into(),
                        studio_server::part_catch_ups::Offer::CatchUp {
                            ready: true,
                            writing: false,
                            failed: false,
                            stage: None,
                        },
                    ),
                    (
                        "hook".into(),
                        studio_server::part_catch_ups::Offer::CatchUp {
                            ready: false,
                            writing: true,
                            failed: false,
                            stage: Some(studio_understanding::pr_brief_check::Stage::Fixing {
                                claims: 2,
                            }),
                        },
                    ),
                    (
                        "core".into(),
                        studio_server::part_catch_ups::Offer::Chapter {
                            pr: 334,
                            chapter: 1,
                        },
                    ),
                ]),
                stale: BTreeMap::from([("core".into(), 2)]),
                stale_why: BTreeMap::from([(
                    "core".into(),
                    "6 weeks or more: not engaged since tracking began on 2026-10-01".into(),
                )]),
                pre_flight: Some(studio_understanding::pre_flight::PreFlight {
                    issue: Some(378),
                    title: "Studio: Spec: the modules".into(),
                    parts: vec!["core".into(), "server".into()],
                    predicted: true,
                    at: 1_761_743_990_000,
                }),
                micro_worlds: worlds(),
                law_patches: law_patches(),
                ..empty(Status::Loaded)
            }),
        ));
    }
    samples.push(("UserMap", json(user())));
    samples.push(("UserMap", json(UserState::default())));
    samples.push((
        "ProjectMark",
        json(Mark {
            project: "/repo".into(),
            why: vec![MarkWhy::Fog, MarkWhy::Broken, MarkWhy::Brief],
        }),
    ));
    write(
        env!("CARGO_MANIFEST_DIR"),
        "server",
        &["MapState", "UserMap", "ProjectMark"],
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

/// A story with every field filled: an overview step and a chapter whose step carries a figure, a check, risks and cites.
#[allow(clippy::too_many_lines, reason = "one sample brief, written out whole")]
fn story() -> studio_understanding::pr_brief::Brief {
    use studio_understanding::pr_brief::*;
    let step = Step {
        k: "Now".into(),
        h: "The map's replies are sampled".into(),
        b: "Each reply has a sample `tsc` checks.".into(),
        on: vec!["server".into(), "window".into()],
        hops: vec![Hop("server-window".into(), true)],
        law: Some("wire-types-agree".into()),
        covered_by: Some("server/tests/wire.rs".into()),
        list: vec!["map_history".into()],
        risks: vec![RiskItem {
            part: "window".into(),
            trigger: "a field renamed".into(),
            check: "tsc -p scripts/wire".into(),
        }],
        fig: Some(Figure {
            kind: "before-after".into(),
            frame: Some("The replies".into()),
            items: vec![FigItem {
                label: "map_at".into(),
                tone: Some("good".into()),
                bad: Some(false),
            }],
            before: vec!["unsampled".into()],
            after: vec!["sampled".into()],
        }),
        check: Some(BriefCheck {
            kind: "pick".into(),
            q: "Which reply is sampled?".into(),
            opts: vec!["map_at".into(), "watch".into()],
            key: serde_json::json!(0),
            yes: "Right.".into(),
            no: "Not that one.".into(),
            flow: Some("look".into()),
            test: Some("the_wire_samples_of_the_replies_are_written_for_tsc".into()),
        }),
        cites: vec![Cite {
            from: "server/tests/wire.rs".into(),
            quote: "@law: wire-types-agree".into(),
        }],
        flags: vec!["not confirmed from the facts".into()],
    };
    // A step with every optional field left empty, one with an empty figure item and check, and one with neither.
    let bare = Step {
        fig: Some(Figure {
            items: vec![FigItem {
                label: "watch".into(),
                tone: None,
                bad: None,
            }],
            ..Figure::default()
        }),
        check: Some(BriefCheck::default()),
        ..Step::default()
    };
    let unset = Step {
        fig: None,
        check: None,
        ..bare.clone()
    };
    Brief {
        title: "The wire's replies".into(),
        overview: vec![step.clone()],
        chapters: vec![Chapter {
            behaviour: "The replies agree".into(),
            parts: vec!["server".into()],
            title: "Samples".into(),
            steps: vec![step, bare, unset],
            flags: vec!["one chapter".into()],
            gist: "Every reply has a sample.".into(),
            gist_flags: vec!["too long".into()],
        }],
        also: vec![Also {
            part: "core".into(),
            line: "Nothing changed in core.".into(),
            flags: Vec::new(),
        }],
        // Also touched's groups (#492): a file with every field, and one with only its path and lines.
        also_groups: vec![
            AlsoGroup {
                kind: AlsoKind::Tests,
                files: vec![TouchedFile {
                    path: "core/tests/order.rs".into(),
                    lines: 4,
                    part: Some("core".into()),
                    ignored: true,
                    laws: vec!["wire-types-agree".into()],
                }],
                lines: 4,
            },
            AlsoGroup {
                kind: AlsoKind::NoPart,
                files: vec![TouchedFile {
                    path: "loose.rs".into(),
                    lines: 2,
                    ..TouchedFile::default()
                }],
                lines: 2,
            },
        ],
        unseen: vec!["Cargo.lock".into()],
        decisions: vec![
            Decision {
                id: Some("d1".into()),
                text: "Bare JSON replies stay outside the law.".into(),
                flags: Vec::new(),
            },
            Decision {
                id: None,
                text: "A bare sentence from the writer.".into(),
                flags: vec!["no id yet".into()],
            },
        ],
        flags: vec!["a required part no chapter lights".into()],
    }
}

/// A stored brief with every field filled.
fn brief() -> studio_understanding::pr_brief::StoredBrief {
    use studio_understanding::pr_brief::*;
    StoredBrief {
        version: 3,
        pr: 334,
        head: COMMIT.into(),
        written_at: 1_761_743_990,
        stale: true,
        model: "claude-opus".into(),
        shape: Shape {
            chapters: true,
            required: vec!["server".into()],
            also: vec!["core".into()],
            overview: true,
            flows_changed: vec!["look".into()],
        },
        brief: story(),
        guard: Some(Guard {
            parts: vec!["server".into()],
        }),
        // The page reads it with the Partial facts chip (#491).
        partial_facts: PartialFacts {
            cut: Some(DiffCut {
                files_cut: vec!["ui/map.ts".into()],
                files_partial: vec!["server/src/map.rs".into()],
                bytes_sent: 65_536,
                bytes_cut: 20_480,
            }),
            claim_error: Some("the check timed out".into()),
            no_extractor: vec![".go".into()],
        },
        // Ratify joins it to an added link's row (#499).
        link_evidence: vec![LinkEvidence {
            link: "ui-server".into(),
            sites: vec![
                ImportSite {
                    file: "ui/map.ts".into(),
                    line: Some(12),
                },
                ImportSite {
                    file: "Cargo.toml".into(),
                    line: None,
                },
            ],
        }],
        // Ratify's head lists them (#641).
        map_problems: vec!["stray/s.ts: no part owns it".into()],
    }
}

// @law: wire-types-agree
#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one sample per reply, written out in one place"
)]
fn the_wire_samples_of_the_map_replies_are_written_for_tsc() {
    use studio_server::{
        back_a_law::{NewLaw, Opened as LawOpened, Started},
        ci_laws::Proof as TaggedTest,
        history::{At, Commit, Entry as HistoryEntry, File},
        map_diff::Tag as MapTag,
        micro_worlds::Explored,
        part_catch_ups::Opened as CatchUpOpened,
        pr_briefs::{BriefView, Opened as BriefOpened, StaleBase, Waiting},
        pre_flight::OpenIssue,
        projects::Project,
        ratify::{
            Detail, Evidence, HeldDetail, LinkDetail, PartDetail, Row, SiteSnippet,
            Status as Ratified, Words,
        },
        stop_rules::{Facts, Kind as StopKind},
    };
    use studio_understanding::part_catch_up::{FogEntry, StoredCatchUp, Summary};
    let entry = |fogged, brief| HistoryEntry {
        commit: COMMIT.into(),
        short: "0123456".into(),
        label: "#334 PR briefs".into(),
        fogged,
        brief,
    };
    let commit = |url: Option<&str>| Commit {
        commit: COMMIT.into(),
        short: "0123456".into(),
        label: "#334 PR briefs".into(),
        url: url.map(String::from),
        files: vec![File {
            path: "server/src/map.rs".into(),
            diff: "@@ -1 +1 @@\n-a\n+b\n".into(),
        }],
    };
    let proof = |test: Option<&str>| TaggedTest {
        file: "server/tests/wire.rs".into(),
        line: 12,
        test: test.map(String::from),
        first: 12,
        snippet: "// @law: wire-types-agree\n#[test]\nfn f() {}".into(),
    };
    let row = |status, change: &str| Row {
        pr: 334,
        kind: "law".into(),
        id: "wire-types-agree".into(),
        text: "The window's types and the Rust wire types agree.".into(),
        change: change.into(),
        status,
        detail: None,
        evidence: None,
        mark: Some("law:wire-types-agree".into()),
    };
    let words = |before: Option<&str>, after: &str| Words {
        before: before.map(String::from),
        after: Some(after.into()),
    };
    let part_row = Row {
        kind: "part".into(),
        id: "server".into(),
        text: "Server".into(),
        detail: Some(Detail::Part(PartDetail {
            name: words(Some("Server"), "Server"),
            purpose: words(
                Some("Serves the page."),
                "Serves the page and keeps its state.",
            ),
            caption: words(Some("Serves the page"), "Serves the page"),
            holds: vec![HeldDetail {
                name: "Fog".into(),
                text: words(None, "Greys a part a PR changed."),
                paths_added: vec!["server/src/fog.rs".into()],
                paths_removed: vec![],
            }],
            paths_added: vec!["server/src/".into()],
            paths_removed: vec!["src-tauri/".into()],
        })),
        ..row(Ratified::NotYet, "changed")
    };
    let link_row = Row {
        kind: "link".into(),
        id: "ui-server".into(),
        text: "ui to server".into(),
        detail: Some(Detail::Link(LinkDetail {
            kind: words(Some("import"), "http"),
            reason: words(Some(""), "The page calls the server over HTTP."),
            direction: Some(words(Some("server to ui"), "ui to server")),
        })),
        ..row(Ratified::NotYet, "changed")
    };
    let view = |expected| BriefView {
        brief: brief(),
        partial: true,
        expected,
        stale_base: None,
        since: None,
    };
    let kinds = [
        StopKind::Session,
        StopKind::MapOpen,
        StopKind::MapAction,
        StopKind::Scrub,
        StopKind::Proof,
        StopKind::Know,
        StopKind::Dismissed,
        StopKind::BriefOffered,
        StopKind::BriefOpened,
        StopKind::ChapterDone,
        StopKind::CheckAnswered,
        StopKind::Ratify,
        StopKind::UnconfirmedSeen,
        StopKind::CatchUpOffered,
        StopKind::CatchUpOpened,
        StopKind::CatchUpDone,
        StopKind::ExplainBackGiven,
        StopKind::ExplainBackMarked,
        StopKind::RefresherEntered,
        StopKind::StaleEntered,
        StopKind::PreFlightRun,
        StopKind::MicroWorldOpened,
        StopKind::BackLawStarted,
        StopKind::GuardOffered,
        StopKind::GuardStarted,
    ];
    let catch_up = |summary| StoredCatchUp {
        version: 1,
        part: "server".into(),
        fog: vec![COMMIT.into()],
        written_at: 1_761_743_990,
        model: "claude-opus".into(),
        partial: true,
        brief: story(),
        summary,
    };
    let fog_entry = |pr: Option<u64>| FogEntry {
        commit: COMMIT.into(),
        pr,
        title: "PR briefs".into(),
        body: String::new(),
    };
    let samples: Vec<Sample> = vec![
        ("HistoryEntry", json(entry(true, Some(334)))),
        ("HistoryEntry", json(entry(false, None))),
        (
            "HistoricalMap",
            json(At {
                commit: COMMIT.into(),
                short: "0123456".into(),
                label: "#334 PR briefs".into(),
                source: "{ \"parts\": [] }".into(),
            }),
        ),
        (
            "CommitFiles",
            json(commit(Some("https://github.com/o/r/commit/0123456"))),
        ),
        ("CommitFiles", json(commit(None))),
        (
            "PrDiffArgs",
            json(studio_server::map::PrDiffArgs {
                project: "/repo".into(),
                number: 334,
                head: COMMIT.into(),
                title: "PR briefs".into(),
                parts: Some(vec!["server".into()]),
                paths: None,
                base: None,
            }),
        ),
        (
            "PrDiffArgs",
            json(studio_server::map::PrDiffArgs {
                project: "/repo".into(),
                number: 334,
                head: COMMIT.into(),
                title: "PR briefs".into(),
                parts: None,
                paths: None,
                base: None,
            }),
        ),
        (
            "PrDiffArgs",
            json(studio_server::map::PrDiffArgs {
                project: "/repo".into(),
                number: 334,
                head: COMMIT.into(),
                title: "PR briefs".into(),
                parts: None,
                paths: Some(vec!["docs/a.md".into()]),
                base: None,
            }),
        ),
        (
            "PrDiffArgs",
            json(studio_server::map::PrDiffArgs {
                project: "/repo".into(),
                number: 334,
                head: COMMIT.into(),
                title: "PR briefs".into(),
                parts: Some(vec!["server".into()]),
                paths: None,
                base: Some(COMMIT.into()),
            }),
        ),
        ("Proof", json(proof(Some("the_wire_samples")))),
        ("Proof", json(proof(None))),
        (
            "BriefView",
            json(BriefView {
                // The stale-base line (#494): what main changed since the merge base.
                stale_base: Some(StaleBase {
                    laws: vec!["wire-types-agree".into()],
                    links: vec!["server-window".into()],
                    flows: vec!["turn-writing".into()],
                    paths: vec!["server".into()],
                    files: vec!["Cargo.toml".into(), "server/src/map.rs".into()],
                }),
                ..view(Some(studio_understanding::pre_flight::Expected {
                    expected: vec!["server".into()],
                    touched: vec!["server".into(), "window".into()],
                }))
            }),
        ),
        (
            "BriefView",
            json(BriefView {
                brief: studio_understanding::pr_brief::StoredBrief {
                    guard: None,
                    partial_facts: Default::default(),
                    link_evidence: Vec::new(),
                    ..brief()
                },
                ..view(None)
            }),
        ),
        (
            "BriefView",
            json(BriefView {
                // A "Since you read" story (#497): the head it runs from.
                since: Some(COMMIT.into()),
                ..view(None)
            }),
        ),
        (
            "WaitingBriefs",
            json(Waiting {
                project: "/repo".into(),
                numbers: vec![334, 336],
            }),
        ),
        ("RatifyRow", json(row(Ratified::Ratified, "added"))),
        ("RatifyRow", json(row(Ratified::Accepted, "changed"))),
        ("RatifyRow", json(row(Ratified::Overturned, "removed"))),
        ("RatifyRow", json(row(Ratified::NotYet, ""))),
        ("RatifyRow", json(part_row)),
        (
            "MapDiff",
            // A PR's Map diff (#640): an added part, a removed line, an added law.
            json(studio_server::map_diff::MapDiff {
                pr: 334,
                base: COMMIT.into(),
                head: COMMIT.into(),
                source: "{ \"parts\": [] }".into(),
                parts: [
                    ("install".to_string(), MapTag::Added),
                    ("server".to_string(), MapTag::Edited),
                ]
                .into(),
                actors: Default::default(),
                lines: vec![studio_server::map_diff::LineTag {
                    ends: ["plugins".into(), "server".into()],
                    change: MapTag::Removed,
                }],
                laws: [("wire-types-agree".to_string(), MapTag::Added)].into(),
            }),
        ),
        ("RatifyRow", json(link_row)),
        (
            "RatifyRow",
            // An added link with the import sites its brief stored (#499).
            json(Row {
                kind: "link".into(),
                id: "ui-server".into(),
                text: "ui to server".into(),
                evidence: Some(Evidence {
                    head: COMMIT.into(),
                    sites: vec![
                        studio_understanding::pr_brief::ImportSite {
                            file: "ui/map.ts".into(),
                            line: Some(12),
                        },
                        studio_understanding::pr_brief::ImportSite {
                            file: "Cargo.toml".into(),
                            line: None,
                        },
                    ],
                }),
                ..row(Ratified::NotYet, "added")
            }),
        ),
        (
            "SiteSnippet",
            json(SiteSnippet {
                file: "ui/map.ts".into(),
                line: Some(12),
                first: 8,
                lines: vec!["import { x } from \"./y.js\";".into()],
            }),
        ),
        (
            "SiteSnippet",
            json(SiteSnippet {
                file: "Cargo.toml".into(),
                line: None,
                first: 1,
                lines: vec!["[package]".into()],
            }),
        ),
        (
            "StopFacts",
            json(Facts {
                active_days: 5,
                merged_prs: 4,
                engaged_days: 3,
                scrubs: 2,
                proofs: 1,
                knows: 1,
                dismissed: vec!["history".into()],
                briefs_offered: 4,
                briefs_opened: 2,
                chapters_done: 3,
                counts: kinds.into_iter().map(|k| (k, 1)).collect(),
                began: Some("2026-11-03".into()),
                stale_since: Some(1_761_743_990),
            }),
        ),
        ("StopFacts", json(Facts::default())),
        (
            "BriefOpened",
            json(BriefOpened::Brief(Box::new(view(None)))),
        ),
        ("BriefOpened", json(BriefOpened::Writing { writing: true })),
        (
            "CatchUpOpened",
            json(CatchUpOpened::CatchUp(Box::new(catch_up(Some(Summary {
                line: "This catch-up summarises the main shifts of 4 fog entries".into(),
                entries: vec![fog_entry(Some(334)), fog_entry(None)],
            }))))),
        ),
        (
            "CatchUpOpened",
            json(CatchUpOpened::CatchUp(Box::new(catch_up(None)))),
        ),
        (
            "CatchUpOpened",
            json(CatchUpOpened::Writing { writing: true }),
        ),
        (
            "OpenIssue",
            json(OpenIssue {
                number: 374,
                title: "Studio: the law wire-types-agree".into(),
            }),
        ),
        (
            "PreFlight",
            json(studio_understanding::pre_flight::PreFlight {
                issue: None,
                title: "Parts picked by hand".into(),
                parts: vec!["server".into()],
                predicted: false,
                at: 1_761_743_990_000,
            }),
        ),
        (
            "Explored",
            json(Explored {
                started: false,
                line: Some("A run is already going".into()),
                prompt: Some("In the repository at /repo, build a micro-world".into()),
            }),
        ),
        (
            "Explored",
            json(Explored {
                started: true,
                line: None,
                prompt: None,
            }),
        ),
        (
            "SavedProject",
            json(Project {
                path: "/repo".into(),
                gone: true,
            }),
        ),
        (
            "BackLawStarted",
            json(Started::Run {
                id: "19a2f-1".into(),
            }),
        ),
        (
            "BackLawStarted",
            json(Started::Refused {
                line: "Claude hit its usage or rate limit".into(),
                prompt: "Write a test that backs the law.".into(),
            }),
        ),
        (
            "LawPatchOpened",
            json(LawOpened::Pr {
                number: 412,
                url: "https://github.com/someone/app/pull/412".into(),
            }),
        ),
        ("LawPatchOpened", json(LawOpened::OutOfDate)),
        (
            "NewLaw",
            json(NewLaw {
                id: "core-keeps-order".into(),
                on: "core".into(),
                text: "Events apply in the order they were made.".into(),
            }),
        ),
        ("About", json(studio_server::about())),
    ];
    // The page's version is the package's, which the release bumps with npm's (#512).
    assert_eq!(
        json(studio_server::about())["version"],
        env!("CARGO_PKG_VERSION")
    );
    write(
        env!("CARGO_MANIFEST_DIR"),
        "server-replies",
        &[
            "About",
            "HistoryEntry",
            "HistoricalMap",
            "CommitFiles",
            "PrDiffArgs",
            "Proof",
            "BriefView",
            "WaitingBriefs",
            "RatifyRow",
            "SiteSnippet",
            "StopFacts",
            "BriefOpened",
            "CatchUpOpened",
            "OpenIssue",
            "PreFlight",
            "Explored",
            "SavedProject",
            "BackLawStarted",
            "LawPatchOpened",
            "NewLaw",
        ],
        &samples,
    );
}

/// `name` as the page writes it: `agent_id` is `agentId`. `api::call` turns it back.
fn camel(name: &str) -> String {
    let mut out = String::new();
    let mut upper = false;
    for c in name.chars() {
        if c == '_' {
            upper = true;
        } else if upper {
            out.push(c.to_ascii_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// Each command `api::call` answers and the argument names its arm reads, from the source: an arm is a line opening with the command's
/// name at the arms' indent, and an argument is each `(a, "<name>")` in it. A command whose arguments are one wire type (`whole(a)`) has that type's field
/// names, from a value with every field filled.
fn command_arguments() -> Vec<(String, Vec<String>)> {
    let api = include_str!("../src/api.rs");
    let body = &api[api
        .find("match name {")
        .expect("api::call matches on the name")..];
    let whole: Vec<String> = match json(studio_server::map::PrDiffArgs {
        project: String::new(),
        number: 0,
        head: String::new(),
        title: String::new(),
        parts: Some(Vec::new()),
        paths: Some(Vec::new()),
        base: Some(String::new()),
    }) {
        serde_json::Value::Object(fields) => fields.keys().cloned().collect(),
        _ => unreachable!("a struct serializes as an object"),
    };
    let mut commands: Vec<(String, Vec<String>)> = Vec::new();
    // The arms sit at one indent; a `match` inside an arm, such as `log`'s on the level, is deeper.
    let mut indent = None;
    for line in body.lines() {
        let trimmed = line.trim_start();
        let depth = line.len() - trimmed.len();
        if let Some(rest) = trimmed.strip_prefix('"')
            && let Some((name, after)) = rest.split_once('"')
            && after.trim_start().starts_with("=>")
            && *indent.get_or_insert(depth) == depth
        {
            commands.push((name.to_string(), Vec::new()));
        }
        let Some((command, names)) = commands.last_mut() else {
            continue;
        };
        if line.contains("whole(a)") {
            assert_eq!(
                command, "pr_diff",
                "a new command reads its arguments whole: give its type here"
            );
            names.extend(whole.iter().map(|name| camel(name)));
        }
        let mut rest = line;
        while let Some(at) = rest.find("(a, \"") {
            rest = &rest[at + 5..];
            let end = rest
                .find('"')
                .expect("an argument's name is a string literal");
            names.push(camel(&rest[..end]));
            rest = &rest[end..];
        }
    }
    for (_, names) in &mut commands {
        names.sort();
        names.dedup();
    }
    commands
}

// @law: wire-types-agree
/// The page's `Commands` (`ui/types.ts`) against the server's handlers (#554): `scripts/wire/server-args.ts` lists each command's
/// argument names as `api::call` reads them, and `tsc -p scripts/wire` fails when the page has a command the server lacks, lacks one it
/// has, or names a command's arguments otherwise.
#[test]
fn the_command_arguments_are_written_for_tsc() {
    use std::fmt::Write;
    let commands = command_arguments();
    assert!(
        commands
            .iter()
            .any(|(c, names)| c == "subagent_log" && names == &["agentId", "offset", "session"]),
        "the reading found the arms: {commands:?}"
    );
    let mut ts = String::from(
        "// Written by `server/tests/wire.rs` (#554) from `server/src/api.rs`; `tsc -p scripts/wire` checks it.\n\
         import type { Args, Command } from \"../../ui/types.ts\";\n\n\
         // Each command the server answers, with the names of the arguments its handler reads.\ntype Server = {\n",
    );
    for (command, names) in &commands {
        let names = if names.is_empty() {
            "never".to_string()
        } else {
            names
                .iter()
                .map(|n| format!("\"{n}\""))
                .collect::<Vec<_>>()
                .join(" | ")
        };
        writeln!(ts, "  {command}: {names};").expect("a String takes a write");
    }
    ts.push_str(
        "};\n\n\
         // The commands the transport posts itself, not through `invoke`.\n\
         type Own = \"log\" | \"page_focus\";\n\
         type Same<A, B> = [A] extends [B] ? ([B] extends [A] ? true : false) : false;\n\
         // A command with no arguments takes `Record<string, never>`, whose keys are any string.\n\
         type Names<T> = string extends keyof T ? never : keyof T;\n\
         export const commands: Same<Command, Exclude<keyof Server, Own>> = true;\n\
         export const names: { [C in Command]: C extends keyof Server ? Same<Names<Args<C>>, Server[C]> : false } = {\n",
    );
    for (command, _) in &commands {
        if command != "log" && command != "page_focus" {
            writeln!(ts, "  {command}: true,").expect("a String takes a write");
        }
    }
    ts.push_str("};\n");
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/wire/server-args.ts");
    std::fs::write(&path, ts).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
}
