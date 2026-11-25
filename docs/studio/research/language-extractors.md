# Language extractors: which languages the system map should read next

Research notes on which popular languages `system-map` should get an import extractor for after `cargo`, `rust-modules` and `typescript-imports`. For each language: how popular it is, how hard it is to resolve its imports to tracked files without a compiler, and which config the resolver would read. Studio terms follow `docs/studio/CONTEXT.md`.

## How this was read

- Popularity comes from primary sources only: GitHub's Octoverse posts on github.blog and the Stack Overflow survey site survey.stackoverflow.co. Language rules come from each language's own spec or official docs. Pages were read through WebFetch/WebSearch on 2026-11-25. Nothing was installed or run.
- The fetch tool summarises pages, so quotes are as it returned them.
- Each claim is tagged **[cited]** (with its URL), **[repo]** (checked in this repository, with `file:line`) or **[judgement]** (my own reading, not checked against a source or code).

## What an extractor must produce

- An extractor returns `(dependent file, dependency file)` pairs over tracked files. The checker turns them into part-to-part edges and drops pairs inside one part or touching an ignored file. **[repo]** `system-map/src/edges.rs:22` (`type Pairs`), `docs/studio/behaviour/map.md:69`.
- A part no named extractor reads keeps its `import` links as "unproven". The doc names Go and Python as the example. **[repo]** `docs/studio/behaviour/map.md:58-63`, `edges.rs:106-115` (`is_read`).
- Two shapes already exist. **[repo]**
  - A **manifest-level** extractor: `cargo` pairs a crate's `Cargo.toml` with the manifests of the workspace crates it depends on. It runs `cargo metadata`, so it is not toolchain-free. `edges.rs:252-261`.
  - **File-level** extractors: `rust-modules` and `typescript-imports`. The TypeScript reader already reads `.js`, `.mjs`, `.jsx` and `.cjs` as well as the TypeScript suffixes. `edges.rs:842`, `edges.rs:917`.
- A false edge fails the link check, so the bar is no false edges. An extractor should under-count, or over-count only in ways the doc states. **[repo]** spec #475, "Problem Statement" and the barrel decisions.

So **JavaScript is already covered** by `typescript-imports`. **[repo]** `edges.rs:917`. Spec #475 adds workspace packages, barrels, and a scanner that skips comments and strings. **[repo]** `gh issue view 475`.

## Popularity sources

- **Octoverse 2026** ("Octoverse: Commits, pushes, reach new highs in 2026 as developers build with agents", published October 28, 2026, updated November 6, 2026; Octoverse year October 1, 2025 to September 30, 2026). https://github.blog/news-insights/octoverse/octoverse-commits-pushes-reach-new-highs-in-2026-as-developers-build-with-agents/ **[cited]**
  - It ranks only the top three by monthly contributors: "TypeScript first, Python second, and JavaScript third".
  - Its forecast chart then says "C#, Java, Shell, PHP, Go, HCL, and C++ are projected to range from 2.12 million to 0.88 million". That order is a forecast to September 2027, not a ranking.
  - "Rust grew fastest by percentage among the top languages, doubling in the past year."
  - It does not mention Kotlin, Swift, Dart, Ruby or Scala.
- **Octoverse 2025** ("Octoverse: A new developer joins GitHub every second as AI leads TypeScript to #1", published October 28, 2025; Octoverse year September 1, 2024 to August 31, 2025). https://github.blog/news-insights/octoverse/octoverse-a-new-developer-joins-github-every-second-as-ai-leads-typescript-to-1/ **[cited]**
  - It gives a full top 10 by contributors: 1 TypeScript, 2 Python, 3 JavaScript, 4 Java, 5 C#, 6 PHP, 7 Shell, 8 C++, 9 HCL, 10 Go. Ranks 6 to 10 come from the chart's alt text.
  - Its ranking by new repositories: JavaScript, Python, TypeScript, Java, C++, C#.
  - The table below uses the 2025 ranks, since 2026 publishes no full ranking.
