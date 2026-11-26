# .NET extractor: the rules a text-only `dotnet-projects` must follow

Research notes for issue #541. They set out how a text-only `dotnet-projects` extractor turns a .NET repository's project files (`.csproj`, `.fsproj`, `.vbproj`) into `(dependent project file, dependency project file)` pairs over its tracked files, without running `dotnet`, MSBuild or NuGet restore. They extend the "C#" section of `language-extractors.md` and the Go and Java notes, `go-extractor.md` and `java-extractor.md`, and don't repeat them. Studio terms follow `docs/studio/CONTEXT.md`. The extractor contract is `docs/studio/behaviour/map.md`, "The extractors", and spec #529.

## How this was read

- MSBuild rules come from the Microsoft Learn MSBuild reference, read from its Markdown source in `MicrosoftDocs/visualstudio-docs` at `c81086b` (2026-11-26) with `curl`, so quotes are verbatim. A cite like `msbuild-items.md:40` is a line of `docs/msbuild/msbuild-items.md` there, published at https://learn.microsoft.com/en-us/visualstudio/msbuild/msbuild-items. The .NET SDK property reference (`docs/core/project-sdk/msbuild-props.md`, `overview.md`) was read the same way from `dotnet/docs` at `e9abfc2`, and Central Package Management from `NuGet/docs.microsoft.com-nuget` at `30c0a12`.
- MSBuild itself was read at `dotnet/msbuild` `main` (`df20ae8`) through raw.githubusercontent.com: `src/Build/Evaluation/ProjectParser.cs`, `ItemSpec.cs`, `SemiColonTokenizer.cs`, `LazyItemEvaluator.IncludeOperation.cs`, `src/Build/Xml/ProjectXmlUtilities.cs`, `XmlReaderExtension.cs`, `src/Build/Collections/ItemDictionary.cs`, `src/Build/Instance/ProjectItemInstance.cs`, `src/Build/BackEnd/Components/RequestBuilder/IntrinsicTasks/MSBuild.cs`, `src/Framework/FileUtilities.cs`, `src/Tasks/Microsoft.Common.CurrentVersion.targets` and `Microsoft.Common.props`. Line numbers will drift.
- Test markers come from the packages' own build files: `microsoft/vstest` `9b5dc68` (`src/package/Microsoft.NET.Test.Sdk/`), `microsoft/testfx` `03ae017` (`src/MSTest/MSTest.Sdk/`, `MSTest.TestAdapter/`, `src/MTP/Microsoft.Testing.Platform.MSBuild/`), `xunit/xunit` `f7bd8ab` (`src/xunit.v3.core/Package/buildTransitive/`), `dotnet/sdk` `f8e69e5` and `dotnet/arcade` `bca80a2` (`src/Microsoft.DotNet.Arcade.Sdk/tools/Tests.props`).
- `quick-xml` was read at `tafia/quick-xml` `master` (`c0d031a`): `Cargo.toml`, `src/reader/mod.rs`, `src/events/mod.rs` and `src/name.rs`; crates.io gives 0.42.0 as its latest release.
- Six public repositories were shallow-cloned into a scratch folder, not this repository: akkadotnet/akka.net `f2f9a55`, linq2db/linq2db `bdbce2d`, dotnet/orleans `6a0e937`, dotnet/efcore `e867ee0`, fsprojects/Paket `ec2542b`, dotnet/fsharp `73c8ada`. Nothing was built or restored. A throwaway Python scan parsed every tracked project file and counted the forms in §6.
- Each claim is tagged **[cited]** (with its source and line), **[measured]** (counted by that scan), **[repo]** (checked in this repository at `10e3a03`) or **[judgement]** (my own reading). **Unverified** marks what no primary source settled.

## 1. `ProjectReference` items

### What the item is

