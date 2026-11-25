//! The writer the wire tests share (#246), which each crate's `tests/wire.rs` includes by path: values serialized as
//! the shell serializes them, written as `export const` literals typed with their mirror in `ui/types.ts`. `tsc -p
//! scripts/wire` checks the files, so a field the mirror lacks, or one it has that the wire does not, fails.

use serde_json::Value;
use std::fmt::Write;
use std::path::Path;

/// A sample: the name of its mirror in `ui/`, and its value as the shell sends it.
pub(crate) type Sample = (&'static str, Value);

pub(crate) fn json(value: impl serde::Serialize) -> Value {
    serde_json::to_value(value).expect("a wire value serializes")
}

/// The mirrors that live in another module of `ui/` than `types.ts`, by type.
const ELSEWHERE: [(&str, &str); 3] = [
    ("CheckOutput", "evidence.ts"),
    ("Facts", "ask.ts"),
    ("Hunk", "ask.ts"),
];

/// Writes `samples` into `scripts/wire/<name>.ts`, from the crate at `manifest_dir`, once each of `types` has one.
pub(crate) fn write(manifest_dir: &str, name: &str, types: &[&str], samples: &[Sample]) {
    for ty in types {
        assert!(samples.iter().any(|(t, _)| t == ty), "no sample of {ty}");
    }
    let mut types: Vec<&str> = samples.iter().map(|(t, _)| *t).collect();
    types.sort();
    types.dedup();
    let module = |ty: &str| {
        ELSEWHERE
            .iter()
            .find(|(t, _)| *t == ty)
            .map_or("types.ts", |(_, m)| *m)
    };
    let mut ts =
        format!("// Written by `{name}/tests/wire.rs` (#246); `tsc -p scripts/wire` checks it.\n");
    for ty in types {
        writeln!(
            ts,
            "import type {{ {ty} }} from \"../../ui/{}\";",
            module(ty)
        )
        .unwrap();
    }
    for (i, (ty, value)) in samples.iter().enumerate() {
        let literal = serde_json::to_string_pretty(value).unwrap();
        write!(ts, "\nexport const sample{i}: {ty} = {literal};\n").unwrap();
    }
    let path = Path::new(manifest_dir).join(format!("../scripts/wire/{name}.ts"));
    std::fs::write(&path, ts).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
}
