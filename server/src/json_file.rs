//! The server's JSON state files: one project's entry changed in place, and a whole file written by a
//! temporary file and a rename, so a crash never leaves one cut short.

use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::{fmt, path::Path, sync::Mutex};

/// Why a state file was not written.
#[derive(Debug)]
pub(crate) enum UpdateError {
    /// The file's `projects` is not an object, so no project's entry can go in it.
    NotObjects(&'static str),
    /// The file's path has no folder.
    NoFolder(&'static str),
    Io(std::io::Error),
    Json(serde_json::Error),
    Persist(tempfile::PersistError),
}

impl fmt::Display for UpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UpdateError::NotObjects(name) => write!(f, "{name}: projects is not an object"),
            UpdateError::NoFolder(name) => write!(f, "no folder for {name}"),
            UpdateError::Io(e) => e.fmt(f),
            UpdateError::Json(e) => e.fmt(f),
            UpdateError::Persist(e) => e.fmt(f),
        }
    }
}

impl From<UpdateError> for String {
    fn from(e: UpdateError) -> Self {
        e.to_string()
    }
}

/// `file` as a JSON object; a missing or unreadable file, or one that holds no object, is `{}`.
pub(crate) fn read_object(file: &Path) -> Value {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}))
}

/// `project`'s entry in `file`'s `projects`; a missing or unreadable one is the default.
pub(crate) fn read_project<T: DeserializeOwned + Default>(file: &Path, project: &str) -> T {
    read_object(file)
        .get("projects")
        .and_then(|projects| projects.get(project))
        .and_then(|entry| serde_json::from_value(entry.clone()).ok())
        .unwrap_or_default()
}

/// Changes `project`'s entry in `file` (`name` in errors) with `change`, keeping the other projects'
/// and any fields this build does not know, and `finish`es the whole file before it is written.
/// `writing` serialises the changes to the file, so two never lose one another.
pub(crate) fn update_json_file<T: Serialize + DeserializeOwned + Default>(
    file: &Path,
    name: &'static str,
    writing: &Mutex<()>,
    project: &str,
    change: impl FnOnce(&mut T),
    finish: impl FnOnce(&mut Value),
) -> Result<T, UpdateError> {
    let _writing = writing.lock().unwrap();
    let mut whole = read_object(file);
    let mut mine: T = read_project(file, project);
    change(&mut mine);
    let entry = whole
        .as_object_mut()
        .unwrap()
        .entry("projects")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or(UpdateError::NotObjects(name))?
        .entry(project)
        .or_insert_with(|| serde_json::json!({}));
    let saved = serde_json::to_value(&mine).map_err(UpdateError::Json)?;
    match entry.as_object_mut() {
        Some(object) => object.extend(saved.as_object().unwrap().clone()),
        None => *entry = saved,
    }
    finish(&mut whole);
    write_json(file, name, &whole)?;
    Ok(mine)
}

/// Writes `value` as `file` (`name` in errors), pretty, by a temporary file beside it and a rename.
pub(crate) fn write_json(
    file: &Path,
    name: &'static str,
    value: &impl Serialize,
) -> Result<(), UpdateError> {
    let dir = file.parent().ok_or(UpdateError::NoFolder(name))?;
    std::fs::create_dir_all(dir).map_err(UpdateError::Io)?;
    let mut temp = tempfile::NamedTempFile::new_in(dir).map_err(UpdateError::Io)?;
    serde_json::to_writer_pretty(&mut temp, value).map_err(UpdateError::Json)?;
    temp.persist(file).map_err(UpdateError::Persist)?;
    Ok(())
}
