//! `dotnet-projects`: .NET repositories in temporary folders, read through `check` and `imports` (spec #617, "Testing
//! Decisions").

use std::{path::Path, process::Command};

use serde_json::{Value, json};

use crate::common::{has, project_text, write};

/// A map naming `dotnet-projects` whose parts are `parts`, each an id with its paths, and whose import links are
/// `links`.
fn map(parts: &[(&str, &[&str])], links: &[(&str, &str)]) -> Value {
    json!({
        "version": 2,
        "extractors": ["dotnet-projects"],
        "parts": parts.iter().map(|(id, paths)| json!({
            "id": id, "name": id, "caption": id, "purpose": "A part.", "holds": [], "tier": "core", "paths": paths
        })).collect::<Vec<_>>(),
        "actors": [],
        "links": links.iter().map(|(from, to)| json!({
            "id": format!("{from}-{to}"), "from": from, "to": to, "kind": "import"
        })).collect::<Vec<_>>(),
        "ignored": ["system-map.json"],
        "laws": []
    })
}

/// A repository holding `map` and `files` with their texts, committed.
fn repo(map: &Value, files: &[(&str, &str)]) -> (tempfile::TempDir, String) {
    let (dir, root) = project_text(&map.to_string(), &[]);
    for (file, text) in files {
        write(dir.path(), file, text);
    }
    studio_test_support::git(dir.path(), &["add", "-A"]);
    studio_test_support::git(dir.path(), &["commit", "-q", "-m", "Code"]);
    (dir, root)
}

fn report(root: &str) -> system_map::Report {
    system_map::check(Path::new(root))
}

/// Every file pair of the computed edges, as `from>to`.
fn pairs(report: &system_map::Report) -> Vec<String> {
    report
        .edges
        .iter()
        .flat_map(|e| e.pairs.iter().map(|p| format!("{}>{}", p.from, p.to)))
        .collect()
}

/// The file pairs a repository of `files` gives, its parts `a` owning only the files named `a/App.*` and `b` the rest,
/// with no link, so that only the pairs matter.
fn net_pairs(files: &[(&str, &str)]) -> Vec<String> {
    let (_dir, root) = repo(&map(&[("b", &["**"]), ("a", &["a/App.*"])], &[]), files);
    pairs(&report(&root))
}

fn run(root: &str, command: &str, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_system-map"))
        .arg(command)
        .args(args)
        .args(["--root", root])
        .output()
        .unwrap()
}

/// A project file holding `body` in an SDK-style `Project`.
fn project(body: &str) -> String {
    format!("<Project Sdk=\"Microsoft.NET.Sdk\">\n{body}</Project>\n")
}

/// A project file with one `ItemGroup` holding one `ProjectReference` to `include`.
fn refers(include: &str) -> String {
    project(&format!(
        "  <ItemGroup>\n    <ProjectReference Include=\"{include}\" />\n  </ItemGroup>\n"
    ))
}

const EMPTY: &str = "<Project Sdk=\"Microsoft.NET.Sdk\">\n</Project>\n";

#[test]
fn a_literal_reference_proves_its_link_and_a_link_without_one_fails() {
    let files = [
        ("app/App.csproj", &refers("..\\core\\Core.csproj")[..]),
        ("core/Core.csproj", EMPTY),
        ("web/Web.csproj", EMPTY),
    ];
    let parts: &[(&str, &[&str])] = &[("app", &["app/"]), ("core", &["core/"]), ("web", &["web/"])];
    let (_dir, root) = repo(&map(parts, &[("app", "core"), ("app", "web")]), &files);
    let found = report(&root);
    assert_eq!(pairs(&found), ["app/App.csproj>core/Core.csproj"]);
    assert_eq!(found.problems.len(), 1, "{:?}", found.problems);
    assert!(
        has(
            &found.problems,
            &["import link \"app-web\"", "no dependency"]
        ),
        "{:?}",
        found.problems
    );
}

/// The pairs from `a/App.csproj`, which holds `include` as a reference, with the project files `others` tracked.
fn from_app(include: &str, others: &[&str]) -> Vec<String> {
    let app = refers(include);
    let mut files = vec![("a/App.csproj", app.as_str())];
    files.extend(others.iter().map(|f| (*f, EMPTY)));
    net_pairs(&files)
}

