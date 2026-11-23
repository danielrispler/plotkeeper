//! The hook binaries' copies in the data folder's `bin/` (#356, spec #320 "Hooks call binaries at a stable path").

use std::{
    fs,
    io::{self, Write},
    os::unix::fs::PermissionsExt,
    path::Path,
};
use studio_hook::HOOK_BINARIES;

/// What a start did to the copies.
#[derive(Debug, PartialEq, Eq)]
pub enum Refresh {
    /// Every copy already matched.
    Current,
    /// These copies were written.
    Replaced(Vec<&'static str>),
    /// These binaries are not beside the executable; no copy was touched.
    Missing(Vec<&'static str>),
}

/// Makes each copy in `bin` match the binary of its name in `beside`.
/// When one is missing beside the executable it touches no copy, so the hooks never run a mix of two builds.
pub fn refresh(beside: &Path, bin: &Path) -> io::Result<Refresh> {
    let missing: Vec<_> = HOOK_BINARIES
        .into_iter()
        .filter(|name| !beside.join(name).is_file())
        .collect();
    if !missing.is_empty() {
        return Ok(Refresh::Missing(missing));
    }
    fs::create_dir_all(bin)?;
    let mut replaced = Vec::new();
    for name in HOOK_BINARIES {
        let source = fs::read(beside.join(name))?;
        if fs::read(bin.join(name)).is_ok_and(|copy| copy == source) {
            continue;
        }
        replace(&source, &bin.join(name))?;
        replaced.push(name);
    }
    Ok(if replaced.is_empty() {
        Refresh::Current
    } else {
        Refresh::Replaced(replaced)
    })
}

/// Writes `bytes` to a temporary file beside `copy`, makes it executable and renames it over `copy`, so the path
/// holds the old binary or the new one, never part of one. A failure removes the temporary file.
fn replace(bytes: &[u8], copy: &Path) -> io::Result<()> {
    let folder = copy.parent().unwrap_or(Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(folder)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    fs::set_permissions(file.path(), fs::Permissions::from_mode(0o755))?;
    file.persist(copy).map_err(|e| e.error)?;
    Ok(())
}

/// The one line a start prints about the copies, if any.
pub fn notice(done: &Refresh, beside: &Path) -> Option<String> {
    let Refresh::Missing(missing) = done else {
        return None;
    };
    Some(format!(
        "plotkeeper: {} not found in {}; the hooks keep the copies they have",
        missing.join(" and "),
        beside.display()
    ))
}
