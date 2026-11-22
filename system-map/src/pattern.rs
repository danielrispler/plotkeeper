//! The map's path patterns: a trailing `/` owns a folder, `*` matches within one segment, `**`
//! across segments, and anything else names one file.

pub struct Pattern {
    text: String,
    folder: bool,
    segments: Vec<String>,
}

impl Pattern {
    pub fn new(text: &str) -> Pattern {
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

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn matches(&self, path: &str) -> bool {
        let path: Vec<&str> = path.split('/').collect();
        if self.folder {
            // Some folder above the file, never the file itself.
            (1..path.len()).any(|end| segments_match(&self.segments, &path[..end]))
        } else {
            segments_match(&self.segments, &path)
        }
    }
}

fn segments_match(pattern: &[String], path: &[&str]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((first, rest)) if first == "**" => {
            (0..=path.len()).any(|skip| segments_match(rest, &path[skip..]))
        }
        Some((first, rest)) => match path.split_first() {
            Some((name, tail)) => segment_matches(first, name) && segments_match(rest, tail),
            None => false,
        },
    }
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