#[test]
fn project_files_of_all_three_languages_pair() {
    for target in ["core/Core.csproj", "core/Core.fsproj", "core/Core.vbproj"] {
        let name = target.rsplit('/').next().unwrap();
        let found = from_app(&format!("..\\core\\{name}"), &[target]);
        assert_eq!(found, [format!("a/App.csproj>{target}")]);
    }
}

#[test]
fn a_semicolon_list_gives_one_pair_per_piece_and_an_escaped_semicolon_stays_in_one_name() {
    let found = from_app(
        " ..\\core\\Core.csproj ; ;..\\web\\Web.csproj;",
        &["core/Core.csproj", "web/Web.csproj"],
    );
    assert_eq!(
        found,
        [
            "a/App.csproj>core/Core.csproj",
            "a/App.csproj>web/Web.csproj"
        ]
    );
    let found = from_app(
        "..\\x%3By\\X.csproj",
        &["x;y/X.csproj", "x/X.csproj", "y/X.csproj"],
    );
    assert_eq!(found, ["a/App.csproj>x;y/X.csproj"]);
}

#[test]
fn an_entity_in_a_path_is_decoded_before_it_is_read() {
    let found = from_app("..\\a&amp;b\\X.csproj", &["a&b/X.csproj"]);
    assert_eq!(found, ["a/App.csproj>a&b/X.csproj"]);
}

#[test]
fn an_absolute_path_gives_nothing() {
    // `a/` is the project file's folder, so a join would name `a/core/Core.csproj` or `a/C:/...`.
    for include in [
        "/core/Core.csproj",
        "\\core\\Core.csproj",
        "\\\\server\\share\\Core.csproj",
        "C:\\core\\Core.csproj",
        "c:/core/Core.csproj",
        "%2Fcore%2FCore.csproj",
    ] {
        let found = from_app(
            include,
            &[
                "core/Core.csproj",
                "a/core/Core.csproj",
                "a/C:/core/Core.csproj",
            ],
        );
        assert_eq!(found, Vec::<String>::new(), "{include}");
    }
}

#[test]
fn the_project_folder_properties_read_as_the_project_files_own_folder() {
    let targets = ["a/Core/Core.csproj", "core/Core.csproj"];
    for include in [
        "$(MSBuildThisFileDirectory)Core\\Core.csproj",
        "$(msbuildthisfiledirectory)Core\\Core.csproj",
        "$(MSBuildProjectDirectory)\\Core\\Core.csproj",
        "$(MSBuildProjectDirectory)/Core/Core.csproj",
        "$(MSBUILDPROJECTDIRECTORY)\\..\\core\\Core.csproj",
    ] {
        let found = from_app(include, &targets);
        let want = if include.contains("..") {
            "core/Core.csproj"
        } else {
            "a/Core/Core.csproj"
        };
        assert_eq!(found, [format!("a/App.csproj>{want}")], "{include}");
    }
}

