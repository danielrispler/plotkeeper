// Studio records trees from several threads at once (the follow loop, the file and ask commands,
// the card jobs): each call needs a scratch index of its own.
use std::{
    fs,
    path::{Path, PathBuf},
    thread,
};
use studio_hook::{record_tree, record_tree_within};
use studio_test_support::git;

#[test]
fn concurrent_recordings_of_one_worktree_all_give_its_tree() {
    let dir = std::env::temp_dir().join(format!("studio-record-tree-{}", std::process::id()));
    // Cleanup: a leftover temp file fails no test.
    fs::remove_dir_all(&dir).ok();
    fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q"]);
    for i in 0..300 {
        fs::write(dir.join(format!("f{i}.txt")), format!("file {i}\n")).unwrap();
    }
    let trees: Vec<_> = (0..8)
        .map(|_| {
            let dir = dir.clone();
            thread::spawn(move || (0..4).map(|_| record_tree(&dir)).collect::<Vec<_>>())
        })
        .collect::<Vec<_>>()
        .into_iter()
        .flat_map(|t| t.join().unwrap())
        .collect();
    // Cleanup: a leftover temp file fails no test.
    fs::remove_dir_all(&dir).ok();
    let first = trees[0].clone().expect("a tree");
    assert!(
        trees.iter().all(|t| t.as_ref() == Some(&first)),
        "{trees:?}"
    );
}

/// A fresh repository in the temp dir, named for `name`.
fn repo(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("studio-record-tree-{name}-{}", std::process::id()));
    // Cleanup: a leftover temp file fails no test.
    fs::remove_dir_all(&dir).ok();
    fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q"]);
    dir
}

/// The paths in `tree` of the repository `dir`.
fn paths(dir: &Path, tree: &str) -> Vec<String> {
    git(dir, &["ls-tree", "-r", "--name-only", tree])
        .lines()
        .map(Into::into)
        .collect()
}

// #141: a parallel run's worker target dirs sit at the main checkout's root, and the hook hashed
// them at every turn.
#[test]
fn a_workers_target_dir_is_never_in_a_tree() {
    let dir = repo("target");
    fs::write(dir.join(".gitignore"), include_str!("../../.gitignore")).unwrap();
    fs::write(dir.join("kept.txt"), "kept\n").unwrap();
    fs::create_dir_all(dir.join("target-123/debug")).unwrap();
    fs::write(dir.join("target-123/debug/studio"), "built\n").unwrap();

    let (_, tree) = record_tree(&dir).expect("a tree");

    let paths = paths(&dir, &tree);
    // Cleanup: a leftover temp file fails no test.
    fs::remove_dir_all(&dir).ok();
    assert!(paths.contains(&"kept.txt".to_owned()), "{paths:?}");
    assert!(paths.iter().all(|p| !p.starts_with("target-")), "{paths:?}");
}

#[test]
fn a_tree_with_more_untracked_files_than_its_cap_is_not_recorded() {
    let dir = repo("files");
    for i in 0..20 {
        fs::write(dir.join(format!("f{i}.txt")), format!("file {i}\n")).unwrap();
    }

    let capped = record_tree_within(&dir, 10, u64::MAX);
    let within = record_tree_within(&dir, 20, u64::MAX);

    // Cleanup: a leftover temp file fails no test.
    fs::remove_dir_all(&dir).ok();
    assert_eq!(capped, None);
    assert!(within.is_some());
}

#[test]
fn a_tree_with_more_untracked_bytes_than_its_cap_is_not_recorded() {
    let dir = repo("bytes");
    fs::write(dir.join("big.bin"), vec![b'x'; 4096]).unwrap();

    let capped = record_tree_within(&dir, usize::MAX, 4095);
    let within = record_tree_within(&dir, usize::MAX, 4096);

    // Cleanup: a leftover temp file fails no test.
    fs::remove_dir_all(&dir).ok();
    assert_eq!(capped, None);
    assert!(within.is_some());
}
