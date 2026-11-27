use super::*;
use serde::Deserialize;

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
struct Kept {
    project: String,
    names: Vec<String>,
}

fn kept(project: &str, names: &[&str]) -> Kept {
    Kept {
        project: project.into(),
        names: names.iter().map(ToString::to_string).collect(),
    }
}

const PROJECT: &str = "/Users/me/my proj.v2";

#[test]
fn a_record_is_kept_where_and_as_the_builds_before_the_store_kept_it() {
    let dir = tempfile::tempdir().unwrap();
    let records = Records::<Kept>::new(dir.path().join("store"));
    // The folder and file the builds before this store wrote: the path with each character not a letter or digit as `_`,
    // its FNV-1a hash in hex, and the record's name.
    let file = dir
        .path()
        .join("store/_Users_me_my_proj_v2-09985c8998b2dec3/7.json");
    records
        .write(PROJECT, "7", &kept(PROJECT, &["a", "b"]))
        .unwrap();
    assert_eq!(
        fs::read_to_string(&file).ok().as_deref(),
        Some(r#"{"project":"/Users/me/my proj.v2","names":["a","b"]}"#)
    );
    assert_eq!(records.read(PROJECT, "7"), Some(kept(PROJECT, &["a", "b"])));
    // A file a build before the store wrote reads back, and one that is not an `R` reads as none.
    fs::write(
        &file,
        r#"{"project": "/Users/me/my proj.v2", "names": ["c"], "later": 1}"#,
    )
    .unwrap();
    assert_eq!(records.read(PROJECT, "7"), Some(kept(PROJECT, &["c"])));
    fs::write(&file, "not json").unwrap();
    assert_eq!(records.read(PROJECT, "7"), None);
    assert_eq!(records.read(PROJECT, "8"), None);
}

#[test]
fn records_list_per_project_and_every_projects_together() {
    let dir = tempfile::tempdir().unwrap();
    let records = Records::<Kept>::new(dir.path().join("store"));
    let other = "/Users/me/my_proj_v2";
    records
        .write(PROJECT, "1", &kept(PROJECT, &["one"]))
        .unwrap();
    records
        .write(PROJECT, "2", &kept(PROJECT, &["two"]))
        .unwrap();
    records.write(other, "1", &kept(other, &["three"])).unwrap();
    // A page beside the records, as micro-worlds keep, and a file that is not an `R`, are not records.
    fs::write(records.folder(PROJECT).join("1.html"), "<p>").unwrap();
    fs::write(records.file(PROJECT, "3"), "[]").unwrap();
    let names = |mut list: Vec<Kept>| {
        list.sort_by(|a, b| a.names.cmp(&b.names));
        list.into_iter()
            .flat_map(|kept| kept.names)
            .collect::<Vec<_>>()
    };
    assert_eq!(names(records.records(PROJECT)), ["one", "two"]);
    assert_eq!(names(records.records(other)), ["three"]);
    assert_eq!(names(records.records("/elsewhere")), Vec::<String>::new());
    assert_eq!(names(records.every()), ["one", "three", "two"]);
}

#[test]
fn a_change_told_after_a_write_reaches_the_first_listener_once() {
    use std::sync::{Arc, Mutex};
    let dir = tempfile::tempdir().unwrap();
    let records = Records::<Kept>::new(dir.path().join("store"));
    // Told before a listener is set: no one hears it.
    records.tell(PROJECT);
    let (first, second) = (
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(Mutex::new(Vec::new())),
    );
    let heard = first.clone();
    records.on_change(move |project| heard.lock().unwrap().push(project.to_string()));
    let heard = second.clone();
    records.on_change(move |project| heard.lock().unwrap().push(project.to_string()));
    for names in [&["a"][..], &["a", "b"]] {
        records.write(PROJECT, "1", &kept(PROJECT, names)).unwrap();
        records.tell(PROJECT);
    }
    assert_eq!(*first.lock().unwrap(), [PROJECT, PROJECT]);
    assert_eq!(*second.lock().unwrap(), Vec::<String>::new());
}

#[test]
fn writes_at_once_under_the_lock_all_land() {
    use std::sync::Barrier;
    let dir = tempfile::tempdir().unwrap();
    let records = Records::<Kept>::new(dir.path().join("store"));
    let names: Vec<String> = (0..8).map(|n| format!("writer {n}")).collect();
    let start = Barrier::new(names.len());
    std::thread::scope(|scope| {
        for name in &names {
            let (records, start) = (&records, &start);
            scope.spawn(move || {
                start.wait();
                let _guard = records.lock();
                let mut now = records.read(PROJECT, "1").unwrap_or_default();
                // Long enough for every other writer to read the same record, were the lock not held.
                std::thread::sleep(std::time::Duration::from_millis(5));
                now.names.push(name.clone());
                records.write(PROJECT, "1", &now).unwrap();
            });
        }
    });
    let mut landed = records.read(PROJECT, "1").unwrap_or_default().names;
    landed.sort();
    assert_eq!(landed, names);
    // While the lock is held, a caller that would rather not wait gets none.
    let held = records.lock();
    assert!(records.try_lock().is_none());
    drop(held);
    assert!(records.try_lock().is_some());
}

#[test]
fn a_runs_end_is_claimed_once_however_many_ask_at_once() {
    use std::sync::Barrier;
    let dir = tempfile::tempdir().unwrap();
    let records = Records::<Kept>::new(dir.path().join("store"));
    let start = Barrier::new(8);
    let won = std::thread::scope(|scope| {
        let claims: Vec<_> = (0..8)
            .map(|_| {
                let (records, start) = (&records, &start);
                scope.spawn(move || {
                    start.wait();
                    records.claim("run-1")
                })
            })
            .collect();
        let won: Vec<bool> = claims
            .into_iter()
            .map(|claim| claim.join().unwrap())
            .collect();
        won.into_iter().filter(|won| *won).count()
    });
    assert_eq!(won, 1);
    assert!(records.claimed("run-1"));
    assert!(!records.claimed("run-2"));
    assert!(records.claim("run-2"));
    // Once read, the end can be claimed again.
    records.release("run-1");
    assert!(!records.claimed("run-1"));
    assert!(records.claim("run-1"));
}