- "Represents a reference to another project. `ProjectReference` items are transformed into Reference items by the `ResolveProjectReferences` target". **[cited]** `common-msbuild-project-items.md:84`. It is an ordinary item: `<ProjectReference Include="…" />` as a child of an `ItemGroup`, its metadata as child elements or, since MSBuild 15, as attributes (`item-element-msbuild.md:37-55`). **[cited]**
- In .NET (Core and 5+) "project references *are* transitive. You can code against Project3 in Project1", and in .NET Framework they are not. **[cited]** `common-msbuild-project-items.md:102`. So A's code may use C through B with no `ProjectReference` from A to C. **[judgement]**

### The path

- "The `Include` attribute is a path that is interpreted relative to the project file's folder, `$(MSBuildProjectPath)`, even if the item is in an imported file such as a `.targets` file." **[cited]** `msbuild-items.md:40`. There is no reserved property `MSBuildProjectPath`; the reserved one is `MSBuildProjectDirectory`, "the absolute path of the directory where the project file is located" (`msbuild-reserved-and-well-known-properties.md:62`), so the docs mean that. **[cited]**, the name a docs slip, high confidence.
- `Import` differs: "in `Import` elements, the path is always interpreted relative to the project that contains the `Import` element". **[cited]** `import-element-msbuild.md:64`. linq2db's `Tests/linq2db.TestProjects.props` holds `..\..\Source\…` references that resolve from each importing project's folder, not the `.props` file's. **[measured]**
- A plain value is kept as written: an `Include` fragment with no wildcard becomes a `ValueFragment` of the escaped text, and the item is created from it unchanged. **[cited]** `ItemSpec.cs:228-235`, `LazyItemEvaluator.IncludeOperation.cs:89-97`. MSBuild later splits references into those that exist on disk and those that don't (`Exists('%(Identity)')`), and a missing one is warning MSB9008, an error only with `ErrorOnMissingProjectReference`. **[cited]** `Microsoft.Common.CurrentVersion.targets:1689-1690`, `:2215-2224`.

### Lists, wildcards, properties, escapes

- Several paths: "the file names are separated by a semicolon". **[cited]** `msbuild-items.md:32-37`. Properties are expanded first, then the value is split on `;`; "Fragments are trimmed and empty fragments discarded". **[cited]** `ItemSpec.cs:186-208`, `SemiColonTokenizer.cs:14-18`.
- Wildcards `?`, `*` and `**` are allowed in any `Include` (`msbuild-items.md:58-64`), and a fragment with one becomes a glob expanded against the disk. **[cited]** `ItemSpec.cs:228-248`.
- `$(Name)` is a property, `@(Type)` an item list, `%XX` an escaped character (`%3B` is `;`, `%2A` is `*`). **[cited]** `msbuild-special-characters.md:27-37`. A property's value can come from the project, any import (`Directory.Build.props` is imported by `Microsoft.Common.props:19-34`), the command line or the environment. **[cited]** for the import; the rest is MSBuild's general model, not re-read here, unverified.
- `$(MSBuildThisFileDirectory)` is "the directory portion of `MSBuildThisFileFullPath`", with the final backslash. **[cited]** `msbuild-reserved-and-well-known-properties.md:71`. In a project file it equals the project's folder. **[judgement]**

### Conditions, `Update`, `Remove`, targets

- An item or its `ItemGroup` may carry `Condition`; "the items are only added to the item list if the condition is satisfied". **[cited]** `itemgroup-element-msbuild.md:86`. `Choose` is another child of `Project` (`project-element-msbuild.md:31`), whose `When` branches gate the items inside them. **[judgement]**, its page not read. String compares in conditions are case-insensitive (`msbuild-conditions.md:26-27`). Conditions read properties like `$(TargetFramework)`, so their value needs evaluation. **[judgement]**
- `Update` changes metadata of items already included and is not allowed under targets; `Remove` removes items. **[cited]** `msbuild-items.md:186-195`, `:392`. One element carries at most one of `Include`, `Remove` and `Update`. **[cited]** `ProjectParser.cs:279-300`.
- Items inside a `Target` are made at execution, not evaluation. **[cited]** `msbuild-items.md:44-50`. linq2db's `Tests.Wasm.App.csproj` removes `@(_TransitiveProjectReferences)` inside one. **[measured]**

### Metadata that changes the meaning