- **Stack Overflow Developer Survey 2026**, "Programming, scripting, and markup languages". https://survey.stackoverflow.co/2026/technology/data/languages **[cited]**
  - The 2026 page has **no Professional Developers split**. Its only filters are All Respondents, years of work experience (1-5, 6-15, 16+) and "Used for AI". The percentages below are **All Respondents (n = 14,099)**.
  - The page does have a 16+ years filter, which is the closest stand-in for professional developers. I did not read per-language numbers for it, beyond the search tool's summary (SQL 63.0%, JavaScript 62.0%, HTML/CSS 56.4%, Python 53.2%, Bash/Shell 52%), which is unverified.

## Summary table

Octoverse rank is the 2025 top 10 by contributors, with "—" meaning outside it. SO % is the 2026 survey, all respondents. Difficulty is my judgement, explained in each section.

| Language | Octoverse rank (2025; 2026 note) | SO 2026 % | Difficulty (file-level) | Config to read | Notes |
|---|---|---|---|---|---|
| TypeScript | 1 (1 in 2026) | 43.8 | done | `package.json` workspaces, `pnpm-workspace.yaml` | `typescript-imports`, spec #475 |
| JavaScript | 3 (3 in 2026) | 62.0 | done | as TypeScript | already read by `typescript-imports` (`edges.rs:917`) |
| Python | 2 (2 in 2026) | 58.0 | Moderate | `pyproject.toml`, `setup.cfg`, a `src/` layout | file-exact for relative imports and imports under known roots; dynamic imports are not read |
| Java | 4 (in 2026 forecast) | 27.6 | Moderate | Maven/Gradle source roots (`src/main/java` by convention) | file-exact for single-type imports; same-package use needs no import |
| C# | 5 (in 2026 forecast) | 27.1 | Hard (file); Easy (project) | `.csproj` `ProjectReference`, `.sln` | `using` names a namespace, not a file |
| PHP | 6 (in 2026 forecast) | 17.7 | Moderate | `composer.json` `autoload.psr-4` | `use` of a class is file-exact under PSR-4; same-namespace names need no `use` |
| Shell | 7 (in 2026 forecast) | 51.3 (Bash/Shell) | Hard | none | `source` resolves against `PATH` and the run-time cwd |
| C++ | 8 (in 2026 forecast) | 24.3 | Moderate (`"..."`); Hard (`<...>`) | `compile_commands.json` (a build output) | quoted includes next to the file are exact |
| HCL | 9 (in 2026 forecast) | not listed | Easy (module-level) | `module` `source = "./..."` | Terraform; directory-level edges |
| Go | 10 (in 2026 forecast) | 16.5 | Easy (package-level) | `go.mod`, `go.work` | an import names a directory, not a file |
| C | — | 21.8 | Moderate (`"..."`); Hard (`<...>`) | as C++ | as C++ |
| Rust | — (fastest growth 2026) | 18.0 | done | `Cargo.toml` | `cargo`, `rust-modules` |
| PowerShell | — | 24.6 | Hard | module manifests `.psd1` | not researched in depth |
| Kotlin | — | 10.8 | Hard | Gradle source sets (`build.gradle.kts`, which is code) | a package need not match its folder; imports name declarations |
| Lua | — | 9.4 | Moderate | `package.path` (set at run time) | not researched in depth |
| Swift | — | 5.9 | Hard (file); Moderate (module) | `Package.swift` (Swift code), Xcode projects | `import` names a module; no file imports exist |
| Ruby | — | 5.8 | Hard (Rails); Easy (`require_relative`) | Zeitwerk autoload paths, `$LOAD_PATH` | Rails code has no `require` for app files |
| Dart | — | 4.9 | Easy | `pubspec.yaml` `name` | URIs name files; `package:` maps to `lib/` |
| Scala | — | 2.2 | Hard | sbt (`build.sbt`, Scala code) | like Kotlin, plus wildcard and implicit/given imports |