#[test]
fn a_stale_a_climbing_and_a_wrongly_cased_reference_give_nothing() {
    let found = from_app("..\\gone\\Gone.csproj", &["core/Core.csproj"]);
    assert_eq!(found, Vec::<String>::new());
    let found = from_app("..\\..\\core\\Core.csproj", &["core/Core.csproj"]);
    assert_eq!(found, Vec::<String>::new());
    let found = from_app("..\\Core\\core.csproj", &["core/Core.csproj"]);
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn dots_and_repeated_separators_collapse() {
    let found = from_app(
        ".\\..\\\\core\\.\\x\\..\\Core.csproj",
        &["core/Core.csproj"],
    );
    assert_eq!(found, ["a/App.csproj>core/Core.csproj"]);
}

#[test]
fn a_reference_to_a_tracked_esproj_pairs_and_its_part_stays_unproven() {
    let files = [
        ("a/App.csproj", &refers("..\\ui\\Ui.esproj")[..]),
        ("ui/Ui.esproj", "<Project />\n"),
    ];
    let parts: &[(&str, &[&str])] = &[("app", &["a/"]), ("ui", &["ui/"])];
    let (_dir, root) = repo(&map(parts, &[("app", "ui")]), &files);
    let found = report(&root);
    assert_eq!(pairs(&found), ["a/App.csproj>ui/Ui.esproj"]);
    assert_eq!(found.problems, Vec::<String>::new());
    assert_eq!(
        (found.counts.links.proven, found.counts.links.unproven),
        (0, 1)
    );
}

#[test]
fn a_reference_to_the_project_file_itself_gives_nothing() {
    assert_eq!(from_app("App.csproj", &[]), Vec::<String>::new());
}

#[test]
fn a_project_file_is_read_whatever_the_case_of_its_extension() {
    let found = net_pairs(&[
        ("a/App.CSPROJ", &refers("..\\core\\Core.FsProj")),
        ("core/Core.FsProj", EMPTY),
    ]);
    assert_eq!(found, ["a/App.CSPROJ>core/Core.FsProj"]);
}

/// What `a/App.csproj`, which holds `include` as a reference, leaves Unsure with the project files `others` tracked: each
/// finding's name, reason and candidates.
fn unsure_of(include: &str, others: &[&str]) -> Vec<(String, String, Vec<String>)> {
    let app = refers(include);
    let mut files = vec![("a/App.csproj", app.as_str())];
    files.extend(others.iter().map(|f| (*f, EMPTY)));
    let (_dir, root) = repo(&map(&[("b", &["**"]), ("a", &["a/App.*"])], &[]), &files);
    report(&root)
        .unsure
        .into_iter()
        .map(|u| {
            assert_eq!(u.file, "a/App.csproj");
            (u.name, u.reason, u.candidates)
        })
        .collect()
}

/// `unsure_of`'s one finding, as its reason and candidates.
fn one_unsure(include: &str, others: &[&str]) -> (String, Vec<String>) {
    let mut found = unsure_of(include, others);
    assert_eq!(found.len(), 1, "{include}: {found:?}");
    let (name, reason, candidates) = found.remove(0);
    assert_eq!(name, include);
    (reason, candidates)
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(ToString::to_string).collect()
}

/// `a/` holds the app, which refers to `src/Core/Core.csproj` through a property; `src/` and `lib/` are the other parts.
fn property_repo(links: &[(&str, &str)]) -> (tempfile::TempDir, String) {
    let parts: &[(&str, &[&str])] = &[("app", &["a/"]), ("core", &["src/"]), ("lib", &["lib/"])];
    repo(
        &map(parts, links),
        &[
            (
                "a/App.csproj",
                &refers("$(SourceRoot)src\\Core\\Core.csproj"),
            ),
            ("src/Core/Core.csproj", EMPTY),
            ("lib/Lib.csproj", EMPTY),
        ],
    )
}

#[test]
fn a_property_path_makes_the_file_unsure_and_relaxes_only_the_link_to_its_candidates_part() {
    let (_dir, root) = property_repo(&[("app", "core"), ("app", "lib")]);
    let out = run(&root, "check", &[]);
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "note: a/App.csproj is unsure of $(SourceRoot)src\\Core\\Core.csproj; its links to core pass as unproven\n"
    );
    // The link to `lib`, another one from the same part, still fails.
    let problems = String::from_utf8_lossy(&out.stdout);
    assert_eq!(problems.lines().count(), 1, "{problems}");
    assert!(problems.contains("import link \"app-lib\""), "{problems}");
    let out = run(&root, "check", &["--json"]);
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        value["unsure"],
        json!([{
            "file": "a/App.csproj",
            "name": "$(SourceRoot)src\\Core\\Core.csproj",
            "reason": "built from a property",
            "candidates": ["src/Core/Core.csproj"]
        }])
    );
    assert_eq!(value["edges"], json!([]));
}

