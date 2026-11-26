//! The `dotnet-projects` extractor: the `ProjectReference`s of .NET project files, read as XML text with no `dotnet`,
//! MSBuild or NuGet run (ADR 0002), each paired with the tracked project file its path names (spec #617).

use std::borrow::Cow;

use quick_xml::{Reader, XmlVersion, escape::resolve_predefined_entity, events::Event};

use super::{Extractor, Output, Pair, Repo, Unsure, folders::folder_of};

pub(crate) struct DotnetProjects;

impl Extractor for DotnetProjects {
    fn name(&self) -> &'static str {
        "dotnet-projects"
    }

    /// A tracked C#, F# or Visual Basic project file, its extension in any case, as MSBuild takes any project file name.
    fn reads(&self, file: &str) -> bool {
        is_project_file(file)
    }

    /// A test project (spec #617, "`skips`"): unless it holds `<IsTestProject>false</IsTestProject>`, one under a test
    /// folder or named for tests, or marked as one in the file. A file that is not a project is not skipped, wherever
    /// it sits, so it keeps its Problem.
    fn skips(&self, repo: &Repo, file: &str) -> bool {
        let Some(text) = repo.read(file) else {
            return false;
        };
        let Ok(project) = parse(&text) else {
            return false;
        };
        !project.not_test && (is_test_path(file) || project.test_marker)
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
                for written in pieces(&reference.include) {
                    let piece = in_own_folder(written);
                    // A piece built from a property, an item list or a wildcard cannot be resolved without evaluation.
                    if let Some(reason) = computed_reason(piece) {
                        let tail = literal_tail(piece);
                        let candidates: Vec<String> = repo
                            .files()
                            .iter()
                            .copied()
                            .filter(|other| {
                                *other != file
                                    && other.to_ascii_lowercase().ends_with("proj")
                                    && !repo.is_ignored(other)
                                    && other.ends_with(tail.as_str())
                            })
                            .map(String::from)
                            .collect();
                        // A tail that matches no file is no finding.
                        if !candidates.is_empty() {
                            output.unsure.push(Unsure {
                                file: file.to_string(),
                                name: written.to_string(),
                                reason: reason.into(),
                                candidates,
                            });
                        }
                        continue;
                    }
                    if let Some(to) = resolve(folder_of(file), piece)
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
            test_file: Some(|name| format!("{name}/{name}.Tests.csproj")),
            skip: vec![(
                Case::MalformedConfig,
                "it has no config file; a malformed project file is covered by the behaviour tests",
            )],
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
    /// It holds `<IsTestProject>false</IsTestProject>`, which wins over every other rule.
    not_test: bool,
    /// It marks itself a test project: `IsTestProject` true, a `Microsoft.NET.Test.Sdk` package or the `MSTest.Sdk` SDK.
    test_marker: bool,
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
    let (mut not_test, mut test_marker) = (false, false);
    // The depth of an `IsTestProject` being read and the text it holds so far.
    let mut reading: Option<(usize, String)> = None;
    loop {
        // The position after the last event is where this one starts, so a start tag gives its own first line.
        let at = reader.buffer_position() as usize;
        let event = reader.read_event().map_err(|e| e.to_string())?;
        if let Some(content) = content(&event)? {
            // Only whitespace text may sit outside the root.
            if stack.is_empty()
                && !(matches!(event, Event::Text(_))
                    && content
                        .chars()
                        .all(|c| matches!(c, ' ' | '\t' | '\r' | '\n')))
            {
                return Err("it holds text outside its root element".into());
            }
            // An `IsTestProject` value is the text directly in it, not in a child.
            if let Some((depth, buffer)) = reading.as_mut()
                && *depth == stack.len() - 1
            {
                buffer.push_str(&content);
            }
        }
        match &event {
            Event::Start(tag) | Event::Empty(tag) => {
                let name = tag.local_name().as_ref().to_string();
                if stack.is_empty() {
                    if seen_root {
                        return Err("it holds a second root element".into());
                    }
                    // MSBuild rejects a prefixed root (`m:Project`), so the root's whole name is matched.
                    let whole = tag.name();
                    let whole = whole.as_ref();
                    if whole != "Project" {
                        return Err(format!("its root element is {whole}, not Project"));
                    }
                    seen_root = true;
                }
                let is_reference =
                    name.eq_ignore_ascii_case("ProjectReference") && in_group(&stack, "ItemGroup");
                let is_package =
                    name.eq_ignore_ascii_case("PackageReference") && in_group(&stack, "ItemGroup");
                if name.eq_ignore_ascii_case("IsTestProject")
                    && in_group(&stack, "PropertyGroup")
                    && matches!(event, Event::Start(_))
                {
                    reading = Some((stack.len(), String::new()));
                }
                // Every element's attributes are read, so a bad one in any element is a problem.
                for attribute in tag.attributes() {
                    let attribute = attribute.map_err(|e| e.to_string())?;
                    let value = attribute
                        .normalized_value(XmlVersion::Implicit1_0)
                        .map_err(|e| e.to_string())?;
                    let key = attribute.key.as_ref();
                    if is_reference && key == "Include" {
                        references.push(Reference {
                            line: 1 + text[..at].matches('\n').count() as u32,
                            include: value.into_owned(),
                        });
                    } else {
                        test_marker |= marks_test(&name, key, &value, is_package, stack.is_empty());
                    }
                }
                if matches!(event, Event::Start(_)) {
                    stack.push(name);
                }
            }
            Event::End(_) => {
                stack.pop();
                if let Some((_, buffer)) = reading.take_if(|(depth, _)| *depth == stack.len())
                    && let Some(on) = flag(&buffer)
                {
                    test_marker |= on;
                    not_test |= !on;
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
    Ok(Project {
        references,
        not_test,
        test_marker,
    })
}

/// The text `event` adds to the element it sits in: text, CDATA as written, or a character or predefined entity
/// reference decoded. An unknown entity makes the file not a project.
fn content<'a>(event: &Event<'a>) -> Result<Option<Cow<'a, str>>, String> {
    Ok(match event {
        Event::Text(t) => Some(t.xml10_content()),
        Event::CData(c) => Some(c.xml10_content()),
        Event::GeneralRef(entity) => Some(Cow::Owned(
            match entity.resolve_char_ref().map_err(|e| e.to_string())? {
                Some(c) => c.to_string(),
                None => resolve_predefined_entity(entity)
                    .ok_or_else(|| format!("it holds an unknown entity &{};", &**entity))?
                    .to_string(),
            },
        )),
        _ => None,
    })
}

/// Whether `stack`, the names of the open elements, ends in a `group` (an `ItemGroup` or a `PropertyGroup`) that MSBuild
/// evaluates: a child of `Project`, or of a `When` or `Otherwise` of a `Choose` at any depth. One in a `Target` runs at
/// build time and is left out.
fn in_group(stack: &[String], group: &str) -> bool {
    let [root, branches @ .., last] = stack else {
        return false;
    };
    root == "Project"
        && last == group
        && branches.chunks(2).all(|pair| {
            matches!(pair, [choose, branch] if choose == "Choose" && (branch == "When" || branch == "Otherwise"))
        })
}

/// The value of an `IsTestProject`: `true` or `false`, trimmed and in any case, as MSBuild conditions compare.
fn flag(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// Whether the attribute `key` of the element `name` marks a test project: a `PackageReference`'s `Include` naming
/// `Microsoft.NET.Test.Sdk`, or an SDK named `MSTest.Sdk` by the root's `Sdk`, an `Import`'s `Sdk` or an `<Sdk Name>`.
fn marks_test(name: &str, key: &str, value: &str, is_package: bool, is_root: bool) -> bool {
    if is_package && key == "Include" {
        return pieces(value).any(|piece| piece.eq_ignore_ascii_case("Microsoft.NET.Test.Sdk"));
    }
    let sdk = (name == "Project" && key == "Sdk" && is_root)
        || (name == "Import" && key == "Sdk")
        || (name == "Sdk" && key == "Name");
    sdk && names_mstest_sdk(value)
}

/// The test suffixes of Arcade's `Tests.props`, matched case-sensitively at the end of a name.
const TEST_SUFFIXES: [&str; 4] = [
    ".Tests",
    ".UnitTests",
    ".IntegrationTests",
    ".PerformanceTests",
];

/// Whether the path of project file `file` says it is a test project: a folder named `test` or `tests` in any case, or
/// ending with a test suffix, at any depth, or a name without the extension ending with one.
fn is_test_path(file: &str) -> bool {
    let (folders, name) = file.rsplit_once('/').map_or(("", file), |(f, n)| (f, n));
    let stem = name.rfind('.').map_or(name, |dot| &name[..dot]);
    folders.split('/').any(|folder| {
        folder.eq_ignore_ascii_case("test")
            || folder.eq_ignore_ascii_case("tests")
            || TEST_SUFFIXES.iter().any(|suffix| folder.ends_with(suffix))
    }) || TEST_SUFFIXES.iter().any(|suffix| stem.ends_with(suffix))
}

/// Whether an `Sdk` attribute or `<Sdk Name>` value, a `;` list of names each with an optional `/version`, names
/// `MSTest.Sdk`; NuGet ids compare in any case.
fn names_mstest_sdk(value: &str) -> bool {
    value.split(';').any(|sdk| {
        sdk.split('/')
            .next()
            .is_some_and(|name| name.trim().eq_ignore_ascii_case("MSTest.Sdk"))
    })
}

/// The pieces of an `Include`, in MSBuild's order, which splits the value while it is still escaped: split on `;`,
/// trimmed, empty ones dropped.
fn pieces(include: &str) -> impl Iterator<Item = &str> {
    include
        .split(';')
        .map(str::trim)
        .filter(|piece| !piece.is_empty())
}

/// `piece` without a leading `$(MSBuildThisFileDirectory)`, or `$(MSBuildProjectDirectory)` with a separator after it,
/// and the separators that follow, since both name the project file's own folder and MSBuild reads `a/` joined to
/// `\..\b` as `a//../b`.
fn in_own_folder(piece: &str) -> &str {
    let rest = strip_property(piece, "$(MSBuildThisFileDirectory)").or_else(|| {
        strip_property(piece, "$(MSBuildProjectDirectory)")
            .filter(|rest| rest.starts_with(['\\', '/']))
    });
    rest.map_or(piece, |rest| rest.trim_start_matches(['\\', '/']))
}

/// `piece` without its leading `property`, whose name MSBuild matches in any case.
fn strip_property<'a>(piece: &'a str, property: &str) -> Option<&'a str> {
    let head = piece.get(..property.len())?;
    head.eq_ignore_ascii_case(property)
        .then(|| &piece[property.len()..])
}

/// Why a piece is a path the extractor cannot resolve without evaluating the project, the first that applies: it holds
/// a property, an item list or a wildcard. None for a literal piece.
fn computed_reason(piece: &str) -> Option<&'static str> {
    if piece.contains("$(") {
        Some("built from a property")
    } else if piece.contains("@(") {
        Some("built from an item list")
    } else if piece.contains(['*', '?']) {
        Some("holds a wildcard")
    } else {
        None
    }
}

/// What a computed piece spells out literally at its end: the text after its last `)`, `*` or `?`, escapes decoded and
/// `\` read as `/`, then collapsed (repeated `/`, `.` segments, `x/..`) and stripped of its leading `..` segments, so
/// that a project file's path ending with it, as text, is a candidate for the piece. Empty when nothing is literal.
fn literal_tail(piece: &str) -> String {
    let after = piece
        .rfind([')', '*', '?'])
        .map_or(piece, |at| &piece[at + 1..]);
    collapse(&slashed(after)).0
}

/// `piece` with its `%XX` escapes decoded and every `\` read as `/`.
fn slashed(piece: &str) -> String {
    unescape(piece).replace('\\', "/")
}

/// The `/` path `path` collapsed: empty and `.` segments dropped and `x/..` folded, and whether a `..` climbed above
/// its start, which drops it.
fn collapse(path: &str) -> (String, bool) {
    let mut parts: Vec<&str> = Vec::new();
    let mut climbed = false;
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => climbed |= parts.pop().is_none(),
            part => parts.push(part),
        }
    }
    (parts.join("/"), climbed)
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
    let path = slashed(piece);
    if is_absolute(&path) {
        return None;
    }
    let (path, climbed) = collapse(&format!("{folder}{path}"));
    (!climbed).then_some(path)
}
