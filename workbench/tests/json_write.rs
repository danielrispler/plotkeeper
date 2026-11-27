//! The one atomic write of a whole JSON file (#728): a temporary file beside the target, then a rename over it.

use std::{fs, os::unix::fs::PermissionsExt, path::Path};
use studio_workbench::write_atomic;

/// The names in `dir`, sorted.
fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn a_write_leaves_the_file_holding_exactly_the_contents_and_nothing_beside_it() {
    let cases: [(&str, Option<&str>, &str); 2] = [
        ("a new file", None, r#"{"a":1}"#),
        (
            "a shorter write over a longer file",
            Some(r#"{"old":"a value longer than the new one"}"#),
            r#"{"b":2}"#,
        ),
    ];
    for (case, old, new) in cases {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("state.json");
        if let Some(old) = old {
            fs::write(&file, old).unwrap();
        }

        write_atomic(&file, new.as_bytes()).unwrap();

        let held = fs::read_to_string(&file).ok();
        assert_eq!(held.as_deref(), Some(new), "{case}");
        assert_eq!(names(dir.path()), ["state.json"], "{case}");
    }
}

#[test]
fn a_failed_write_returns_its_error_and_leaves_the_old_file_as_it_was() {
    const OLD: &str = r#"{"old":true}"#;
    // (case, the folder written into exists, it can be written to)
    let cases = [
        ("a folder that does not exist", false, true),
        ("a folder that cannot be written to", true, false),
    ];
    for (case, exists, writable) in cases {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("state.json");
        fs::write(&old, OLD).unwrap();
        let folder = if exists {
            dir.path().to_path_buf()
        } else {
            dir.path().join("missing")
        };
        if !writable {
            fs::set_permissions(&folder, fs::Permissions::from_mode(0o555)).unwrap();
        }

        let written = write_atomic(&folder.join("state.json"), br#"{"new":true}"#);
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();

        assert!(written.is_err(), "{case}: {written:?}");
        assert_eq!(fs::read_to_string(&old).unwrap(), OLD, "{case}");
        assert_eq!(names(dir.path()), ["state.json"], "{case}");
    }
}
