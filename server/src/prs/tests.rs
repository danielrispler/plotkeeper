use super::*;

#[test]
fn an_author_gh_cannot_tell_is_not_daniels_and_his_own_login_is_not_asked() {
    let dir = tempfile::tempdir().unwrap();
    let (gh, calls) = (dir.path().join("gh"), dir.path().join("calls"));
    std::fs::write(
        &gh,
        format!("#!/bin/sh\necho \"$@\" >> '{}'\ncase \"$1 $2\" in\n  'api user') echo daniel ;;\n  *) exit 1 ;;\nesac\n", calls.display()),
    )
    .unwrap();
    std::fs::set_permissions(&gh, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    assert!(!by_own_login_with(&gh.to_string_lossy(), dir.path(), 7));
    let asked = std::fs::read_to_string(&calls).unwrap();
    assert!(asked.contains("pr view 7"), "{asked}");
    assert!(!asked.contains("api user"), "{asked}");
}

const OPEN: &str = r#"{"number":7,"title":"Fog on the map","isDraft":false,"state":"OPEN","headRefOid":"aaa111","body":"Closes #5","closingIssuesReferences":[{"number":5}]}"#;
const PUSHED: &str = r#"{"number":7,"title":"Fog on the map","isDraft":false,"state":"OPEN","headRefOid":"bbb222","body":"Closes #5","closingIssuesReferences":[{"number":5}]}"#;
const DRAFT: &str = r#"{"number":8,"title":"Wip","isDraft":true,"state":"OPEN","headRefOid":"ccc333","body":"","closingIssuesReferences":[]}"#;
const CLOSED: &str = r#"{"number":9,"title":"Gone","isDraft":false,"state":"CLOSED","headRefOid":"ddd444","body":"","closingIssuesReferences":[]}"#;

fn recorded(branch: &str) -> Looked {
    match branch {
        "fog" => Looked::Found(OPEN.into()),
        "wip" => Looked::Found(DRAFT.into()),
        "gone" => Looked::Found(CLOSED.into()),
        _ => Looked::NoPr,
    }
}
fn no_list() -> Option<String> {
    None
}
fn branches() -> Vec<String> {
    ["fog", "wip", "gone", "none"].map(String::from).to_vec()
}

#[test]
fn only_an_open_pr_that_is_not_a_draft_is_ready() {
    let prs = Prs::default();
    let found = prs.ask(
        "/p",
        Trigger::TurnEnd,
        Instant::now(),
        &branches(),
        recorded,
        no_list,
    );
    // Every PR found still names its session; a branch with none names nothing.
    assert_eq!(
        found.unwrap(),
        vec![
            ("fog".into(), Some(7)),
            ("wip".into(), Some(8)),
            ("gone".into(), Some(9)),
            ("none".into(), None)
        ]
    );
    let ready = prs.ready("/p");
    assert_eq!(ready.len(), 1);
    assert_eq!(
        (ready[0].number, ready[0].title.as_str()),
        (7, "Fog on the map")
    );
}

#[test]
fn a_new_head_replaces_the_stored_one() {
    let prs = Prs::default();
    let start = Instant::now();
    prs.ask(
        "/p",
        Trigger::TurnEnd,
        start,
        &["fog".into()],
        recorded,
        no_list,
    );
    assert_eq!(prs.ready("/p")[0].head, "aaa111");
    let pushed = |_: &str| Looked::Found(PUSHED.to_string());
    prs.ask(
        "/p",
        Trigger::TurnEnd,
        start,
        &["fog".into()],
        pushed,
        no_list,
    );
    assert_eq!(prs.ready("/p")[0].head, "bbb222");
}

#[test]
fn a_pr_that_is_no_longer_ready_leaves_the_tray() {
    let prs = Prs::default();
    let start = Instant::now();
    prs.ask(
        "/p",
        Trigger::TurnEnd,
        start,
        &["fog".into()],
        recorded,
        no_list,
    );
    prs.ask(
        "/p",
        Trigger::TurnEnd,
        start,
        &["fog".into()],
        |_| Looked::Found(DRAFT.into()),
        no_list,
    );
    assert!(prs.ready("/p").is_empty());
}

#[test]
fn a_turns_end_asks_at_once_but_focus_and_map_open_share_one_cooldown() {
    let prs = Prs::default();
    let start = Instant::now();
    let asks = std::cell::Cell::new(0);
    let view = |b: &str| {
        asks.set(asks.get() + 1);
        recorded(b)
    };
    let one = ["fog".to_string()];
    assert!(
        prs.ask("/p", Trigger::MapOpen, start, &one, view, no_list)
            .is_some()
    );
    // Focus within the cooldown asks nothing.
    let soon = start + Duration::from_secs(30);
    assert!(
        prs.ask("/p", Trigger::Focus, soon, &one, view, no_list)
            .is_none()
    );
    assert!(
        prs.ask("/p", Trigger::MapOpen, soon, &one, view, no_list)
            .is_none()
    );
    assert_eq!(asks.get(), 1);
    // A turn's end asks at once, whatever the cooldown.
    assert!(
        prs.ask("/p", Trigger::TurnEnd, soon, &one, view, no_list)
            .is_some()
    );
    assert_eq!(asks.get(), 2);
    // Another repository has its own cooldown; after a minute focus asks again.
    assert!(
        prs.ask("/q", Trigger::Focus, soon, &one, view, no_list)
            .is_some()
    );
    let later = soon + crate::map::COOLDOWN;
    assert!(
        prs.ask("/p", Trigger::Focus, later, &one, view, no_list)
            .is_some()
    );
    assert_eq!(asks.get(), 4);
}

#[test]
fn a_lookup_in_flight_blocks_every_trigger() {
    let prs = Prs::default();
    let start = Instant::now();
    let inner = std::cell::Cell::new(None);
    prs.ask(
        "/p",
        Trigger::TurnEnd,
        start,
        &["fog".into()],
        |b| {
            inner.set(Some(prs.ask(
                "/p",
                Trigger::TurnEnd,
                start,
                &["fog".into()],
                recorded,
                no_list,
            )));
            recorded(b)
        },
        no_list,
    );
    assert_eq!(inner.take(), Some(None));
}

fn list_of(prs: &[&str]) -> impl Fn() -> Option<String> {
    let json = format!("[{}]", prs.join(","));
    move || Some(json.clone())
}
const HAND_MADE: &str = r#"{"number":21,"title":"By hand","isDraft":false,"state":"OPEN","headRefOid":"eee555","headRefName":"by-hand","body":"","closingIssuesReferences":[]}"#;

#[test]
fn a_pr_no_session_has_a_branch_for_is_ready_from_the_list_of_open_prs() {
    let prs = Prs::default();
    let start = Instant::now();
    let found = prs.ask(
        "/p",
        Trigger::TurnEnd,
        start,
        &[],
        recorded,
        list_of(&[HAND_MADE]),
    );
    assert_eq!(found, Some(vec![]));
    let ready = prs.ready("/p");
    assert_eq!(ready.len(), 1);
    assert_eq!((ready[0].number, ready[0].head.as_str()), (21, "eee555"));
}

#[test]
fn a_pr_no_session_has_a_branch_for_takes_its_new_head_from_the_next_list() {
    let prs = Prs::default();
    let start = Instant::now();
    prs.ask(
        "/p",
        Trigger::TurnEnd,
        start,
        &[],
        recorded,
        list_of(&[HAND_MADE]),
    );
    let pushed = HAND_MADE.replace("eee555", "fff666");
    prs.ask(
        "/p",
        Trigger::TurnEnd,
        start,
        &[],
        recorded,
        list_of(&[&pushed]),
    );
    assert_eq!(prs.ready("/p")[0].head, "fff666");
}

#[test]
fn a_pr_that_left_the_list_and_has_no_session_leaves_the_tray() {
    let prs = Prs::default();
    let start = Instant::now();
    prs.ask(
        "/p",
        Trigger::TurnEnd,
        start,
        &[],
        recorded,
        list_of(&[HAND_MADE]),
    );
    // Merged since: the list no longer holds it, and no session asks about its branch.
    prs.ask("/p", Trigger::TurnEnd, start, &[], recorded, list_of(&[]));
    assert!(prs.ready("/p").is_empty());
}

#[test]
fn a_failed_lookup_keeps_what_was_known() {
    let prs = Prs::default();
    let start = Instant::now();
    prs.ask(
        "/p",
        Trigger::TurnEnd,
        start,
        &["fog".into()],
        recorded,
        list_of(&[HAND_MADE]),
    );
    assert_eq!(prs.ready("/p").len(), 2);
    // `gh` fails for the branch and for the list: both PRs stay, and the session's name is not touched.
    let found = prs.ask(
        "/p",
        Trigger::TurnEnd,
        start,
        &["fog".into()],
        |_| Looked::Failed,
        no_list,
    );
    assert_eq!(found, Some(vec![]));
    assert_eq!(prs.ready("/p").len(), 2);
    assert!(prs.listed("/p"), "the list answered once");
    assert!(!prs.listed("/q"));
    // `gh` says the branch has no PR any more: it goes.
    let found = prs.ask(
        "/p",
        Trigger::TurnEnd,
        start,
        &["fog".into()],
        |_| Looked::NoPr,
        no_list,
    );
    assert_eq!(found, Some(vec![("fog".into(), None)]));
    assert_eq!(prs.ready("/p").len(), 1);
}

#[test]
fn a_session_branch_that_is_no_longer_asked_about_is_forgotten() {
    let prs = Prs::default();
    let start = Instant::now();
    prs.ask(
        "/p",
        Trigger::TurnEnd,
        start,
        &["fog".into()],
        recorded,
        list_of(&[]),
    );
    assert_eq!(prs.ready("/p").len(), 1);
    // The session closed: its branch is not asked about, and the list (which answered) does not hold the PR.
    prs.ask("/p", Trigger::TurnEnd, start, &[], recorded, list_of(&[]));
    assert!(prs.ready("/p").is_empty());
}

#[test]
fn output_that_is_not_a_pr_is_none() {
    assert_eq!(parse("no pull requests found"), None);
}

#[test]
fn a_branch_is_asked_about_in_its_project_s_folder_where_its_session_went() {
    let query = |project: &str, cwd: &str, branch: &str| studio_core::BranchQuery {
        project: project.into(),
        cwd: cwd.into(),
        branch: branch.into(),
    };
    let asked = lookups(
        vec![
            // A session that `cd`ed out of its repository, and one in a task's worktree.
            query("/r/p", "/tmp/elsewhere", "fix-hook"),
            query("/r/p", "/r/p/.claude/worktrees/a", "issue-7"),
            query("", "/tmp/x", "loose"),
        ],
        Some("/r/q".into()),
    );
    assert_eq!(
        asked,
        BTreeMap::from([
            (
                "/r/p".to_string(),
                vec![
                    ("/r/p".to_string(), "fix-hook".to_string()),
                    ("/r/p".to_string(), "issue-7".to_string()),
                ]
            ),
            ("/r/q".to_string(), vec![]),
        ])
    );
}

/// A `gh` that prints `author` for `pr view` and `me` for `api user`, and fails when either is empty.
fn fake_gh(dir: &Path, author: &str, me: &str) -> String {
    let path = dir.join("gh");
    let script = format!(
        "#!/bin/sh\ncase \"$1 $2\" in\n  \"pr view\") out='{author}' ;;\n  \"api user\") out='{me}' ;;\nesac\n\
         [ -n \"$out\" ] || exit 1\necho \"$out\"\n"
    );
    std::fs::write(&path, script).unwrap();
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    std::fs::set_permissions(&path, perms).unwrap();
    path.display().to_string()
}

#[test]
fn a_pr_is_by_daniel_s_own_login_only_when_gh_says_both_logins_match() {
    let dir = tempfile::tempdir().unwrap();
    let at = |author, me| by_own_login_with(&fake_gh(dir.path(), author, me), dir.path(), 7);
    assert!(at("danielrispler", "danielrispler"));
    assert!(!at("someone-else", "danielrispler"));
    // `gh` cannot say who wrote it, or who is signed in: not trusted.
    assert!(!at("", "danielrispler"));
    assert!(!at("danielrispler", ""));
}