- `ReferenceOutputAssembly="false"`: "doesn't include the output of the referenced project as a Reference of this project, but still ensures that the other project builds before this one". **[cited]** `common-msbuild-project-items.md:92`. MSBuild sets it to `false` itself for any reference to an `.esproj` (JavaScript/TypeScript project), which "never produce an assembly". **[cited]** `Microsoft.Common.CurrentVersion.targets:1560-1575`.
- `OutputItemType="Analyzer"` with `ReferenceOutputAssembly="false"` is how a project takes another as a source generator or analyzer: orleans' root `Directory.Build.targets:19-44` gives every project its code generator that way. **[measured]**
- `BuildReference="false"` means the reference "will not be built by MSBuild". **[cited]** `common-msbuild-project-items.md:93`.

**What this means.** A text reader can honour a literal relative path, `$(MSBuildThisFileDirectory)` and `$(MSBuildProjectDirectory)` at its start, `;` lists and `%XX` escapes. It cannot honour other properties, item lists, conditions or globs without evaluating MSBuild. Conditions are best ignored: the reference exists in some build of the project, so reading it is an over-count for the other builds, which the doc can state. `ReferenceOutputAssembly="false"` is a build-order edge, not a reference the code compiles against. **[judgement]**

## 2. Paths: backslashes and case