#[test]
fn a_property_a_glued_name_and_a_wholly_computed_path_give_wide_candidates() {
    let all = ["src/MyCore.csproj", "src/Core.csproj", "web/Web.fsproj"];
    let (reason, candidates) = one_unsure("$(P)Core.csproj", &all);
    assert_eq!(reason, "built from a property");
    assert_eq!(candidates, ["src/Core.csproj", "src/MyCore.csproj"]);
    // An empty tail matches every other project file, but not the importing one.
    let (_, candidates) = one_unsure("$(CoreProject)", &all);
    assert_eq!(
        candidates,
        strings(&["src/Core.csproj", "src/MyCore.csproj", "web/Web.fsproj"])
    );
    let (_, candidates) = one_unsure("$(X)\\$(Y).csproj", &all);
    assert_eq!(
        candidates,
        strings(&["src/Core.csproj", "src/MyCore.csproj"])
    );
}

#[test]
fn an_item_list_and_a_wildcard_are_unsure_with_their_own_reasons() {
    let others = ["core/X.csproj", "web/X.csproj", "web/Y.csproj", "web/X.cs"];
    let (reason, candidates) = one_unsure("@(Refs)", &others);
    assert_eq!(reason, "built from an item list");
    assert_eq!(candidates.len(), 3);
    let (reason, candidates) = one_unsure("..\\*\\X.csproj", &others);
    assert_eq!(reason, "holds a wildcard");
    assert_eq!(candidates, ["core/X.csproj", "web/X.csproj"]);
    let (reason, _) = one_unsure("..\\cor?\\X.csproj", &others);
    assert_eq!(reason, "holds a wildcard");
    // A property wins over an item list, which wins over a wildcard.
    assert_eq!(one_unsure("$(A)@(B)*", &others).0, "built from a property");
    assert_eq!(one_unsure("@(B)*", &others).0, "built from an item list");
}

#[test]
fn the_tail_is_decoded_normalised_and_stripped_of_its_leading_dots() {
    let others = ["src/Core/Core.csproj", "lib/Core/Core.csproj"];
    let (_, candidates) = one_unsure("$(Root)src\\..\\src\\\\Core\\.\\Core.csproj", &others);
    assert_eq!(candidates, ["src/Core/Core.csproj"]);
    let (_, candidates) = one_unsure("$(Root)..\\..\\.\\lib/Core/Core.csproj", &others);
    assert_eq!(candidates, ["lib/Core/Core.csproj"]);
    let (_, candidates) = one_unsure("$(Root)src%5CCore%5CCore.csproj", &others);
    assert_eq!(candidates, ["src/Core/Core.csproj"]);
    // The tail starts after the last `)`, `*` or `?`.
    let (_, candidates) = one_unsure("$(Root)\\lib\\*\\Core\\Core.csproj", &others);
    assert_eq!(candidates, ["lib/Core/Core.csproj", "src/Core/Core.csproj"]);
}

#[test]
fn a_property_path_to_a_tracked_esproj_lists_it() {
    let (_, candidates) = one_unsure("$(Ui)Ui.esproj", &["ui/Ui.esproj", "ui/Ui.cs"]);
    assert_eq!(candidates, ["ui/Ui.esproj"]);
}

#[test]
fn a_semicolon_list_mixing_a_property_piece_and_a_literal_one_gives_one_unsure_and_one_pair() {
    let include = "$(P)Core.csproj;..\\core\\Core.csproj";
    let app = refers(include);
    let files = [("a/App.csproj", app.as_str()), ("core/Core.csproj", EMPTY)];
    let (_dir, root) = repo(&map(&[("b", &["**"]), ("a", &["a/App.*"])], &[]), &files);
    let found = report(&root);
    assert_eq!(pairs(&found), ["a/App.csproj>core/Core.csproj"]);
    assert_eq!(found.unsure.len(), 1, "{:?}", found.unsure);
    assert_eq!(found.unsure[0].name, "$(P)Core.csproj");
}

#[test]
fn glued_to_a_name_the_project_directory_property_is_unsure_and_no_pair() {
    let others = ["a/Core.csproj", "core/Core.csproj"];
    assert_eq!(
        from_app("$(MSBuildProjectDirectory)Core.csproj", &others),
        Vec::<String>::new()
    );
    let (reason, candidates) = one_unsure("$(MSBuildProjectDirectory)Core.csproj", &others);
    assert_eq!(reason, "built from a property");
    assert_eq!(candidates, strings(&others));
}

