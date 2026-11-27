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

/// `path` joined to `folder` (a prefix: empty for the root, else ending in `/`), with empty and `.` segments dropped
/// and `..` popping a segment, as a path with no trailing `/`; and whether a `..` climbed above the root, which drops it.
pub(crate) fn collapse(folder: &str, path: &str) -> (String, bool) {
    let mut parts: Vec<&str> = folder.split('/').filter(|s| !s.is_empty()).collect();
    let mut climbed = false;
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => climbed |= parts.pop().is_none(),
            _ => parts.push(segment),
        }
    }
    (parts.join("/"), climbed)
}

/// [`collapse`], or none when `path` climbs above the root.
pub(crate) fn join(folder: &str, path: &str) -> Option<String> {
    let (joined, climbed) = collapse(folder, path);
    (!climbed).then_some(joined)
}

/// `path` (no trailing `/`) as a folder prefix: empty for the root, else ending in `/`.
pub(crate) fn prefix(path: &str) -> String {
    if path.is_empty() {
        String::new()
    } else {
        format!("{path}/")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_cases() {
        let cases: [(&str, &str, Option<&str>); 11] = [
            ("a/b/", "c/d", Some("a/b/c/d")),
            ("", "c", Some("c")),
            ("a/b/", "./c", Some("a/b/c")),
            ("a/b/", ".", Some("a/b")),
            ("a/b/", "../c", Some("a/c")),
            ("a/b/", "../../c", Some("c")),
            ("a/b/", "../../..", None),
            ("a/", "../../c", None),
            ("a//b/", "c///d", Some("a/b/c/d")),
            ("a/", "c/", Some("a/c")),
            ("", "", Some("")),
        ];
        for (folder, path, want) in cases {
            assert_eq!(join(folder, path).as_deref(), want, "{folder:?} + {path:?}");
        }
    }

    #[test]
    fn collapse_drops_a_climb_and_says_so() {
        assert_eq!(collapse("", "../a/../../b"), ("b".to_string(), true));
        assert_eq!(collapse("a/", "b"), ("a/b".to_string(), false));
    }

    #[test]
    fn prefix_cases() {
        assert_eq!(prefix(""), "");
        assert_eq!(prefix("a/b"), "a/b/");
    }
}