- MSBuild's `FixFilePath` "Fixes backslashes to forward slashes on Unix. This allows to recognise windows style paths on Unix. However, this leads to incorrect path on Linux if backslash was part of the file/directory name." It replaces `\` with `/` whenever the platform's separator is not `\`. **[cited]** `FileUtilities.cs:168-175`. Every item handed to a task has its include passed through it. **[cited]** `ProjectItemInstance.cs:869-870`. Values that look like paths are also adjusted, `\` and runs of slashes collapsed to one `/`. **[cited]** `FileUtilities.cs:716-793` (`MaybeAdjustFilePath`, `CollapseSlashes`).
- So `..\Core\Core.csproj` names `../Core/Core.csproj` on every platform, and a reader should read `\` as `/` always, collapsing repeated separators. **[judgement]**, from the code above.
- Case: "Linux-based file systems are case-sensitive." **[cited]** `customize-by-directory.md:60`. MSBuild checks existence through the file system (`FileSystems.Default.FileExists`, `MSBuild.cs:336` in the intrinsic task), so a wrongly cased reference builds on Windows and macOS and fails on Linux. **[judgement]**, medium confidence.
- Measured: 1,386 of 1,409 references hold a backslash. No reference reached a tracked file only by ignoring case. **[measured]** (§6).

**What this means.** Normalise `\` to `/`, resolve against the project's folder, collapse `.` and `..`, and match tracked paths exactly. A reference that leaves the repository or names no tracked file gives no pair, which the core already enforces. **[judgement]**

## 3. Other places project edges come from

- **Imported files.** `Directory.Build.props` is imported early by `Microsoft.Common.props`, and `Directory.Build.targets` by `Microsoft.Common.targets`, from the nearest folder upward. **[cited]** `customize-by-directory.md:50-56`, `:122`; `Microsoft.Common.props:19-34`. A `ProjectReference` there applies to every project below it, its path relative to each project (§1). Any `<Import Project="…">`, wildcards allowed, does the same for its file. **[cited]** `import-element-msbuild.md:42`, `:74`. Measured: `ProjectReference` items outside project files in orleans (3, root `Directory.Build.targets`), linq2db (7, in five `.props`) and dotnet/fsharp (33, in three `.targets`). **[measured]**
- **`Directory.Packages.props`** holds `<PackageVersion />` items for Central Package Management, and `GlobalPackageReference` adds a package to every project. **[cited]** `Central-Package-Management.md:22-45`, `:259-264`. Packages only; no project edge. **[judgement]**
- **SDK-style and legacy projects.** `<Project Sdk="Microsoft.NET.Sdk">` adds implicit imports at the top and bottom; an `<Sdk Name=…/>` element does the same, and several SDKs can be added. **[cited]** `how-to-use-project-sdk.md:15-75`; `overview.md:46-57`. A legacy project has `xmlns="http://schemas.microsoft.com/developer/msbuild/2003"` and no `Sdk`; its `ProjectReference` has the same `Include` and usually `<Project>{GUID}</Project>` and `<Name>` children (`common-msbuild-project-items.md:88-90`). **[cited]** The `Include` rule is the same in both. **[judgement]** Legacy projects: akka 16, Paket 18, efcore 2, fsharp 1. **[measured]**
- **Shared projects** (`.shproj` with `.projitems`) share source files "between multiple projects". **[cited]** `docs/extensibility/migration/update-visual-studio-extension.md:154`. That a consuming project takes one with `<Import Project="…\X.projitems" Label="Shared" />` is **unverified** from a Microsoft page, high confidence. It is a source include, not an assembly reference, and none of the six repositories has one. **[measured]**
- **Other project kinds.** `.esproj` is the JavaScript Project System's format (`docs/javascript/javascript-project-system-msbuild-reference.md:19`); `.vcxproj`, `.sqlproj` and `.proj` are MSBuild projects too. **[cited]** for `.esproj`. A `ProjectReference` can name any of them; MSBuild special-cases `.vcxproj` and `.nativeproj` (`Microsoft.Common.CurrentVersion.targets:1824`). **[cited]** All 1,409 measured references named `.csproj`, `.fsproj` or `.vbproj`. **[measured]**
- **F# file order.** `<Compile>` order matters inside an `.fsproj`, but it orders files inside one project, so it adds no project edge. **[judgement]**, not checked against an F# page.
- **`.sln` / `.slnx`** list projects for a build and add no reference of their own that this note found. **[judgement]**
- **`<Reference Include="X"><HintPath>..\X\bin\X.dll`** in legacy projects reaches another project's build output with no `ProjectReference`. **[judgement]**, an invisible edge; not counted.

**What this means.** Against the starting point "`.sln`, `Directory.Build.props` and `PackageReference` are not read": `.sln`, `Directory.Packages.props` and `PackageReference` add no project edge, so skipping them loses nothing. `Directory.Build.props`/`.targets` and other imported `.props`/`.targets` do add edges in three of six repositories, and they hold the root properties most `$(…)` paths need (§6). **[judgement]**

## 4. XML a reader must handle

- **Namespace.** "When specified, the `xmlns` attribute must have the value of `http://schemas.microsoft.com/developer/msbuild/2003`." **[cited]** `project-element-msbuild.md:57`. MSBuild accepts that namespace or none, and rejects a prefixed root (`<x:Project>`). **[cited]** `ProjectXmlUtilities.cs:81-91`, `ProjectParser.cs:141-152`. So element names are never prefixed, and a reader can match local names. **[judgement]**
- **Case.** Child elements are matched with a C# `switch` on the name, ordinal: `ItemGroup` must be spelt so, and `itemgroup` is an unrecognised element. **[cited]** `ProjectParser.cs:162-216`. `Include` is read with `GetAttribute`, case-sensitive, and a mis-cased known attribute (`include=`) is rejected "so that mis-capitalizing an attribute like Include or Exclude results in an easy to understand error". **[cited]** `ProjectParser.cs:273`, `:374-381`. The item type is the element's name (`:272`), and item types are stored case-insensitively, so `<projectreference>` joins `@(ProjectReference)`. **[cited]** `ItemDictionary.cs:48`. No project file in the six used another casing. **[measured]**
- **Encoding.** MSBuild reads through a `StreamReader` that detects a byte order mark, defaulting to UTF-8, and lets the XML declaration override it. **[cited]** `XmlReaderExtension.cs:38-64`. DTDs are ignored (`:97`). **[cited]** A UTF-8 BOM is common: 393 of 828 project files. None was UTF-16 or invalid UTF-8, and none failed to parse. **[measured]** `Repo::read` is lossy UTF-8 (`system-map/src/lib.rs:682-688`), so a BOM must be stripped, as `typescript.rs:123-124` does; a UTF-16 file would read as garbage. **[repo]**
- **Comments** may hold whole `<ProjectReference … />` elements; only a real XML parser or a comment-aware lexer skips them. **[judgement]**
- **`quick-xml`** (MIT, 0.42.0, MSRV 1.86, one required dependency, `memchr`). **[cited]** its `Cargo.toml:12-13`, `:21-29`. It reports comments, CDATA and self-closing elements as their own events (`src/events/mod.rs:1988-2000`), gives local names and namespace resolution (`src/name.rs:185`, `:1037`), and removes a UTF-8 BOM even without its `encoding` feature. **[cited]** `src/reader/mod.rs:293-304`. Without `encoding` it assumes UTF-8; with it, UTF-16 still needs its `DecodingReader`. **[cited]** `Cargo.toml:84-142`. `buffer_position()` gives a byte offset, from which a pair's line can be counted. **[cited]** `src/reader/mod.rs:909`. It fits: comments, entities (`&amp;` in a path), CDATA and self-closing elements are what a hand reader gets wrong. It is not yet in this workspace's `Cargo.lock`. **[repo]**