#[test]
fn a_tail_matching_no_file_gives_no_finding() {
    assert_eq!(unsure_of("$(P)Gone.csproj", &["core/Core.csproj"]), []);
    assert_eq!(unsure_of("@(Refs)", &[]), []);
    // Only a project file is a candidate, and the importing file never is.
    assert_eq!(unsure_of("$(P)App.csproj", &["core/App.cs"]), []);
}

/// The pairs from `a/App.csproj`, holding `text`, with `core/Core.csproj` tracked.
fn pairs_of(text: &str) -> Vec<String> {
    net_pairs(&[("a/App.csproj", text), ("core/Core.csproj", EMPTY)])
}

const PAIR: &str = "a/App.csproj>core/Core.csproj";

#[test]
fn a_reference_pairs_whatever_condition_or_choose_branch_it_sits_in() {
    let core = "<ProjectReference Include=\"..\\core\\Core.csproj\" />";
    for text in [
        project(&format!(
            "<ItemGroup Condition=\"'$(X)' == 'y'\">{core}</ItemGroup>"
        )),
        project(
            "<ItemGroup><ProjectReference Condition=\"false\" Include=\"..\\core\\Core.csproj\" /></ItemGroup>",
        ),
        project(&format!(
            "<Choose><When Condition=\"a\"><ItemGroup>{core}</ItemGroup></When></Choose>"
        )),
        project(&format!(
            "<Choose><When Condition=\"a\"/><Otherwise><ItemGroup>{core}</ItemGroup></Otherwise></Choose>"
        )),
        project(&format!(
            "<Choose><When Condition=\"a\"><Choose><When Condition=\"b\"><ItemGroup>{core}</ItemGroup></When></Choose></When></Choose>"
        )),
    ] {
        assert_eq!(pairs_of(&text), [PAIR], "{text}");
    }
}

#[test]
fn a_reference_that_changes_only_the_build_pairs_like_any_other() {
    let text = project(
        "<ItemGroup><ProjectReference Include=\"..\\core\\Core.csproj\" ReferenceOutputAssembly=\"false\" OutputItemType=\"Analyzer\">\
         <Private>false</Private></ProjectReference></ItemGroup>",
    );
    assert_eq!(pairs_of(&text), [PAIR]);
}

#[test]
fn update_remove_target_and_comment_references_give_nothing() {
    for text in [
        project("<ItemGroup><ProjectReference Update=\"..\\core\\Core.csproj\" /></ItemGroup>"),
        project("<ItemGroup><ProjectReference Remove=\"..\\core\\Core.csproj\" /></ItemGroup>"),
        project(
            "<Target Name=\"T\"><ItemGroup><ProjectReference Include=\"..\\core\\Core.csproj\" /></ItemGroup></Target>",
        ),
        project("<!-- <ProjectReference Include=\"..\\core\\Core.csproj\" /> -->"),
        project(
            "<ItemGroup><!-- <ProjectReference Include=\"..\\core\\Core.csproj\" /> --></ItemGroup>",
        ),
        project("<ProjectReference Include=\"..\\core\\Core.csproj\" />"),
        project(
            "<PropertyGroup><ProjectReference Include=\"..\\core\\Core.csproj\" /></PropertyGroup>",
        ),
        project("<ItemGroup><PackageReference Include=\"..\\core\\Core.csproj\" /></ItemGroup>"),
        project(
            "<ItemGroup><Other><ProjectReference Include=\"..\\core\\Core.csproj\" /></Other></ItemGroup>",
        ),
        // MSBuild rejects a mis-cased element or attribute, so neither is read.
        project("<itemgroup><ProjectReference Include=\"..\\core\\Core.csproj\" /></itemgroup>"),
        project("<ItemGroup><ProjectReference include=\"..\\core\\Core.csproj\" /></ItemGroup>"),
        project("<ItemGroup><ProjectReference /></ItemGroup>"),
    ] {
        assert_eq!(pairs_of(&text), Vec::<String>::new(), "{text}");
    }
}

