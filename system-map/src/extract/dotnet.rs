//! The `dotnet-projects` extractor: the `ProjectReference`s of .NET project files, read as XML text with no `dotnet`,
//! MSBuild or NuGet run (ADR 0002), each paired with the tracked project file its path names (spec #617).

use quick_xml::{Reader, XmlVersion, escape::resolve_predefined_entity, events::Event};

use super::{Extractor, Output, Pair, Repo};

pub(crate) struct DotnetProjects;

impl Extractor for DotnetProjects {
    fn name(&self) -> &'static str {
        "dotnet-projects"
    }

    /// A tracked C#, F# or Visual Basic project file, its extension in any case, as MSBuild takes any project file name.
    fn reads(&self, file: &str) -> bool {
        is_project_file(file)
    }

    fn extract(&self, repo: &Repo) -> Output {
        let mut output = Output::default();
        for file in repo.files().iter().copied() {
            if !is_project_file(file) || repo.is_ignored(file) {
                continue;
            }
            let Some(text) = repo.read(file) else {
                continue;
            };
            let project = match parse(&text) {
                Ok(project) => project,
                Err(why) => {
                    output
                        .problems
                        .push(format!("{file}: not a project file: {why}"));
                    continue;
                }
            };
            for reference in project.references {
                for piece in pieces(&reference.include) {
                    // A piece that is built from a property, an item list or a wildcard resolves to nothing for now.
                    if is_computed(&piece) {
                        continue;
                    }
                    if let Some(to) = resolve(folder_of(file), &piece)
                        && repo.files().contains(to.as_str())
                    {
                        output.pairs.push(Pair::at(file, &to, reference.line));
                    }
                }
            }
        }
        output
    }

    #[cfg(test)]
    fn fixture(&self) -> super::contract::Fixture {
        use super::contract::{Case, Fixture, MissingConfig};
        Fixture {
            support: Vec::new(),
            source: |name| format!("{name}/{name}.csproj"),
            // `from` and `to` are paths from the root, so `from`'s folder reaches `to` through `..`.
            importing: |_, to| {
                format!(
                    "<Project Sdk=\"Microsoft.NET.Sdk\">\n  <ItemGroup>\n    <ProjectReference Include=\"..\\{}\" />\n  \
                     </ItemGroup>\n</Project>\n",
                    to.replace('/', "\\")
                )
            },
            plain: |_| "<Project Sdk=\"Microsoft.NET.Sdk\">\n</Project>\n".into(),
            missing_config: MissingConfig::None,
            malformed_config: None,
            test_file: None,
            skip: vec![
                (
                    Case::MalformedConfig,
                    "it has no config file; a malformed project file is covered by the behaviour tests",
                ),
                (
                    Case::TestFile,
                    "it skips no test project until the skips ticket (#624) lands",
                ),
            ],
        }
    }
}

/// Whether `file` names a C#, F# or Visual Basic project file.
fn is_project_file(file: &str) -> bool {
    let lower = file.to_ascii_lowercase();
    [".csproj", ".fsproj", ".vbproj"]
        .iter()
        .any(|ext| lower.ends_with(ext))
}

/// What a project file holds that the extractor reads.
struct Project {
    /// Each `ProjectReference` with an `Include`, in file order.
    references: Vec<Reference>,
}

/// One `ProjectReference` item.
struct Reference {
    /// The line of its `<`.
    line: u32,
    /// Its `Include`, entity-decoded, as written otherwise.
    include: String,
}