## 5. Test projects

- `IsTestProject` "is set to `true` by the Microsoft.NET.Test.Sdk NuGet package. It signifies whether a project is a VSTest test project so that it's recognized by `dotnet test`." **[cited]** `msbuild-props.md:1681-1686`. The package's props set it unconditionally. **[cited]** vstest `src/package/Microsoft.NET.Test.Sdk/Microsoft.NET.Test.Sdk.props:15-18`. A package's props are imported only after restore, so from text the marker is the `PackageReference` line itself, wherever it is written. **[judgement]**
- `IsTestingPlatformApplication` marks a Microsoft.Testing.Platform test app; "Test framework and platform NuGet packages normally set the property through restored MSBuild imports". `true` is an MTP app whatever `IsTestProject` says; `IsTestProject` `true` alone is a VSTest project. **[cited]** `msbuild-props.md:1559-1577`. xunit.v3 sets it by default (`xunit.v3.core.props:12`); MSTest's adapter sets it to `$(EnableMSTestRunner)` (`MSTest.TestAdapter.targets:12`). **[cited]**
- **MSTest.Sdk.** The docs say "MSTest.Sdk references the Microsoft.NET.Test.Sdk NuGet package" (`msbuild-props.md:1686`). The source adds that reference only when `UseVSTest` is `true` (`MSTest.Sdk/Sdk/VSTest/VSTest.targets:33`), and `UseVSTest` defaults to `false` (`Sdk.targets:9`); the default runner sets `IsTestingPlatformApplication` instead (`Runner/ClassicEngine.targets:16`). **[cited]** Either way `Sdk="MSTest.Sdk"` on the project is a reliable text marker. **[judgement]**
- **By name.** Arcade, the build SDK of dotnet/efcore, dotnet/fsharp and the other dotnet repositories, makes a project a test when `$(MSBuildProjectName)` ends with `.Tests`, `.UnitTests`, `.IntegrationTests` or `.PerformanceTests`, and adds `Microsoft.NET.Test.Sdk` itself. **[cited]** arcade `Tests.props:6-35`. orleans does the same by hand for names ending in `Test` or `Tests`, in `test/Directory.Build.props:20-21`. **[measured]**
- **By hand.** `<IsTestProject>false</IsTestProject>` marks test kits that are not test projects: akka's `Akka.Hosting.TestKit`, orleans' `Orleans.Serialization.TestKit`, efcore's three `*.Specification.Tests`. **[measured]**
- **Paket** puts packages in a `paket.references` file beside the project, not in `PackageReference`: Paket's own two test projects carry `Microsoft.NET.Test.Sdk` only there. **[measured]**

**What this means.** The project file's own markers (`Microsoft.NET.Test.Sdk`, `IsTestProject` `true`, `Sdk="MSTest.Sdk"`) are reliable when present, and absent in most test projects of four of the six repositories (§6). A name rule, the project's file name ending in `.Tests`, `.Test`, `.UnitTests` or `.IntegrationTests`, covers Arcade's and orleans' conventions. An explicit `IsTestProject` `false` should beat the name. **[judgement]**

## 6. What the six repositories hold