SQL and HTML/CSS rank high in the survey (58.4%, 58.2%) but have no import graph between repository files worth an extractor. **[cited]** for the numbers, **[judgement]** for the rest.

## Per language

### Python — Moderate

- **Spec.**
  - A regular package is "a directory containing an `__init__.py` file". Namespace packages have no `__init__.py`, and "there may be multiple `parent` directories found during import search". Python language reference §5.2.1, §5.2.2, citing PEP 420. https://docs.python.org/3/reference/import.html **[cited]**
  - Absolute imports are found along `sys.path`, which "is initialized from the `PYTHONPATH` environment variable and various other installation- and implementation-specific defaults". Same page, §5.5.1. **[cited]**
  - Relative imports "use leading dots": one dot is the current package, and each further dot goes up one parent. Same page, §5.7. **[cited]**
  - `importlib.import_module()` and `__import__()` import by a computed name. Same page, §5.1. **[cited]**
- **What to read.** `sys.path` is a run-time value. Statically, the roots are the repository root, `src/` in a src layout, and the folders `pyproject.toml` or `setup.cfg` declare as package roots. **[judgement]**
- **Resolution.**
  - A relative import `from ..a import b` resolves exactly from the file's own folder. **[judgement]**
  - An absolute `a.b.c` resolves to `a/b/c.py` or `a/b/c/__init__.py` under a root.
  - For `from a.b import c`, the extractor checks whether `a/b/c.py` exists, since `c` may be a submodule or a name inside `a/b/__init__.py`.
  - Importing `a.b.c` also runs `a/__init__.py` and `a/b/__init__.py`, so both are real dependencies. **[judgement]**
- **Ambiguity.**
  - Namespace packages can join one name across several roots. **[judgement]**
  - A third-party package can share a name with a local top-level folder. Only a declared root should resolve, as spec #475 decides for workspace packages. **[judgement]**
  - Dynamic imports are not read: an under-count, which is safe.
  - The scanner must skip comments and all four string forms, triple-quoted included. A docstring with "from x import y" in it is common. **[judgement]**
- **Granularity.** File-level, naturally exact for most imports. **[judgement]**

### Go — Easy (package-level)

- **Spec.**
  - "A package path is the module path joined with the subdirectory containing the package (relative to the module root)." Go Modules Reference, "Modules, packages, and versions". https://go.dev/ref/mod **[cited]**
  - A `go.mod` "must contain exactly one `module` directive". Same page, `module` directive. **[cited]**
  - A `replace` whose right side starts with `./` or `../` is "the local file path to the replacement module root directory". Same page, `replace` directive. **[cited]**
  - In a workspace, `go.work`'s `use` "adds a module on disk to the set of main modules". Same page, Workspaces. **[cited]**
- **What to read.** Every tracked `go.mod` (its `module` line, and local `replace` lines) and `go.work`'s `use` list. All are line-based formats. **[judgement]**
- **Resolution.** An import path that starts with a local module's path names a folder. Import strings are plain literals inside `import (...)` blocks, with no relative imports in module mode. A prefix match on the module path is deterministic. **[judgement]**
- **Ambiguity.**
  - An import names a **package (a folder)**, not a file. The edge is file → every non-test `.go` file in that folder. Parts are usually folder-shaped, so the part edge is still exact. **[judgement]**
  - Build tags and `_test.go` files: the extractor can leave out `_test.go` files, as `cargo` leaves out dev-dependencies (`edges.rs:252-253`), and read every file regardless of tags. That over-counts slightly, and the doc would say so. **[judgement]**
- **Granularity.** Package-level (folder). Exact at the part level when a folder belongs to one part. **[judgement]**

### Java — Moderate

- **Spec.**
  - Packages may be stored in a file system, where `jag.scrabble.board` maps to `jag/scrabble/board`. JLS SE 21 §7.2. https://docs.oracle.com/javase/specs/jls/se21/html/jls-7.html **[cited]**
  - The rule that a type lives in `Type.java` is one the host "may choose to enforce". JLS §7.6. **[cited]**
  - There are four import forms: single-type, type-import-on-demand (`*`), single-static and static-on-demand. JLS §7.5. **[cited]**
  - Code "automatically has access to all classes and interfaces declared in its package", with no import. JLS §7.1, Example 7.6-2. **[cited]**
