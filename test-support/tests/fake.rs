//! The fake program: the test binary started under another name.

use std::{
    io::Write,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use studio_test_support::{Behaviour, calls, fake};

fn said(out: &str, err: &str, code: i32) -> Behaviour {
    Behaviour {
        stdout: out.into(),
        stderr: err.into(),
        code,
        ..Behaviour::default()
    }
}

#[test]
fn a_fake_prints_what_it_was_told_and_exits_with_its_code() {
    let dir = tempfile::tempdir().unwrap();
    let claude = fake(dir.path(), "claude", &said("out\n", "err\n", 3));

    let ran = Command::new(&claude).output().unwrap();

    assert_eq!(String::from_utf8_lossy(&ran.stdout), "out\n");
    assert_eq!(String::from_utf8_lossy(&ran.stderr), "err\n");
    assert_eq!(ran.status.code(), Some(3));
}

#[test]
fn a_fake_records_arguments_folder_stdin_and_environment_one_entry_per_call() {
    let dir = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let agy = fake(
        dir.path(),
        "agy",
        &Behaviour {
            record: true,
            ..Behaviour::default()
        },
    );

    let mut child = Command::new(&agy)
        .args(["status", "--short"])
        .current_dir(work.path())
        .env("FAKE_MARK", "yes")
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"the body\n")
        .unwrap();
    assert!(child.wait().unwrap().success());
    Command::new(&agy).arg("again").status().unwrap();

    let calls = calls(&agy);
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].args, ["status", "--short"]);
    assert_eq!(
        calls[0].cwd,
        work.path().canonicalize().unwrap().display().to_string()
    );
    assert_eq!(calls[0].stdin, "the body\n");
    assert_eq!(
        calls[0].env.get("FAKE_MARK").map(String::as_str),
        Some("yes")
    );
    assert_eq!(calls[1].args, ["again"]);
}

#[test]
fn a_fake_waits_when_told() {
    let dir = tempfile::tempdir().unwrap();
    let slow = fake(
        dir.path(),
        "slow",
        &Behaviour {
            wait: Duration::from_millis(600),
            ..Behaviour::default()
        },
    );

    let started = Instant::now();
    Command::new(&slow).status().unwrap();

    assert!(started.elapsed() >= Duration::from_millis(600));
}

#[test]
fn a_rewrite_changes_what_the_next_call_does() {
    let dir = tempfile::tempdir().unwrap();
    let first = fake(
        dir.path(),
        "git",
        &Behaviour {
            record: true,
            ..said("", "no\n", 1)
        },
    );
    assert_eq!(
        Command::new(&first).arg("one").status().unwrap().code(),
        Some(1)
    );

    let second = fake(
        dir.path(),
        "git",
        &Behaviour {
            record: true,
            ..said("yes\n", "", 0)
        },
    );

    assert_eq!(first, second);
    let ran = Command::new(&second).arg("two").output().unwrap();
    assert_eq!(String::from_utf8_lossy(&ran.stdout), "yes\n");
    assert!(ran.status.success());
    let args: Vec<_> = calls(&second).into_iter().map(|c| c.args).collect();
    assert_eq!(args, [vec!["one"], vec!["two"]]);
}

#[test]
fn concurrent_calls_each_leave_one_whole_entry() {
    let dir = tempfile::tempdir().unwrap();
    let agy = fake(
        dir.path(),
        "agy",
        &Behaviour {
            record: true,
            ..Behaviour::default()
        },
    );

    let children: Vec<_> = (0..40)
        .map(|i| Command::new(&agy).arg(i.to_string()).spawn().unwrap())
        .collect();
    for mut child in children {
        assert!(child.wait().unwrap().success());
    }

    let mut args: Vec<_> = calls(&agy).into_iter().map(|c| c.args[0].clone()).collect();
    args.sort();
    let mut want: Vec<_> = (0..40).map(|i| i.to_string()).collect();
    want.sort();
    assert_eq!(args, want);
}

#[test]
fn a_fake_made_twice_at_once_and_rewritten_while_running_still_runs() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_path_buf();
    let makers: Vec<_> = (0..8)
        .map(|_| {
            let path = path.clone();
            std::thread::spawn(move || {
                let mut codes = Vec::new();
                for round in 0..15 {
                    let made = fake(&path, "racy", &said("x", "", round % 2));
                    codes.push(Command::new(made).output().unwrap().status.code());
                }
                codes
            })
        })
        .collect();

    for maker in makers {
        for code in maker.join().unwrap() {
            assert!(matches!(code, Some(0 | 1)), "{code:?}");
        }
    }
}
