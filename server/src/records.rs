//! The per-project record store the map text review and micro-worlds keep their records in (spec #720, Implementation
//! Decisions 4b): a folder per project in the store, a JSON file per record, the change notice that tells the page a project's
//! records changed, the writing lock that serialises a record's read, change and write, and the in-flight set of the runs whose
//! end is being read, so each is read once.
//!
//! Each user keeps its own record type, folder and file names: the map text review `map-reviews/<project>-<hash>/<pr>.json`,
//! micro-worlds `micro-worlds/<project>-<hash>/<key in hex>.json`. The folder of a project is its path, readable, and its FNV-1a
//! hash ([`project_folder`]), as the builds before this store named it, so the records they wrote read on.

use serde::{Serialize, de::DeserializeOwned};
use std::{
    collections::HashSet,
    fs, io,
    marker::PhantomData,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, OnceLock, PoisonError},
};

type Told = Box<dyn Fn(&str) + Send + Sync>;

/// What is told the project whose state changed; the first listener set wins.
#[derive(Default)]
pub(crate) struct Notice(OnceLock<Told>);

impl Notice {
    /// Sets what is told the project whose state changed; the first call wins.
    pub(crate) fn set(&self, told: impl Fn(&str) + Send + Sync + 'static) {
        // Set once; a later set keeps the first value, which is the same.
        self.0.set(Box::new(told)).ok();
    }

    /// Tells `project` changed, once; nothing before a listener is set.
    pub(crate) fn tell(&self, project: &str) {
        if let Some(told) = self.0.get() {
            told(project);
        }
    }
}

/// Every project's records of type `R`, a JSON file each, in a folder per project.
pub(crate) struct Records<R> {
    store: PathBuf,
    notice: Notice,
    /// The runs whose end is being read.
    in_flight: Mutex<HashSet<String>>,
    /// Serialises the read, change and write of a record.
    writing: Mutex<()>,
    record: PhantomData<fn() -> R>,
}

impl<R: Serialize + DeserializeOwned> Records<R> {
    /// The records kept in `store`, a folder per project.
    pub(crate) fn new(store: PathBuf) -> Self {
        Records {
            store,
            notice: Notice::default(),
            in_flight: Mutex::default(),
            writing: Mutex::default(),
            record: PhantomData,
        }
    }

    /// Sets what is told the project whose records changed; the first call wins.
    pub(crate) fn on_change(&self, told: impl Fn(&str) + Send + Sync + 'static) {
        self.notice.set(told);
    }

    /// Tells `project`'s records changed.
    pub(crate) fn tell(&self, project: &str) {
        self.notice.tell(project);
    }

    /// `project`'s folder.
    pub(crate) fn folder(&self, project: &str) -> PathBuf {
        project_folder(&self.store, project)
    }

    /// The file of `project`'s record `name`: `<name>.json` in its folder.
    pub(crate) fn file(&self, project: &str, name: &str) -> PathBuf {
        self.folder(project).join(format!("{name}.json"))
    }

    /// `project`'s record `name`; none when there is none, or it does not read as an `R`.
    pub(crate) fn read(&self, project: &str, name: &str) -> Option<R> {
        read_record(&self.file(project, name))
    }

    /// Writes `record` as `project`'s record `name`, whole, by a temporary file and a rename.
    pub(crate) fn write(&self, project: &str, name: &str, record: &R) -> io::Result<()> {
        fs::create_dir_all(self.folder(project))?;
        studio_workbench::write_atomic(&self.file(project, name), &serde_json::to_vec(record)?)
    }

    /// Every record of `project`'s that reads as an `R`.
    pub(crate) fn records(&self, project: &str) -> Vec<R> {
        records_in(&self.folder(project))
    }

    /// Every project's records that read as an `R`.
    pub(crate) fn every(&self) -> Vec<R> {
        let folders = fs::read_dir(&self.store).into_iter().flatten().flatten();
        folders
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .flat_map(|folder| records_in(&folder))
            .collect()
    }

    /// The writing lock, held across a record's read, change and write.
    pub(crate) fn lock(&self) -> MutexGuard<'_, ()> {
        self.writing.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The writing lock, if no one holds it.
    pub(crate) fn try_lock(&self) -> Option<MutexGuard<'_, ()>> {
        self.writing.try_lock().ok()
    }

    /// Claims run `id`'s end to read: false when another caller has it already.
    pub(crate) fn claim(&self, id: &str) -> bool {
        self.in_flight().insert(id.to_string())
    }

    /// Whether run `id`'s end is being read.
    pub(crate) fn claimed(&self, id: &str) -> bool {
        self.in_flight().contains(id)
    }

    /// Run `id`'s end is read.
    pub(crate) fn release(&self, id: &str) {
        self.in_flight().remove(id);
    }

    fn in_flight(&self) -> MutexGuard<'_, HashSet<String>> {
        self.in_flight
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

fn read_record<R: DeserializeOwned>(file: &Path) -> Option<R> {
    serde_json::from_slice(&fs::read(file).ok()?).ok()
}

/// The records in `folder`: its `.json` files that read as an `R`.
fn records_in<R: DeserializeOwned>(folder: &Path) -> Vec<R> {
    let files = fs::read_dir(folder).into_iter().flatten().flatten();
    files
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "json"))
        .filter_map(|path| read_record(&path))
        .collect()
}

/// `project`'s folder in `store`: its path, readable, and a hash of it, so two paths that read alike never share one.
pub(crate) fn project_folder(store: &Path, project: &str) -> PathBuf {
    let name: String = project
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    store.join(format!("{name}-{:016x}", fnv1a(project)))
}

/// FNV-1a of `text`: the same in every build, as a name or hash kept across builds must be.
pub(crate) fn fnv1a(text: impl AsRef<[u8]>) -> u64 {
    text.as_ref()
        .iter()
        .copied()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        })
}

#[cfg(test)]
mod tests;
