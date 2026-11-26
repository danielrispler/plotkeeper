//! The hook binaries' copies in the data folder's `bin/` (#356), on a temporary home.

use std::{fs, os::unix::fs::MetadataExt, os::unix::fs::PermissionsExt, path::Path};
use studio_server::hook_copies::{Refresh, notice, refresh};

/// A folder holding the hook binaries as a build leaves them beside `plotkeeper`, with `tag` in their bytes.
fn beside(root: &Path, tag: &str) -> std::path::PathBuf {
    let dir = root.join(format!("beside-{tag}"));
    fs::create_dir_all(&dir).unwrap();
    for name in ["plotkeeper-hook", "plotkeeper-answer"] {
        let path = dir.join(name);
        fs::write(&path, format!("#!/bin/sh\necho {name} {tag}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    dir
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
}

#[test]
fn a_start_with_no_bin_creates_it_with_both_copies() {
    let home = tempfile::tempdir().unwrap();
    let from = beside(home.path(), "v1");
    let bin = studio_hook::bin_dir(home.path());

    let done = refresh(&from, &bin).unwrap();

    assert_eq!(
        done,
        Refresh::Replaced(vec!["plotkeeper-hook", "plotkeeper-answer"])
    );
    #[cfg(target_os = "macos")]
    let expected = "Library/Application Support/plotkeeper/bin";
    // With no `XDG_DATA_HOME`, as the tests run.
    #[cfg(target_os = "linux")]
    let expected = ".local/share/plotkeeper/bin";
    assert_eq!(bin, home.path().join(expected));
    for name in ["plotkeeper-hook", "plotkeeper-answer"] {
        assert_eq!(
            read(&bin.join(name)),
            format!("#!/bin/sh\necho {name} v1\n")
        );
        let mode = fs::metadata(bin.join(name)).unwrap().mode();
        assert_eq!(mode & 0o111, 0o111, "{name} is not executable: {mode:o}");
    }
}

#[test]
fn a_start_with_equal_copies_leaves_them_untouched() {
    let home = tempfile::tempdir().unwrap();
    let from = beside(home.path(), "v1");
    let bin = studio_hook::bin_dir(home.path());
    refresh(&from, &bin).unwrap();
    let before: Vec<_> = ["plotkeeper-hook", "plotkeeper-answer"]
        .map(|name| fs::metadata(bin.join(name)).unwrap())
        .map(|m| (m.ino(), m.mtime_nsec(), m.mtime()))
        .into();

    let done = refresh(&from, &bin).unwrap();

    assert_eq!(done, Refresh::Current);
    let after: Vec<_> = ["plotkeeper-hook", "plotkeeper-answer"]
        .map(|name| fs::metadata(bin.join(name)).unwrap())
        .map(|m| (m.ino(), m.mtime_nsec(), m.mtime()))
        .into();
    assert_eq!(before, after);
}

#[test]
fn a_start_with_a_differing_copy_replaces_it_by_rename() {
    let home = tempfile::tempdir().unwrap();
    let bin = studio_hook::bin_dir(home.path());
    refresh(&beside(home.path(), "v1"), &bin).unwrap();
    let old_hook = fs::metadata(bin.join("plotkeeper-hook")).unwrap().ino();
    let from = beside(home.path(), "v2");
    fs::copy(
        bin.join("plotkeeper-answer"),
        from.join("plotkeeper-answer"),
    )
    .unwrap();

    let done = refresh(&from, &bin).unwrap();

    assert_eq!(done, Refresh::Replaced(vec!["plotkeeper-hook"]));
    assert_eq!(
        read(&bin.join("plotkeeper-hook")),
        "#!/bin/sh\necho plotkeeper-hook v2\n"
    );
    // A new file renamed into place, never the old one rewritten, so a hook starting mid-replace runs one whole binary.
    assert_ne!(
        fs::metadata(bin.join("plotkeeper-hook")).unwrap().ino(),
        old_hook
    );
    let mut left: Vec<_> = fs::read_dir(&bin)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    left.sort();
    assert_eq!(left, ["plotkeeper-answer", "plotkeeper-hook"]);
    let mode = fs::metadata(bin.join("plotkeeper-hook")).unwrap().mode();
    assert_eq!(mode & 0o111, 0o111, "not executable: {mode:o}");
}

#[test]
fn a_start_with_the_binaries_missing_says_so_once_and_keeps_the_copies() {
    let home = tempfile::tempdir().unwrap();
    let bin = studio_hook::bin_dir(home.path());
    refresh(&beside(home.path(), "v1"), &bin).unwrap();
    let from = beside(home.path(), "v2");
    fs::remove_file(from.join("plotkeeper-answer")).unwrap();

    let done = refresh(&from, &bin).unwrap();

    assert_eq!(done, Refresh::Missing(vec!["plotkeeper-answer"]));
    for name in ["plotkeeper-hook", "plotkeeper-answer"] {
        assert_eq!(
            read(&bin.join(name)),
            format!("#!/bin/sh\necho {name} v1\n")
        );
    }
    let said = notice(&done, &from).unwrap();
    assert_eq!(said.lines().count(), 1, "{said}");
    assert!(said.contains("plotkeeper-answer"), "{said}");
    assert!(said.contains(&from.display().to_string()), "{said}");
    assert_eq!(notice(&Refresh::Current, &from), None);
}