#[test]
fn the_item_element_is_matched_in_any_case() {
    let text =
        project("<ItemGroup><projectreference Include=\"..\\core\\Core.csproj\" /></ItemGroup>");
    assert_eq!(pairs_of(&text), [PAIR]);
}

#[test]
fn a_legacy_project_in_the_2003_namespace_and_a_byte_order_mark_are_read() {
    let legacy = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<Project ToolsVersion=\"14.0\" DefaultTargets=\"Build\" \
         xmlns=\"http://schemas.microsoft.com/developer/msbuild/2003\">\n  <ItemGroup>\n    \
         <ProjectReference Include=\"..\\core\\Core.csproj\">\n      <Project>{1}</Project>\n    </ProjectReference>\n  \
         </ItemGroup>\n</Project>\n";
    assert_eq!(pairs_of(legacy), [PAIR]);
    let bom = format!("\u{feff}{}", refers("..\\core\\Core.csproj"));
    assert_eq!(pairs_of(&bom), [PAIR]);
}

#[test]
fn an_item_group_of_a_when_inside_a_target_stays_unread() {
    let text = project(
        "<Target Name=\"T\"><Choose><When Condition=\"a\"><ItemGroup>\
         <ProjectReference Include=\"..\\core\\Core.csproj\" /></ItemGroup></When></Choose></Target>",
    );
    assert_eq!(pairs_of(&text), Vec::<String>::new());
}

#[test]
fn a_file_that_is_not_a_project_gives_one_problem_naming_it_and_no_pair() {
    let good = "<Project>\n  <ItemGroup>\n    <ProjectReference Include=\"..\\core\\Core.csproj\" />\n  </ItemGroup>\n";
    for text in [
        // A mismatched end tag, and a truncated file.
        format!("{good}</Projekt>\n"),
        good.to_string(),
        format!("{good}</Project"),
        "<Project>\n  <!-- never closed\n".to_string(),
        // No file, no root, a root that is not `Project`, a second root.
        String::new(),
        "\n  \n".to_string(),
        "<?xml version=\"1.0\"?>\n<!-- only a comment -->\n".to_string(),
        "<Solution />\n".to_string(),
        format!("{good}</Project>\n<Project />\n"),
        format!("{good}</Project>\n<Other />\n"),
        // Text outside the root, a bare ampersand, an unknown entity, an attribute without quotes or twice.
        format!("junk\n{good}</Project>\n"),
        format!("{good}</Project>\njunk\n"),
        format!("{good}<A Include=\"x&unknown;y\" /></ItemGroup></Project>\n"),
        format!("{good}<A>&unknown;</A></ItemGroup></Project>\n"),
        format!("{good}<A b=c /></ItemGroup></Project>\n"),
        format!("{good}<A b=\"1\" b=\"2\" /></ItemGroup></Project>\n"),
        // A file that is not UTF-8 reads as garbage before the root.
        "\u{fffd}\u{fffd}<Project />\n".to_string(),
    ] {
        let (_dir, root) = repo(
            &map(&[("b", &["**"]), ("a", &["a/App.*"])], &[]),
            &[("a/App.csproj", &text), ("core/Core.csproj", EMPTY)],
        );
        let found = report(&root);
        assert_eq!(pairs(&found), Vec::<String>::new(), "{text:?}");
        assert_eq!(found.problems.len(), 1, "{text:?}: {:?}", found.problems);
        assert!(
            has(&found.problems, &["a/App.csproj", "not a project file"]),
            "{text:?}: {:?}",
            found.problems
        );
    }
}

#[test]
fn a_comment_a_declaration_a_doctype_and_cdata_around_the_root_are_no_problem() {
    let text = format!(
        "\u{feff}<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!-- before -->\n<!DOCTYPE Project>\n{}<!-- after -->\n",
        refers("..\\core\\Core.csproj")
    );
    let (_dir, root) = repo(
        &map(&[("b", &["**"]), ("a", &["a/App.*"])], &[("a", "b")]),
        &[("a/App.csproj", &text), ("core/Core.csproj", EMPTY)],
    );
    let found = report(&root);
    assert_eq!(found.problems, Vec::<String>::new());
    assert_eq!(pairs(&found), [PAIR]);
    let cdata = project(
        "<PropertyGroup><X><![CDATA[<ProjectReference Include=\"a\" />]]></X></PropertyGroup>",
    );
    assert_eq!(pairs_of(&cdata), Vec::<String>::new());
}

