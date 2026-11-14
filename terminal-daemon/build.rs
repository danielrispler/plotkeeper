// Hashes the crate's `.rs` sources, `Cargo.toml`, this script and the workspace `Cargo.lock` into `DAEMON_BUILD_ID`,
// which the daemon sends in `hello`, so the app can tell a daemon older than its build without anyone bumping a
// number.
use std::{
    fs,
    path::{Path, PathBuf},
};

fn rust_sources(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_sources(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
}

fn main() {
    let mut files: Vec<PathBuf> = ["Cargo.toml", "build.rs", "../Cargo.lock"]
        .map(PathBuf::from)
        .into();
    rust_sources(Path::new("src"), &mut files);
    files.sort();
    // A new or deleted source file changes the directory, not any file listed below.
    println!("cargo:rerun-if-changed=src");
    // FNV-1a: stable across toolchains, unlike `DefaultHasher`.
    let mut hash: u64 = 0xcbf29ce484222325;
    for file in &files {
        println!("cargo:rerun-if-changed={}", file.display());
        let bytes = [
            file.to_string_lossy().as_bytes(),
            &[0],
            &fs::read(file).unwrap(),
            &[0],
        ]
        .concat();
        for b in bytes {
            hash = (hash ^ b as u64).wrapping_mul(0x100000001b3);
        }
    }
    println!("cargo:rustc-env=DAEMON_BUILD_ID={hash:016x}");
}