/// Reads the project file `text`, or says why it is not one: quick-xml finds it ill-formed, or it has no single root
/// element named `Project`, an element never closed, an unknown entity or text other than whitespace outside the root.
fn parse(text: &str) -> Result<Project, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut reader = Reader::from_str(text);
    let mut stack: Vec<String> = Vec::new();
    let mut seen_root = false;
    let mut references = Vec::new();
    loop {
        // The position after the last event is where this one starts, so a start tag gives its own first line.
        let at = reader.buffer_position() as usize;
        let event = reader.read_event().map_err(|e| e.to_string())?;
        match &event {
            Event::Start(tag) | Event::Empty(tag) => {
                let name = tag.local_name().as_ref().to_string();
                if stack.is_empty() {
                    if seen_root {
                        return Err("it holds a second root element".into());
                    }
                    if name != "Project" {
                        return Err(format!("its root element is {name}, not Project"));
                    }
                    seen_root = true;
                }
                let is_reference =
                    name.eq_ignore_ascii_case("ProjectReference") && in_item_group(&stack);
                // Every element's attributes are read, so a bad one in any element is a problem.
                for attribute in tag.attributes() {
                    let attribute = attribute.map_err(|e| e.to_string())?;
                    let value = attribute
                        .normalized_value(XmlVersion::Implicit1_0)
                        .map_err(|e| e.to_string())?;
                    if is_reference && attribute.key.as_ref() == "Include" {
                        references.push(Reference {
                            line: 1 + text[..at].matches('\n').count() as u32,
                            include: value.into_owned(),
                        });
                    }
                }
                if matches!(event, Event::Start(_)) {
                    stack.push(name);
                }
            }
            Event::End(_) => {
                stack.pop();
            }
            Event::Text(t)
                if stack.is_empty()
                    && !t.bytes().all(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n')) =>
            {
                return Err("it holds text outside its root element".into());
            }
            Event::CData(_) if stack.is_empty() => {
                return Err("it holds text outside its root element".into());
            }
            Event::GeneralRef(entity) => {
                if stack.is_empty() {
                    return Err("it holds text outside its root element".into());
                }
                let known = entity
                    .resolve_char_ref()
                    .map_err(|e| e.to_string())?
                    .is_some()
                    || resolve_predefined_entity(entity).is_some();
                if !known {
                    return Err(format!("it holds an unknown entity &{};", &**entity));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !seen_root {
        return Err("it holds no root element".into());
    }
    if let Some(open) = stack.last() {
        return Err(format!("its element {open} is never closed"));
    }
    Ok(Project { references })
}

/// Whether `stack`, the names of the open elements, ends in an `ItemGroup` that MSBuild evaluates: a child of `Project`,
/// or of a `When` or `Otherwise` of a `Choose` at any depth. One in a `Target` runs at build time and is left out.
fn in_item_group(stack: &[String]) -> bool {
    let [root, branches @ .., last] = stack else {
        return false;
    };
    root == "Project"
        && last == "ItemGroup"
        && branches.chunks(2).all(|pair| {
            matches!(pair, [choose, branch] if choose == "Choose" && (branch == "When" || branch == "Otherwise"))
        })
}

/// The folder holding `file`, as a prefix: empty for the root, else ending in `/`.
fn folder_of(file: &str) -> &str {
    file.rfind('/').map_or("", |at| &file[..=at])
}

/// The pieces of an `Include`, in MSBuild's order, which splits the value while it is still escaped: split on `;`,
/// trimmed, empty ones dropped, and a leading `$(MSBuildThisFileDirectory)`, or `$(MSBuildProjectDirectory)` with the
/// separator after it, taken off, since both name the project file's own folder.
fn pieces(include: &str) -> Vec<String> {
    include
        .split(';')
        .map(str::trim)
        .filter(|piece| !piece.is_empty())
        .map(|piece| {
            if let Some(rest) = strip_property(piece, "$(MSBuildThisFileDirectory)") {
                return rest.to_string();
            }
            if let Some(rest) = strip_property(piece, "$(MSBuildProjectDirectory)")
                && let Some(rest) = rest.strip_prefix(['\\', '/'])
            {
                return rest.to_string();
            }
            piece.to_string()
        })
        .collect()
}

/// `piece` without its leading `property`, whose name MSBuild matches in any case.
fn strip_property<'a>(piece: &'a str, property: &str) -> Option<&'a str> {
    let head = piece.get(..property.len())?;
    head.eq_ignore_ascii_case(property)
        .then(|| &piece[property.len()..])
}

/// Whether a piece still shows a path the extractor cannot resolve without evaluating the project: it holds a property,
/// an item list or a wildcard.
fn is_computed(piece: &str) -> bool {
    piece.contains("$(") || piece.contains("@(") || piece.contains(['*', '?'])
}

/// `%XX` escapes decoded, as MSBuild does after it has split the value; one that is not two hex digits stays as written.
fn unescape(piece: &str) -> String {
    let bytes = piece.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .filter(|h| h.bytes().all(|b| b.is_ascii_hexdigit()))
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (bytes[i], hex) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (byte, _) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Whether `path` is absolute: it starts with a separator, or with a drive letter and `:`.
fn is_absolute(path: &str) -> bool {
    path.starts_with(['/', '\\'])
        || matches!(path.as_bytes(), [drive, b':', ..] if drive.is_ascii_alphabetic())
}

/// The path a literal piece names from the project file's `folder`: escapes decoded, `\` read as `/`, joined to the
/// folder and collapsed. None for an absolute path, and for one that climbs out of the repository.
fn resolve(folder: &str, piece: &str) -> Option<String> {
    let path = unescape(piece);
    if is_absolute(&path) {
        return None;
    }
    let joined = format!("{folder}{}", path.replace('\\', "/"));
    let mut parts: Vec<&str> = Vec::new();
    for part in joined.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    Some(parts.join("/"))
}
