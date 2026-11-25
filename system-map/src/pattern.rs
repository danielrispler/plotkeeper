//! The map's path patterns: a trailing `/` owns a folder, `*` matches within one segment, `**`
//! across segments, and anything else names one file.

pub(crate) struct Pattern {
    text: String,
    folder: bool,
    segments: Vec<String>,
}

impl Pattern {
    pub(crate) fn new(text: &str) -> Pattern {
        let folder = text.ends_with('/');
        let segments = text
            .trim_end_matches('/')
            .split('/')
            .map(str::to_string)
            .collect();
        Pattern {
            text: text.to_string(),
            folder,
            segments,
        }
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    pub(crate) fn matches(&self, path: &str) -> bool {
        let path: Vec<&str> = path.split('/').collect();
        if self.folder {
            // Some folder above the file, never the file itself.
            (1..path.len()).any(|end| segments_match(&self.segments, &path[..end]))
        } else {
            segments_match(&self.segments, &path)
        }
    }
}

/// Whether `pattern` matches all of `path`. Each (pattern index, path index) is worked out once,
/// so a run of `**` costs no more than its length times the path's.
fn segments_match(pattern: &[String], path: &[&str]) -> bool {
    let mut memo = vec![vec![None; path.len() + 1]; pattern.len() + 1];
    walk(pattern, path, 0, 0, &mut memo)
}

fn walk(
    pattern: &[String],
    path: &[&str],
    p: usize,
    at: usize,
    memo: &mut Vec<Vec<Option<bool>>>,
) -> bool {
    if let Some(known) = memo[p][at] {
        return known;
    }
    let found = match pattern.get(p) {
        None => at == path.len(),
        Some(first) if first == "**" => {
            (at..=path.len()).any(|skip| walk(pattern, path, p + 1, skip, memo))
        }
        Some(first) => {
            at < path.len()
                && segment_matches(first, path[at])
                && walk(pattern, path, p + 1, at + 1, memo)
        }
    };
    memo[p][at] = Some(found);
    found
}

/// `*` matches any run of characters within one segment.
fn segment_matches(pattern: &str, name: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if parts.len() == 1 {
        return pattern == name;
    }
    let Some(mut rest) = name.strip_prefix(first) else {
        return false;
    };
    for middle in &parts[1..parts.len() - 1] {
        match rest.find(middle) {
            Some(at) => rest = &rest[at + middle.len()..],
            None => return false,
        }
    }
    rest.ends_with(last)
}
