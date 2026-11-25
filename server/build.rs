//! Lists the built page (`../dist`) for the binary to embed, as Tauri's build did for the app. With no `dist` (a plain
//! `cargo test` before `npm run build`) the list is empty and the binary serves nothing.

use std::fmt::Write as _;
use std::{env, fs, path::Path};

fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, String)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(std::fs::DirEntry::path);
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, base, out);
        } else if path.extension().is_none_or(|e| e != "map") {
            let name = path
                .strip_prefix(base)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.push((name, path.to_string_lossy().into_owned()));
        }
    }
}

fn main() {
    let dist = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("../dist");
    println!("cargo:rerun-if-changed=../dist");
    let mut files = vec![];
    walk(&dist, &dist, &mut files);
    let mut source = String::from("pub static ASSETS: &[(&str, &[u8])] = &[\n");
    for (name, path) in files {
        // Writing to a `String` cannot fail.
        writeln!(source, "    ({name:?}, include_bytes!({path:?})),").ok();
    }
    source += "];\n";
    fs::write(
        Path::new(&env::var("OUT_DIR").unwrap()).join("assets.rs"),
        source,
    )
    .unwrap();
}