#[test]
fn an_ignored_broken_project_file_is_not_read() {
    let mut ignoring = map(&[("b", &["**"])], &[]);
    ignoring["ignored"] = json!(["system-map.json", "templates/"]);
    let (_dir, root) = repo(
        &ignoring,
        &[
            ("templates/T.csproj", "<Project>"),
            ("src/T.csproj", "<Project>"),
        ],
    );
    let problems = report(&root).problems;
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        has(&problems, &["src/T.csproj", "not a project file"]),
        "{problems:?}"
    );
}

/// Each file pair's import sites in `check --json`, as `from>to file:line`.
fn sites(root: &str, args: &[&str]) -> Vec<String> {
    let out = run(root, "check", &[&["--json"], args].concat());
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    let mut found = Vec::new();
    for edge in value["edges"].as_array().into_iter().flatten() {
        for pair in edge["pairs"].as_array().unwrap() {
            for site in pair["sites"].as_array().unwrap() {
                found.push(format!(
                    "{}>{} {}:{}",
                    pair["from"].as_str().unwrap(),
                    pair["to"].as_str().unwrap(),
                    site["file"].as_str().unwrap(),
                    site["line"]
                ));
            }
        }
    }
    found.sort();
    found
}

#[test]
fn a_pair_keeps_the_line_of_each_reference_and_a_start_tag_over_three_lines_gets_its_first() {
    let text = project(
        "  <ItemGroup>\n    <ProjectReference\n        Include=\"..\\core\\Core.csproj\"\n        Private=\"false\" />\n    \
         <ProjectReference Include=\"..\\core\\Core.csproj\" />\n  </ItemGroup>\n",
    );
    let (_dir, root) = repo(
        &map(&[("b", &["**"]), ("a", &["a/App.*"])], &[("a", "b")]),
        &[("a/App.csproj", &text), ("core/Core.csproj", EMPTY)],
    );
    assert_eq!(
        sites(&root, &[]),
        [
            "a/App.csproj>core/Core.csproj a/App.csproj:3",
            "a/App.csproj>core/Core.csproj a/App.csproj:6"
        ]
    );
}

#[test]
fn the_imports_report_counts_each_project_file_pair_as_one_unit() {
    // Two references between the same two files are one unit, and two files in one part are two.
    let two = project(
        "  <ItemGroup>\n    <ProjectReference Include=\"..\\core\\Core.csproj\" />\n    \
         <ProjectReference Include=\"..\\core\\Core.csproj\" />\n    \
         <ProjectReference Include=\"..\\core\\Util.csproj\" />\n  </ItemGroup>\n",
    );
    let (_dir, root) = repo(
        &map(
            &[("app", &["app/"]), ("core", &["core/"])],
            &[("app", "core")],
        ),
        &[
            ("app/App.csproj", &two),
            ("app/Other.csproj", &refers("..\\core\\Core.csproj")),
            ("core/Core.csproj", EMPTY),
            ("core/Util.csproj", EMPTY),
        ],
    );
    let out = run(&root, "imports", &["--json"]);
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        value["total"],
        json!({"raw": 3, "weighted": value["total"]["weighted"]})
    );
    assert_eq!(value["across"]["raw"], 3);
}

#[test]
fn check_suggests_dotnet_projects_for_a_repository_with_a_project_file_it_would_read() {
    let note = "note: dotnet-projects could read this repository; add it to extractors\n";
    let ignored = [("docs/D.csproj", EMPTY)];
    let none = [("src/Code.cs", "class C {}\n"), ("App.sln", "x\n")];
    for (files, expected) in [
        (&[("src/App.csproj", EMPTY)][..], note),
        (&[("src/App.FSPROJ", EMPTY)], note),
        (&ignored, ""),
        (&none, ""),
    ] {
        let mut map = map(&[("a", &["**"])], &[]);
        map["extractors"] = json!([]);
        map["ignored"] = json!(["system-map.json", "docs/"]);
        let mut files = files.to_vec();
        files.push(("docs/d.md", "x\n"));
        let (_dir, root) = repo(&map, &files);
        let out = run(&root, "check", &[]);
        assert_eq!(String::from_utf8_lossy(&out.stderr), expected, "{files:?}");
    }
}