- **What to read.** Source roots. Maven's convention is `src/main/java` (not fetched: unverified). Gradle source sets live in `build.gradle(.kts)`, which is a program. So the reader would use the conventional roots, or index every file's `package` line. **[judgement]**
- **Resolution.** A declaration index is more robust than a path guess: read each `.java` file's `package` line and top-level type names, then resolve `import a.b.C` to the file declaring `C` in `a.b`. **[judgement]**
- **Ambiguity.**
  - `import a.b.*` names a package, so the edge is package-level. **[judgement]**
  - `import a.b.Outer.Inner` names a nested type, so the reader walks up to the outer type. **[judgement]**
  - Same-package use has no import, so those edges are missed. That is mostly harmless, since a package is one folder, usually inside one part.
  - A fully qualified name used inline without an import is also missed: an under-count.
- **Granularity.** File-level for single-type imports, package-level for wildcards. **[judgement]**

### C# — Hard at file level, Easy at project level

- **Spec.**
  - "Namespace names commonly mirror folders, but the compiler uses your declarations, not your folder names, to assign types to namespaces." "The same namespace declaration can appear multiple times in your program." https://learn.microsoft.com/en-us/dotnet/csharp/fundamentals/types/namespaces **[cited]**
  - A `using` directive "imports the types directly in the named namespace". `global using` "applies an import to every source file in the same compilation", and `<ImplicitUsings>` generates more such directives. Same page. **[cited]**
  - "To use types from another library, first add the appropriate project or assembly reference." Same page. **[cited]**
- **What to read.** `.csproj` `<ProjectReference Include="..\X\X.csproj" />` lines, which are XML with literal relative paths. The `ProjectReference` syntax itself was not fetched: unverified, high confidence.
- **Ambiguity.** A `using` names a namespace that many files and projects can add to. Which file a type comes from needs name resolution of every identifier, which is compiler work. **[judgement]**
- **Granularity.** File-level is not practical. Project-level is exact: a `dotnet-projects` extractor pairing `.csproj` files, shaped like `cargo`. **[judgement]**

### PHP — Moderate

- **Spec.**
  - Under PSR-4, a namespace prefix "corresponds to at least one base directory". Sub-namespaces map to subdirectories, and the class name to `ClassName.php`, matching case. https://www.php-fig.org/psr/psr-4/ **[cited]**
  - Composer's `autoload.psr-4` defines "a mapping from namespaces to paths, relative to the package root". It also has `classmap`, `files` and the root-only `autoload-dev`. https://getcomposer.org/doc/04-schema.md **[cited]**
- **What to read.** Every tracked `composer.json`: `autoload.psr-4` and `autoload-dev.psr-4`. **[judgement]**
- **Resolution.** A `use A\B\C;` resolves to a file through the longest matching prefix. **[judgement]**
- **Ambiguity.**
  - A class in the same namespace, or a fully qualified `\A\B\C` inline, needs no `use`. Reading those means reading identifiers in code, so they are under-counted. **[judgement]**
  - `classmap` needs a scan of class declarations. **[judgement]**
  - `require __DIR__ . '/x.php'` is an expression. Its common literal form can be read, and the rest skipped. **[judgement]**
- **Granularity.** File-level, exact for `use` under PSR-4. **[judgement]**

### C and C++ — Moderate for quoted includes, Hard in full

- **Spec.**
  - `#include "file"` is searched "first relative to the directory of the current file", then the `-I` directories and the system directories. `<file>` searches the system directories (plus `-I`). GCC CPP manual, "Search Path". https://gcc.gnu.org/onlinedocs/cpp/Search-Path.html **[cited]**
  - The real include path comes from compiler flags. `compile_commands.json` records them per file (`directory`, `file`, `arguments` or `command`). CMake writes it when `CMAKE_EXPORT_COMPILE_COMMANDS` is set. https://clang.llvm.org/docs/JSONCompilationDatabase.html **[cited]**
