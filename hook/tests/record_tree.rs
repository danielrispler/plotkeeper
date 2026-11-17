// Studio records trees from several threads at once (the follow loop, the file and ask commands,
// the card jobs): each call needs a scratch index of its own.
use std::{fs, path::PathBuf, process::Command, thread};
use studio_hook::record_tree;

fn git(dir: &PathBuf, args: &[&str]) {
    let ok = Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

#[test]
fn concurrent_recordings_of_one_worktree_all_give_its_tree() {
    let dir = std::env::temp_dir().join(format!("studio-record-tree-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
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
    let _ = fs::remove_dir_all(&dir);
    let first = trees[0].clone().expect("a tree");
    assert!(
        trees.iter().all(|t| t.as_ref() == Some(&first)),
        "{trees:?}"
    );
}