#[test]
fn a_part_owning_no_project_file_is_not_read_and_its_links_pass_unproven() {
    let parts: &[(&str, &[&str])] = &[("code", &["code/"]), ("core", &["core/"])];
    let (_dir, root) = repo(
        &map(parts, &[("code", "core")]),
        &[("code/X.cs", "class X {}\n"), ("core/Core.csproj", EMPTY)],
    );
    let found = report(&root);
    assert_eq!(found.problems, Vec::<String>::new());
    assert_eq!(
        (found.counts.links.proven, found.counts.links.unproven),
        (0, 1)
    );
}

#[test]
fn package_references_solutions_and_central_package_files_give_no_edge() {
    let packages =
        project("<ItemGroup><PackageReference Include=\"Core\" Version=\"1.0.0\" /></ItemGroup>");
    let found = net_pairs(&[
        ("a/App.csproj", &packages),
        ("core/Core.csproj", EMPTY),
        (
            "App.sln",
            "Project(\"{X}\") = \"Core\", \"core\\Core.csproj\", \"{Y}\"\n",
        ),
        (
            "App.slnx",
            "<Solution><Project Path=\"core/Core.csproj\" /></Solution>\n",
        ),
        (
            "Directory.Packages.props",
            "<Project><ItemGroup><PackageVersion Include=\"Core\" Version=\"1\" /></ItemGroup></Project>\n",
        ),
    ]);
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn a_mixed_dotnet_and_typescript_repository_reads_each_languages_parts_side_by_side() {
    let mut mixed = map(
        &[
            ("netlib", &["core/"]),
            ("netapp", &["a/"]),
            ("tslib", &["web/lib/"]),
            ("tsapp", &["web/app/"]),
        ],
        &[("netapp", "netlib"), ("tsapp", "tslib")],
    );
    mixed["extractors"] = json!(["dotnet-projects", "typescript-imports"]);
    let (_dir, root) = repo(
        &mixed,
        &[
            ("a/App.csproj", &refers("..\\core\\Core.csproj")),
            ("core/Core.csproj", EMPTY),
            ("web/app/main.ts", "import { x } from \"../lib/x\";\n"),
            ("web/lib/x.ts", "export const x = 1;\n"),
        ],
    );
    let found = report(&root);
    assert_eq!(found.problems, Vec::<String>::new());
    assert_eq!(
        pairs(&found),
        [
            "a/App.csproj>core/Core.csproj",
            "web/app/main.ts>web/lib/x.ts"
        ]
    );
    assert_eq!(
        (found.counts.links.proven, found.counts.links.unproven),
        (2, 0)
    );
}

#[test]
fn commit_mode_gives_the_edges_of_the_working_tree() {
    let (dir, root) = repo(
        &map(&[("b", &["**"]), ("a", &["a/App.*"])], &[("a", "b")]),
        &[
            ("a/App.csproj", &refers("..\\core\\Core.csproj")),
            ("core/Core.csproj", EMPTY),
        ],
    );
    let tree = run(&root, "check", &["--json"]);
    let commit = run(&root, "check", &["--json", "--commit", "HEAD"]);
    assert_eq!(
        String::from_utf8_lossy(&commit.stdout),
        String::from_utf8_lossy(&tree.stdout)
    );
    assert_eq!(
        sites(&root, &["--commit", "HEAD"]),
        ["a/App.csproj>core/Core.csproj a/App.csproj:3"]
    );
    // An edit not committed changes nothing at the commit.
    write(dir.path(), "a/App.csproj", EMPTY);
    assert_eq!(
        sites(&root, &["--commit", "HEAD"]),
        ["a/App.csproj>core/Core.csproj a/App.csproj:3"]
    );
    assert_eq!(sites(&root, &[]), Vec::<String>::new());
}
