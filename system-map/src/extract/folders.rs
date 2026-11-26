//! Folder helpers the extractors share: a tracked path's folder, and the folder above one, as prefixes.

/// The folder holding `file`, as a prefix: empty for the root, else ending in `/`.
pub(crate) fn folder_of(file: &str) -> &str {
    file.rfind('/').map_or("", |at| &file[..=at])
}

/// The folder above `folder`, or none for the root.
pub(crate) fn parent(folder: &str) -> Option<&str> {
    if folder.is_empty() {
        return None;
    }
    Some(folder_of(&folder[..folder.len() - 1]))
}
