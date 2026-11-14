// Hashes the crate's sources into `DAEMON_BUILD`, which the daemon sends in `hello`, so the app can tell a daemon
// older than its build without anyone bumping a number.
use std::{fs, path::Path};

fn collect(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, files);
        } else {
            files.push(path);
        }
    }
}

fn main() {
    let mut files = vec![Path::new("Cargo.toml").to_path_buf()];
    collect(Path::new("src"), &mut files);
    files.sort();
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
    println!("cargo:rustc-env=DAEMON_BUILD={hash:016x}");
}