| Repository | Licence | Project files (cs/fs/vb) | `ProjectReference` | with `\` | literal, to a tracked file | other `$(…)` | conditional | `ReferenceOutputAssembly=false` | in `.props`/`.targets` |
|---|---|---|---|---|---|---|---|---|---|
| akkadotnet/akka.net | Apache-2.0 | 168 (160/8/0) | 402 | 401 | 401 (1 missing) | 0 | 0 | 6 | 0 |
| linq2db/linq2db | MIT | 88 (83/4/1) | 91 | 74 | 91 | 0 | 4 | 2 | 7 |
| dotnet/efcore | MIT | 60 (58/1/1) | 141 | 141 | 141 | 0 | 0 | 13 | 0 |
| fsprojects/Paket | MIT | 88 (54/34/0) | 25 | 25 | 24 (1 missing) | 0 | 0 | 0 | 0 |
| dotnet/orleans | MIT | 304 (297/5/2) | 592 | 588 | 219 | 373 | 4 | 6 | 3 |
| dotnet/fsharp | MIT | 120 (41/78/1) | 158 | 157 | 75 (6 missing) | 77 | 26 | 9 | 33 |

All **[measured]**. "Literal" counts paths with no property, or with `$(MSBuildThisFileDirectory)` / `$(MSBuildProjectDirectory)` first (10, all in dotnet/fsharp). No reference used `;`, a wildcard, an absolute path or a `%XX` escape. `Update` on a `ProjectReference`: orleans 4 and linq2db 2 in project files, and dotnet/fsharp's root `Directory.Build.targets:15` (`Update="@(ProjectReference)"`); `Remove`: linq2db 2, both of `@(…)` lists inside targets.

- The other `$(…)` are root properties: orleans' `$(SourceRoot)` (328) is `$(MSBuildThisFileDirectory)` in the root `Directory.Build.props:9`, and `$(RepositoryRoot)` (45) is set per project by `GetFullPath('$(MSBuildThisFileDirectory)..\..\…')`. dotnet/fsharp's `$(FSharpSourcesRoot)` (69) is `$(RepoRoot)src`, with `RepoRoot` `$(MSBuildThisFileDirectory)` (`Directory.Build.props:10`, `FSharpBuild.Directory.Build.props:25`). **[measured]**
- Test projects, by the name rule of §5 or a `test`/`tests` path segment, with no marker in the project file or a `paket.references`: akka 7 of 56, linq2db 37 of 37 (4 name an xunit or NUnit package, not a marker), efcore 30 of 33 (the other 3 set `IsTestProject` `false`), orleans 74 of 80, dotnet/fsharp 61 of 61, Paket 3 of 4. Projects with an in-file marker and a plain name: akka 2, orleans 2 (both docs samples). **[measured]**

## 7. A trial repository

**Recommendation: akkadotnet/akka.net (Apache-2.0). [judgement]**
- 168 projects under `src/core` (54), `src/contrib` (51), `src/examples` (43), `src/benchmark` and `src/aot`, with 402 references, every one a literal relative path, all but one with backslashes. So nearly every edge is readable, and a miss points at the reader, not at MSBuild evaluation.
- C# and F# (`Akka.FSharp`, `Akka.Persistence.FSharp`, `Akka.FSharp.Tests` and five F# examples, 8 references to `.fsproj`), 16 legacy projects with the 2003 namespace, 99 files with a BOM.
- Test projects mostly carry `Microsoft.NET.Test.Sdk` in the file (49 of 56 named ones), and two TestKits set `IsTestProject` `false`, so both test rules are exercised.
- One stale reference (`src/examples/AppConfig` names a missing `Akka.Persistence.Sqlite.csproj`), which must give no pair.

**Second run for traps: linq2db/linq2db (MIT).** C#, F# and VB (`Tests.VisualBasic.vbproj`), conditional references, references in imported `.props`, `Update` and in-target `Remove`, and test projects known only by name. orleans is the run that shows what `$(…)` costs if it is not read.

## What this means for the extractor

Rules, all **[judgement]** built on the citations above:

1. **Reads** tracked `.csproj`, `.fsproj` and `.vbproj` files: a part is read only if it owns one. Other project kinds (`.esproj`, `.vcxproj`, `.sqlproj`) are not read; a reference to a tracked one still pairs.
2. **The reader.** Parse each file as XML (`quick-xml`), after a UTF-8 BOM. Match element and attribute names exactly, local names only (`Project`, `ItemGroup`, `ProjectReference`, `Include`), as MSBuild does; also take `ProjectReference` in any case, as item types fold case. Skip comments. A file that does not parse is a Problem naming it.
3. **Which items.** Every `ProjectReference` with `Include` under `Project/ItemGroup` or `Choose/When|Otherwise/ItemGroup`, whatever its conditions (the stated over-count). Not `Update`, not `Remove`, not items inside a `Target`.
4. **The path.** Unescape `%XX`, split on `;`, trim, drop empties. Strip a leading `$(MSBuildThisFileDirectory)` or `$(MSBuildProjectDirectory)`. Turn every `\` into `/`, join to the project file's folder, collapse `.`, `..` and repeated `/`. Pair with that tracked file, matched exactly. A value still holding `$(`, `@(`, `*` or `?` resolves to nothing by itself (rule 7). Each pair carries the line of its `ProjectReference` element.
5. **`ReferenceOutputAssembly="false"`** (as an attribute or child element): either no pair, since it only orders the build, or a pair, since the build depends on it. A grill question.
6. **Tests.** `skips` a project file that holds `<IsTestProject>true</IsTestProject>`, a `Microsoft.NET.Test.Sdk` `PackageReference`, or `Sdk="MSTest.Sdk"` (attribute or `Sdk` element), or whose name ends with `.Tests`, `.Test`, `.UnitTests` or `.IntegrationTests`; unless it holds `<IsTestProject>false</IsTestProject>`. Only the project file is read.
7. **Property paths.** A reference whose path holds a property other than rule 4's is not silent: it is either Unsure, with the tracked project files whose path ends with its literal tail as candidates, or resolved through root properties read from `Directory.Build.props`. A grill question; orleans has 373 of them.
8. **Detection.** Spec #529 says `dotnet-projects` overrides `detect` to test for a tracked, non-ignored project file. The default `detect` (`system-map/src/extract/mod.rs:39-42`) is already that, less skipped files. **[repo]** An override is needed only if a repository of test projects alone should be suggested.
9. **Contract kit.** `MissingConfig` has only `Fallback` and `Problem` (`contract.rs:28-37`) **[repo]**; an extractor with no config file needs a way to say "none", or a `Fallback` naming a file it never needs.

Stated counts:

- **Over-counts.** Conditional references, paired whatever the condition. References with `ReferenceOutputAssembly="false"`, if rule 5 pairs them.
- **Under-counts.** Transitive use in .NET (A uses C through B). References in `Directory.Build.props`/`.targets` and other imported files. Shared projects through `.projitems`. `HintPath` references to another project's build output. Paths built from properties, if rule 7 drops them.
- **Unsure.** If rule 7 takes that branch: a property path whose literal tail matches tracked project files.
- **Exact.** A literal relative path to a tracked project file, backslashes and all: the file MSBuild would build.

## Against the starting points

- *"it reads `.csproj`, `.fsproj` and `.vbproj` `<ProjectReference Include="…">` paths, relative to the project file"*: right for every reference in four of the six repositories, which are all literal. "Relative to the project file" holds even for items in imported files (`msbuild-items.md:40`). In orleans 63% of references use another property, and in dotnet/fsharp 49%.
- *"a part is read only if it owns a project file; two parts inside one project stay unproven"*: right; nothing in MSBuild gives a finer edge without reading C# `using` and namespaces, which `language-extractors.md` rules out.
- *"`.sln`, `Directory.Build.props` and `PackageReference` are not read"*: right for `.sln` and `PackageReference`, which add no project edge. `Directory.Build.props`/`.targets` do hold `ProjectReference` items in orleans and the root properties orleans and dotnet/fsharp paths need (§3, §6).
- *"`quick-xml` may be allowed"*: it fits (§4).
- *"verify: backslash path normalisation"*: MSBuild turns `\` into `/` on Unix (`FileUtilities.cs:168-175`, `ProjectItemInstance.cs:869-870`), and 98% of measured references use `\`. Paths are case-sensitive on Linux; exact matching is right.
- *"verify: test markers `<IsTestProject>` and `Microsoft.NET.Test.Sdk`"*: both are real, but `IsTestProject` is set by the package's props, not usually by hand, and in Arcade and orleans repositories neither appears in the project file: the test status comes from the project's name (§5). `IsTestingPlatformApplication` and `Sdk="MSTest.Sdk"` are the newer markers.

## Open questions for the grill

1. **Property paths.** Read a few root properties from `Directory.Build.props` (those set to `$(MSBuildThisFileDirectory)` plus a literal), make such references Unsure by their literal tail, or leave them silent and accept that orleans' true links fail?
2. **References in imported files.** Keep them a stated under-count, or read `Directory.Build.props`/`.targets` (and only those) for `ProjectReference` items, applying each to every project below it?
3. **`ReferenceOutputAssembly="false"` and analyzers.** Is a build-order or source-generator reference an `import` link in the map, or not a code dependency?
4. **Transitive references.** Leave transitive use as an under-count, or pair A with every project B reaches (an over-count where A doesn't use C)?
5. **Test projects.** Is the name rule (`.Tests`, `.Test`, `.UnitTests`, `.IntegrationTests`) in, beside the in-file markers? Does a `test`/`tests` folder count? Does `paket.references` count?
6. **Conditions.** Pair every conditional reference (the stated over-count), or skip those under a condition?
7. **The trial repository.** akka.net first and linq2db second, or orleans to test property paths?

## After the grill

The grilling of 2026-11-26 settled the open questions and changed these rules; the spec holds the settled design, and `local-evidence/2026-11-26-dotnet-extractor-grilling/reports/decisions.md` (on Daniel's machine) the rounds and the critic's findings.

- **Open questions 1 and 2, `Directory.Build.*`.** Not read, for properties or references. A path holding a property other than `$(MSBuildThisFileDirectory)` or `$(MSBuildProjectDirectory)` makes the project file Unsure, reason `built from a property`, with as candidates the tracked project files whose path ends, as text, with the literal tail after the last `)` (leading `.`/`..` dropped; an empty tail matches every project file), so no property path is silent. References in imported files are a stated under-count. Reading the nearest `Directory.Build.props`/`.targets` is a follow-up, with orleans as its trial.
- **Rule 4, the path.** Split on `;` before decoding `%XX`, as `ItemSpec.cs` splits the still-escaped value, so `%3B` stays inside one name. After stripping a leading `$(MSBuildProjectDirectory)`, one separator is dropped. An absolute path gives nothing.
- **Rule 5, open question 3.** A `ReferenceOutputAssembly="false"` reference pairs like any other; the doc states it as an over-count of compile-time use.
- **Rule 2, a broken file.** quick-xml does not require a root element or closed elements at the end of input, so the extractor checks for one `Project` root itself; a file that fails is a Problem. Ignored project files are not parsed.
- **Rule 6, open question 5.** Name rule: Arcade's four suffixes (`.Tests`, `.UnitTests`, `.IntegrationTests`, `.PerformanceTests`); `.Test` is not Arcade's and is dropped. Folder rule: `test` or `tests` in any case, since linq2db's 37 test projects sit under `Tests/` (`gh api repos/linq2db/linq2db/git/trees/bdbce2d?recursive=1`). **[measured]** `paket.references` is not read.
- **Open question 4, transitive references.** A silent under-count, listed: pairing every reachable project would be a false edge wherever A doesn't use C.
- **Open question 6, conditions.** Every conditional reference pairs, a stated over-count, as Go pairs files of every build tag.
- **Open question 7, trials.** akka.net first, linq2db second; orleans would test the Unsure note, not the reader.
- **Rules 8 and 9.** No `detect` override. `MissingConfig::None` is shared with Java's #604, added by whichever lands first.