- **What to read.** `compile_commands.json` is a build output, usually untracked, and it needs a configure step, which goes against "no compiler". **[judgement]**
- **Resolution.** Without that file, a quoted include that resolves next to the including file is exact. **[judgement]**
- **Ambiguity.** Everything else needs guessed include directories (`include/`, `src/`), and two headers with the same relative name make it ambiguous. `#if` blocks and macro-built includes (`#include MACRO`) cannot be decided statically. **[judgement]**
- **Granularity.** File-level, header to header. Exact only for the quoted-and-adjacent form. **[judgement]**

### Kotlin — Hard

- **Spec.** The folder-follows-package layout is a recommendation, "the recommended directory structure follows the package structure with the common root package omitted", not a rule. Kotlin coding conventions, "Directory structure". https://kotlinlang.org/docs/coding-conventions.html **[cited]**
- **Ambiguity.**
  - Imports name declarations, including top-level functions and properties, not just classes. Resolving them needs an index of every file's `package` line and top-level declarations. **[judgement]**
  - Same-package use has no import, as in Java. **[judgement]**
  - Source sets come from Gradle scripts, which are Kotlin code. **[judgement]**
- **Granularity.** File-level is possible through a declaration index. A Java and Kotlin extractor could share that index. **[judgement]**

### Swift — Hard at file level

- **Spec.** SwiftPM expects a target's sources at `[PackageRoot]/Sources/[TargetName]` unless the target sets `path`. PackageDescription docs, as surfaced by search; the page itself was not opened, so this is unverified. https://docs.swift.org/package-manager/PackageDescription/PackageDescription.html
- **What `import` names.** A module (a target), not a file. Inside a module every `internal` declaration is visible with no import. The access-control page did not load, so this is unverified, high confidence.
- **What to read.** `Package.swift` is Swift code. Its common literal form (`.target(name:dependencies:)`) can be scanned, but not every form. Xcode `.pbxproj` projects are another format. **[judgement]**
- **Granularity.** Module-level only. **[judgement]**

### Ruby — Hard for Rails, Easy for `require_relative`

- **Spec.**
  - `require_relative` loads "relative to the directory containing the requiring file". `require` searches `$LOAD_PATH` for names that do not start with `./` or `../`. https://docs.ruby-lang.org/en/master/Kernel.html **[cited]**
  - Under Zeitwerk in Rails, "file names have to match the constants they define, with directories acting as namespaces". For application files "you do not need (and should not write) a `require` call". https://guides.rubyonrails.org/autoloading_and_reloading_constants.html **[cited]**
- **Ambiguity.** In a Rails app the dependencies are constant references (`User`, `Admin::PaymentsController`), with no import statement to read. A Zeitwerk reader would map constant names to files, an identifier-level scan. **[judgement]**
- **Granularity.** File-level exact for `require_relative`. Guessable for `require 'gem/sub'` against `lib/`. Hard for Rails. **[judgement]**

### Dart — Easy

- **Spec.** `import 'package:enchilada/some/path/olives.dart'` maps to `lib/some/path/olives.dart` of the package named `enchilada` in its `pubspec.yaml`. Relative imports are preferred within a package. `.dart_tool/package_config.json` is written by `dart pub get` and "should never be checked into source control". https://dart.dev/tools/pub/package-layout **[cited]**
- **What to read.** Tracked `pubspec.yaml` `name` fields, not `package_config.json`. **[judgement]**
- **Ambiguity.**
  - Every import is a URI naming a file. `part` and `part of` are file URIs too.
  - Conditional imports (`if (dart.library.io) '...'`) name several files. The reader can read them all, an over-count the doc would state. **[judgement]**
- **Granularity.** File-level, exact. The fit is excellent but the popularity is low (4.9%). **[judgement]**

### Scala — Hard

Scala is like Kotlin: package declarations independent of folders, wildcard and given/implicit imports, and an `sbt` build that is Scala code. At 2.2% in the survey it is not worth it now. **[judgement]**, survey number **[cited]**.

### Shell (Bash) — Hard

- **Spec.** "If filename does not contain a slash, `.` searches for it" in `PATH`. Outside POSIX mode "it searches the current directory if filename is not found in `$PATH`". A path with a slash is relative to the run-time current directory. Bash manual, Bourne Shell Builtins. https://www.gnu.org/software/bash/manual/html_node/Bourne-Shell-Builtins.html **[cited]**
- **Ambiguity.** `source lib.sh` therefore does not resolve against the script's own folder. Scripts that mean "next to me" write `"$(dirname "$0")/lib.sh"` or `"${BASH_SOURCE%/*}/lib.sh"`. A reader could recognise those idioms, but they are expressions. Most links between scripts are run-time `spawn` links (one script calls another), which the map already models as run-time links. **[judgement]**
- **Granularity.** File-level only for the recognised idioms. Popular (51.3%), but little import graph to read. **[judgement]**

### HCL (Terraform) — Easy, module-level

Octoverse 2025 ranks HCL 9th. **[cited]** A `module` block's `source = "./modules/x"` is a literal local path naming a folder. That makes a cheap, exact folder-level extractor. The Terraform docs were not fetched, so the syntax is unverified, high confidence. HCL is configuration more than application code, which lowers its value for plotkeeper's map. **[judgement]**

## Recommendation

Ranked order for the next extractors, weighing popularity, difficulty, and fit with the design: no false edges, deterministic, no toolchain, the same scanner discipline as spec #475. All of the ranking is **[judgement]**; the popularity figures it rests on are **[cited]** above.

1. **`python-imports`.** Python is #2 on GitHub in both Octoverse editions and at 58.0% in the survey. The doc already names Python as unread (`map.md:58-63`, **[repo]**). Relative imports and imports under declared roots resolve to exact files. The work reuses #475's patterns: a comment- and string-aware scanner, and only declared roots resolve, as only declared workspace packages do. The risk is choosing roots; reading `pyproject.toml` and the src layout keeps it deterministic.
2. **`go-imports`.** It is the easiest of the popular languages. `go.mod` and `go.work` are line formats, import strings are literals, and a module-path prefix match has no ambiguity. Go is in the Octoverse top 10 and at 16.5% in the survey, and the doc names it as unread too. Edges are folder-level, but parts are folder-shaped, so part edges are exact.
3. **`java-imports`, built on a package-declaration index.** Java is #4 on GitHub and at 27.6% in the survey, with a large professional base. Single-type imports resolve to exact files through an index of `package` lines and type names. Missing same-package edges stay inside one folder. The same index is the base a later Kotlin reader needs, so this step also prepares Kotlin (10.8%).
4. **`dotnet-projects` (C# at the project level), or C/C++ quoted includes.**
   - C# is #5 on GitHub and at 27.1% in the survey, but its file-level edges are out of reach without a compiler. Its `.csproj` `ProjectReference` paths give exact project-to-project edges, a manifest extractor like `cargo`, cheap and with no false edges.
   - C/C++ (24.3% and 21.8%) quoted includes next to the file are also exact, but the rest of C/C++ needs `compile_commands.json`, a build output, so its coverage would be partial.
   - I would pick `dotnet-projects` first for its exactness.

Not recommended now:

- **PHP** (Moderate, 17.7%): a reasonable fifth, as `composer.json` PSR-4 makes `use` exact.
- **Kotlin, Swift, Scala**: no file-level imports to read, or only module-level ones.
- **Ruby/Rails**: dependencies are constant references with no import statements.
- **Shell**: `source` resolves against `PATH` and the run-time cwd, and most of its links are `spawn` links.
- **Dart**: easy and exact, but at 4.9% it is unpopular.
- **HCL**: easy, but it is configuration more than application code.
