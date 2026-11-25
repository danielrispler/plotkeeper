# Go extractor: the rules a text-only `go-imports` must follow

Research notes for issue #537. They set out how a text-only `go-imports` extractor turns Go imports into `(dependent file, dependency file)` pairs over a repository's tracked files, without running `go`. They extend the "Go" section of `language-extractors.md` and don't repeat it. Studio terms follow `docs/studio/CONTEXT.md`. The extractor contract is `docs/studio/behaviour/map.md`, "The extractors".

## How this was read

- Language and tool rules come from primary sources only. These are the Go Modules Reference (https://go.dev/ref/mod), the Go spec (https://go.dev/ref/spec), the `go` command's help (https://pkg.go.dev/cmd/go), and the Go toolchain's own source on GitHub where the docs say nothing. Pages were read through WebFetch on 2026-11-25. The fetch tool summarises pages, so quotes are as it returned them.
- Source was read at `golang/go` and `golang/mod` `master` on 2026-11-25, through raw.githubusercontent.com. Line numbers are for that day's `master` and will drift. The files were:
  - `src/cmd/go/internal/modload/import.go` and `init.go`
  - `src/cmd/go/internal/load/pkg.go`
  - `src/cmd/go/internal/imports/read.go` and `build.go`
  - `src/cmd/go/internal/search/search.go`
  - `src/go/build/build.go`
  - `golang/mod` `modfile/read.go` and `modfile/rule.go`
- The candidate repositories were read through `gh api` (repository metadata, the git tree, single files). Nothing was cloned, installed or run.
- Each claim is tagged **[cited]** (with its URL and section, or a source file and line), **[repo]** (checked in this repository, with `file:line`) or **[judgement]** (my own reading, not checked against a source or code).

## 1. `go.mod` and `go.work`

### The `module` directive

- Grammar: `ModuleDirective = "module" ( ModulePath | "(" newline ModulePath newline ")" ) newline .`. So the path may also sit in a one-line block. **[cited]** go.dev/ref/mod, "`module` directive".
- "A `go.mod` file must contain exactly one `module` directive." **[cited]** same section.
- "Comments start with `//` and run to the end of a line. `/* */` comments are not allowed." **[cited]** go.dev/ref/mod, "Lexical elements".
- There are two string forms: interpreted (`"..."`) and raw (`` `...` ``). "Identifiers and strings are interchangeable in the `go.mod` grammar." So `module example.com/m`, `module "example.com/m"` and ``module `example.com/m` `` are the same. **[cited]** same section.
- A `// Deprecated:` comment may sit before the `module` line or after it on the same line. **[cited]** "`module` directive". Stripping `//` comments handles it. **[judgement]**
- The go command's own tolerant reader, `modfile.ModulePath`, works line by line. It cuts each line at `//`, trims it, and takes the first line that starts with `module` followed by whitespace. It unquotes a `"` or `` ` `` value and returns the rest of the line otherwise. It does not read the block form. **[cited]** `golang/mod` `modfile/read.go:930-965`. A text extractor can copy this and add the block form. **[judgement]**

### `replace` with local paths

- Grammar: `ReplaceSpec = ModulePath [ Version ] "=>" FilePath newline | ModulePath [ Version ] "=>" ModulePath Version newline .`. Blocks `replace ( … )` are allowed. **[cited]** go.dev/ref/mod, "`replace` directive".
- The left side may carry a version or not. "If the left version is omitted, all versions of the module are replaced." **[cited]** same section. For a text extractor the left version does not matter. **[judgement]**
- The right side is a local path when it is "an absolute or relative path (beginning with `./` or `../`)". It "must contain a `go.mod` file", and "The replacement version must be omitted in this case." **[cited]** same section.
- The parser counts more as a directory path than the docs list: `.`, `./…`, `..`, `../…`, `.\…`, `..\…`, `/…`, `\…`, and a drive letter `C:…`. **[cited]** `modfile/rule.go:770-779` (`IsDirectoryPath`). So `replace golang.org/x/tools => ..` (a bare `..`) is a local replace. golang/tools' `gopls/go.mod` has exactly that line. **[cited]** `gh api repos/golang/tools/contents/gopls/go.mod`, line 40.
- "if the replacement module has a `go.mod` file, its `module` directive must match the module path it replaces." **[cited]** "`replace` directive".
- "`replace` directives only apply in the main module's `go.mod` file and are ignored in other modules." **[cited]** same section.
- "a `replace` directive alone does not add a module to the module graph. A `require` directive that refers to a replaced module version is also needed." **[cited]** same section.

**What this means.** The target folder's own `module` line must match a local replace's left side. So a local replace adds no path→folder mapping that the target's `go.mod` doesn't already give. It only says the go command *uses* the local copy. **[judgement]** An absolute right side points at a folder on one machine and is not a tracked path, so the extractor ignores it. **[judgement]**

### `go.work`

- "Lexical elements in `go.work` files are defined in exactly the same way as for `go.mod` files." Blocks work as in `go.mod`. **[cited]** go.dev/ref/mod, "`go.work` files".
- `UseDirective = "use" ( UseSpec | "(" newline { UseSpec } ")" newline ) .`, and `UseSpec = FilePath newline .`. **[cited]** "`use` directive".
- "Its argument is a relative path to the directory containing the module's `go.mod` file. A `use` directive does not add modules contained in subdirectories of its argument directory." There are no patterns: `use ./...` does not exist. Each nested module needs its own `use` line. **[cited]** same section.
- `go.work` does have `replace`, with the same grammar as `go.mod`, local paths included. "`replace` directives in `go.work` files override any replaces of the same module or module version in workspace modules." **[cited]** "`replace` directive" under `go.work` files.
- The go command finds `go.work` from the current directory. "If `GOWORK` is set to `off`, the command will be in a single-module context. If it is empty or not provided, the command will search the current working directory, and then successive parent directories, for a file `go.work`." If none is found, "the workspace will include only the module containing the working directory". **[cited]** "Workspaces".

**What this means.** What the go command treats as local depends on the directory it runs in. A cwd-free extractor cannot copy that. `use` lines, like local replaces, add no mapping that the used folder's `go.mod` lacks. **[judgement]**

### Two modules with one path

- Within one workspace this is an error: "go: module %s appears multiple times in workspace". **[cited]** `modload/init.go:1398-1401`. A `use` of the same folder twice is also an error ("path %s appears multiple times in workspace", `init.go:852`). **[cited]**
- Two `go.mod` files that are never in one build (for example a fork copy, or a fixture under `testdata/`) are legal. golang/tools tracks `go.mod` files under `testdata/` (`go/analysis/passes/modernize/testdata/importcommentmod/go.mod`, `internal/exportdata/testdata/golden/src/go.mod`). **[cited]** `gh api repos/golang/tools/git/trees/HEAD?recursive=1`.

### Nested modules

- "Module zip files do not include the contents of `vendor` directories or any nested modules (subdirectories that contain `go.mod` files)." "Files within directories containing `go.mod` files, other than the module root directory, are ignored … since they are not part of the module." **[cited]** go.dev/ref/mod, "Module directories within a repository" and "File path and size constraints".
- Resolution has the same rule for local modules. `dirInModule` walks from the package folder up to the module root. If any folder on the way, the package folder included, holds a `go.mod`, the module does not provide the package. **[cited]** `modload/import.go:735-757`.

### The `ignore` directive

`go.mod`'s `ignore` makes the go command skip folders "when matching package patterns". **[cited]** go.dev/ref/mod, "`ignore` directive". It is about patterns like `./...`, not imports. The extractor can leave it alone. **[judgement]**

## 2. Import declarations

- `SourceFile = PackageClause ";" { ImportDecl ";" } { TopLevelDecl ";" } .`. Imports come only after the package clause and before any other declaration. **[cited]** go.dev/ref/spec, "Source file organization". So a reader can stop at the first token after the imports. The go command's own `imports.ReadImports` does exactly this: it "stops reading the input once the imports have completed". **[cited]** `imports/read.go:227-245`.
- `ImportDecl = "import" ( ImportSpec | "(" { ImportSpec ";" } ")" ) .`, `ImportSpec = [ "." | PackageName ] ImportPath .`, `ImportPath = string_lit .`. **[cited]** go.dev/ref/spec, "Import declarations".
  - Single form: `import "p"`. Grouped form: `import ( "p"; "q" )`. Several `import` declarations may follow one another.
  - Named `x "p"`, dot `. "p"` and blank `_ "p"` all name the same package. The blank form imports "solely for its side-effects". **[cited]** same section. For the edge, all four forms count the same. **[judgement]**
- `string_lit = raw_string_lit | interpreted_string_lit`. So ``import `p` `` is legal Go. **[cited]** go.dev/ref/spec, "String literals".
- Import paths are restricted to "non-empty strings using only characters belonging to Unicode's L, M, N, P, and S general categories". The characters ``!"#$%&'()*,:;<=>?[\]^`{|}`` and U+FFFD may be excluded too. **[cited]** "Import declarations". So a real import path holds no space, newline or backslash. A path with a backslash escape (`"\x61"`) can be skipped: an under-count no real code hits. **[judgement]**
- Semicolons are inserted after a line's final token when it is a literal or `)`, among others. **[cited]** go.dev/ref/spec, "Semicolons". So `import "a"; import "b"` on one line and newline-separated specs are the same.
- `import "C"` is cgo's pseudo-package. go/build treats a file importing `"C"` as a cgo file, and a test file using it is an error. **[cited]** `go/build/build.go:989-1001`. "C" names no folder, so the extractor skips it. **[judgement]**

### Lexing traps

- "A comment cannot start inside a rune or string literal, or inside a comment." "A general comment containing no newlines acts like a space. Any other comment acts like a newline." **[cited]** go.dev/ref/spec, "Comments".
- Only comments can appear before or between imports. The package clause and the import specs hold no rune literals and no code strings. So the traps are:
  - `/* import "x" */` and `// import "x"`.
  - The cgo preamble: a comment right before `import "C"`, full of C code with `#include "..."` lines.
  - The `//go:build` line, and doc comments that say "import".
  - A raw string containing `import`, or `import` in later code. Neither can matter, since the reader stops at the first declaration that isn't an import. **[judgement]**
- An old-style import comment, `package x // import "example.com/x"`, is a line comment after the package clause. It is not an import. go/build reads it only for a special mode. **[cited]** `go/build/build.go:967-980` (`findImportComment`). Skipping comments handles it. **[judgement]**

## 3. Which files of a folder form the package

- "A set of files sharing the same PackageName form the implementation of a package. An implementation may require that all source files for a package inhabit the same directory." **[cited]** go.dev/ref/spec, "Package clause". The go command does require it: a package path is "the module path joined with the subdirectory containing the package". **[cited]** go.dev/ref/mod, "Modules, packages, and versions".
- go/build skips any file whose name starts with `_` or `.`, before it reads the file. **[cited]** `go/build/build.go:1450-1453` (`matchFile`). `go help test` says the same: "Files whose names begin with "_" (including "_test.go") or "." are ignored." **[cited]** pkg.go.dev/cmd/go, "Test packages".
- A file is a test file when its name ends in `_test.go`. A test file whose package name ends in `_test` is an external test package. **[cited]** `go/build/build.go:948-952`, and `go help test`: "Test files that declare a package with the suffix "_test" will be compiled as a separate package". **[cited]**
- A file with `package documentation` is ignored. **[cited]** `go/build/build.go:939-944`.
- File-name build constraints: `name_GOOS`, `name_GOARCH` and `name_GOOS_GOARCH`, each optionally followed by `_test`. They apply only to the part after the first `_`, so `linux.go` is not constrained but `dns_linux.go` is. **[cited]** `go/build/build.go:1981-2027` (`goodOSArchFile`).
- `//go:build` lines "must appear near the top of the file, preceded only by blank lines and other comments", and "before the package clause". "It is an error for a file to have more than one //go:build line." `//go:build ignore` excludes a file, and "ignore" is conventional for it. **[cited]** pkg.go.dev/cmd/go, "Build constraints". If no `//go:build` line is present, old `// +build` lines still count. **[cited]** `imports/build.go:79-80`.
- The go command already has an "all tags" mode. `go mod tidy` "acts as if all build tags are enabled … There is one exception: the `ignore` build tag is not enabled, so a file with the build constraint `// +build ignore` will not be considered." **[cited]** go.dev/ref/mod, "`go mod tidy`". The code treats every tag except `ignore` as both true and false. **[cited]** `imports/build.go:184-202`.
- Directories: `./...` patterns skip "Directories named "vendor"", "Directories named "testdata"", and "Files and directories with names beginning with "_" or "."". **[cited]** pkg.go.dev/cmd/go, "Package lists and patterns", and `search/search.go:152-154`. That rule is for *patterns* only. `go mod tidy` will not consider such folders "unless those packages are explicitly imported by other packages". So a `testdata` or `_x` package can still be imported by name. **[cited]** go.dev/ref/mod, "`go mod tidy`".
- Non-Go files (`.c`, `.h`, `.s`, `.S`, `.syso` and others) join the package by extension and build constraint. `.c` and `.h` are compiled when "the package uses cgo or SWIG". **[cited]** `go/build/build.go:1080-1110`, pkg.go.dev/cmd/go, "File types". cgo files are dropped when cgo is disabled. **[cited]** `go/build/build.go:1007-1020`.

**Cheap to honour, by reading text only. [judgement]**
- Names starting with `_` or `.`: a file-name check.
- `_test.go`: a suffix check.
- `//go:build ignore`, and `// +build ignore` when there is no `//go:build` line: read the header comments before `package`, which the import reader reads anyway. Honour only an expression that is exactly `ignore`. Then every other expression counts as true, as `go mod tidy` does.
- `package documentation`: the package clause the reader already reads.

**Over-counted on purpose. [judgement]**
- Files with an OS or architecture in their name, and files with other `//go:build` expressions. All of them are counted, like `go mod tidy`. A `foo_windows.go` imports what Windows builds need, and the pair is real for some build.
- cgo files, counted whether or not cgo is on.

**Left out, an under-count. [judgement]**
- Non-Go files (`.c`, `.s`, …) as dependency ends. An importer uses the Go API. A pair to a `.c` file in the same folder would add no part edge, unless a part owns only the C file.

## 4. `internal/` and `main` packages

- `internal/`: "Code in or below a directory named "internal" is importable only by code" in the tree rooted at the parent of `internal`. **[cited]** pkg.go.dev/cmd/go, "Internal Directories" (the fetch tool returned the sentence cut off; the tail is paraphrased, high confidence). Breaking that rule is an error ("use of internal package %s not allowed"). **[cited]** `load/pkg.go:1513-1577`. It limits who may import a package. It does not change which folder an import path names. So the extractor resolves as usual and doesn't check the rule. **[judgement]**
- `main`: a program is "a single, unimported package called the main package". **[cited]** go.dev/ref/spec, "Program execution". Importing a `main` package from another folder is an error ("import %q is a program, not an importable package"). Tests in the same folder may be `package main`. **[cited]** `load/pkg.go:797-803`. Resolution doesn't change. Such an import only appears in code that does not build. **[judgement]**

## 5. Relative imports, GOPATH, and a missing `go.mod`

- Module mode rejects relative imports: "%q is relative, but relative import paths are not supported in module mode". A local import is `.`, `..`, `./…` or `../…`. **[cited]** `modload/import.go:281-282`, `go/build/build.go:2033-2037` (`IsLocalImport`). An absolute path is also rejected (`import.go:284-285`). **[cited]** The extractor skips them all. **[judgement]**
- "As of Go 1.16, module-aware mode is enabled by default, regardless of whether a `go.mod` file is present." GOPATH mode needs `GO111MODULE=off`. **[cited]** go.dev/ref/mod, "Module-aware commands". Without a `go.mod`, "Packages from other modules cannot be built". **[cited]** go.dev/ref/mod, "Module commands outside a module".
- So a repository with no `go.mod` is a GOPATH-era repository. Its import paths are relative to a `GOPATH/src` that the repository doesn't record. The extractor resolves nothing in it, and detection should not suggest `go-imports` for it. A `.go` file outside every tracked module produces no pairs as a dependent. **[judgement]**
- The standard library "reserves all import paths without a dot in the first element for its packages". **[cited]** pkg.go.dev/cmd/go, "Import path syntax". But a main module's own path may lack a dot (`module myapp`). The dot rule binds only module paths that are downloaded. **[cited]** go.dev/ref/mod, "Module paths": the dot rule is stated for the leading path element. Its exact scope is **[judgement]**, high confidence. So the extractor matches imports against local module paths directly, with no dot test. **[judgement]**

## 6. What is test-only

- Only `*_test.go` files are test files. That covers both in-package tests and external `package x_test` files. **[cited]** `go/build/build.go:948`, `go help test`.
- `testdata/` is not compiled by patterns. "The go tool will ignore a directory named "testdata", making it available to hold ancillary data". **[cited]** `go help test`. It is fixture data, not test code. Its `.go` files are often deliberately broken or fake. golang/tools has 502 `.go` files under `testdata/`. **[cited]** `gh api` tree count.
- A file named `_test.go` starts with `_`, so it is ignored, not a test. **[cited]** `go help test`.
- Nothing else is test-only to the go command. **[judgement]**, from reading `go/build`'s file loop (`build.go:895-1030`).

## 7. Several local modules matching one import

The go command does **not** pick the longest prefix when several local modules could hold an import. **[cited]** `modload/import.go:401-460`.
1. For each prefix of the import path, it checks whether a module in the build list has that path.
2. For each such module it asks `dirInModule` whether the module *provides* the package. Providing means three things. The folder `root + rest of path` exists. No `go.mod` lies between that folder and the module root, the folder included. The folder holds at least one `.go` file, whatever its build constraints ("We don't care about build tags, not even "go:build ignore"", `import.go:765`).
3. If two or more modules provide it, the result is `AmbiguousImportError` ("ambiguous import: found package %s in multiple modules", `import.go:131`). The Modules Reference says the same: "If exactly one module in the build list provides the package, that module is used. If no modules provide the package or if two or more modules provide the package, the `go` command reports an error." **[cited]** go.dev/ref/mod, "Resolving a package to a module".

"The module with the longest path is used" appears only for the *proxy* lookup of a missing module. **[cited]** same section. It is not the rule for modules already in the build.

Take `example.com/a` at the root and `example.com/a/b` at `b/`. For an import of `example.com/a/b/c`, the outer module would look in `b/c`, but `b/go.mod` is in the way. So only the inner module provides it, and the result matches longest-prefix. **[judgement]** The two rules differ when a module's path doesn't follow its folder. Take `example.com/a/b` declared in `libs/b/`, while the root module also has a `b/c/` folder with `.go` files. Go reports an ambiguous import. Longest-prefix would pick `libs/b/c`. **[judgement]**

A major version suffix also breaks folder-based guessing. etcd's root module is `go.etcd.io/etcd/v3`, and `server/` holds `go.etcd.io/etcd/server/v3`. The root path is not a prefix of the server's. **[cited]** `gh api repos/etcd-io/etcd/contents/go.mod` and `server/go.mod`, line 1. So matching must use declared module paths, never folder names. **[judgement]**

## 8. A trial repository

| Repository | Licence | Tracked files / `.go` / `_test.go` | Modules | Fit |
|---|---|---|---|---|
| etcd-io/etcd | Apache-2.0 | 1518 / 1107 / 420 | 14 `go.mod`, a root `go.work` using all 14 | recommended |
| golang/tools | BSD-3-Clause | 2944 / 1974 / 520 | root + nested `gopls/` (`replace golang.org/x/tools => ..`), two `go.mod` under `testdata/`, no `go.work` | a good second run for traps |
| prometheus/prometheus | Apache-2.0 | 1776 / 768 / — | `go.work` with 5 modules, most of the code in one | fewer cross-module edges |
| hashicorp/consul | not SPDX (`NOASSERTION`, BUSL) | 11549 / 2479 / — | 16 `go.mod`, no `go.work` | not permissive; rejected |

All figures come from `gh api repos/<r>` and `repos/<r>/git/trees/HEAD?recursive=1` on 2026-11-25. All four were pushed within the last two days. **[cited]**

**Recommendation: etcd-io/etcd. [judgement]**
- It is medium-sized: about 690 non-test `.go` files.
- It has 14 modules under one `go.work`. Each `go.mod` also has local `replace` lines (`=> ./api` at the root, `=> ../api` in `server/`). So the module index, nested modules (`client/pkg` and `client/v3` under `client/`, `tests/antithesis/pkg` under `tests/`) and cross-module imports all get exercised.
- Its `/v3` suffixes test that matching uses module paths, not folders.
- It has 28 OS- or arch-suffixed files, so the "count every tag" choice shows up. It has no `vendor/` and no `testdata/` `.go` files, so a first run is not swamped by fixtures.
- golang/tools is the better second run. It exercises skipping `testdata/` (502 files of deliberately odd Go), `go.mod` files under `testdata/`, and a bare `..` replace.

## What this means for the extractor

Rules, all **[judgement]** built on the citations above:

1. **Module index.** Read every tracked `go.mod`, except those with a path element named `vendor` or `testdata`, or starting with `_` or `.`. Take its `module` path: one line, or the one-line block form. Cut each line at `//`, unquote `"…"` and `` `…` ``. The folder holding the `go.mod` is the module root. A `go.mod` without a readable `module` line is a problem: "must contain exactly one `module` directive" (go.dev/ref/mod), and the extractor contract requires a malformed config to give at least one (`system-map/src/extract/contract.rs`, `Fixture.malformed_config`). Corrected after the grill of 2026-11-25.
2. **`replace` and `use` add no mappings.** A local replace's target and a `use` target must carry their own `go.mod`, whose `module` line already maps the path. A `go.work` and its local replaces therefore change which local copy the go command would pick, not where a path lives. The extractor reads every tracked module, with no cwd. It does not need to parse `replace` or `use` for resolution. Read them only if the spec wants to warn about a target that is missing or untracked.
3. **One path, two modules.** If two indexed `go.mod` files declare the same path, an import under that path is Unsure: it yields no pair. (The go command errors when both sit in one workspace. Outside one, the choice depends on cwd and `replace`.)
4. **Every `go.mod` counts as a boundary.** When the extractor checks whether a module provides a folder, every tracked `go.mod` is a wall, including ones left out of the index under rule 1. A `go.mod` in the package folder, or in any folder between it and the module root, means that module doesn't provide it.
5. **Resolution, as the go command does it.** For an import path `p`, find every indexed module whose path `m` equals `p` or prefixes it at a `/`. That module's candidate folder is `root/<p minus m>`. It provides `p` when no `go.mod` is in the way (rule 4) and the folder holds at least one tracked `.go` file. With exactly one provider, `p` resolves to that folder. With none, there is no pair. That covers the standard library, third-party code and missing folders. With two or more, Go reports an ambiguous import, so there is no pair. This is not longest-prefix (§7).
6. **Never resolve these.** `"C"`. Relative and absolute paths (`.`, `..`, `./…`, `../…`, `/…`). Paths with an element named `vendor`: Go requires "x/vendor/y must be imported as y" (`load/pkg.go:1616-1620`). Paths holding a backslash or a space. `vendor/` trees never resolve as dependency targets, and their files are never dependents.
7. **Dependent files.** These are tracked `.go` files inside an indexed module that are not test files and not skipped. Skip files whose name starts with `_` or `.`. Skip files under a folder (relative to the module root) named `testdata` or `vendor`, or starting with `_` or `.`. Skip files with an exact `//go:build ignore` (or `// +build ignore` with no `//go:build` line), and files whose package clause is `documentation`.
8. **Dependency files.** An import resolving to folder `D` pairs the dependent with every `.go` file directly in `D` that rule 7 would keep: not `_test.go`, not `_`/`.`, not `ignore`d, not `documentation`. Subfolders are never included. Explicit imports of `testdata`, `_x` or `.x` folders do resolve, since Go allows them, but files inside them still aren't dependents. That is an under-count for such rare packages.
   *Corrected by spec #569 (R2):* imports into `testdata/`, `_x/` and `.x/` packages give no pair, since their files are never package files to the map. It is a stated under-count in `go-imports`' section of `docs/studio/behaviour/map.md`.
9. **The reader.** Skip a UTF-8 BOM, whitespace, `//` and `/* */` comments. Read `package <ident>`. Then, while the next token is `import`, read one spec or a parenthesised group. A spec is an optional identifier, `.` or `_`, then an interpreted or raw string, ended by `;`, a newline or `)`. Stop at the first other token. Nothing after the imports is read, so code strings, runes and raw strings later in the file are not traps.
10. **Tests.** `*_test.go` files are left out as dependents, as `cargo` leaves out dev-dependencies. They are never dependency targets, since importing a package never compiles its tests. Whether a later step reads test pairs is a separate spec choice. The extractor's own test files, for detection's "no test file of its own", are `*_test.go`.
11. **Detection.** `go-imports` could read a repository when it has at least one tracked `go.mod` that rule 1 keeps and one tracked non-test `.go` file inside it. A GOPATH-only repository (no `go.mod`) yields nothing and gets no note.

Stated counts:

- **Over-counts.** Files of every OS, architecture and custom build tag (except `ignore`), and cgo files whether or not cgo is on, the same set `go mod tidy` reads. A module that requires a *published* version of a local module, with no local replace and no `go.work`, still gets pairs to the local source. That pair names the same part's code at another version.
- **Under-counts.** Non-Go files in a package (`.c`, `.s`, `.syso`). `//go:embed` file dependencies. Imports from files under `testdata`, `_x`, `.x` and `vendor`. Imports of a path declared by two modules (rule 3). Ambiguous imports. Paths with escapes.
- **Exact.** For everything else, the folder a pair names is the one the go command would load, given that the local copy is the one used.

## Against the starting points

- *"go.mod module line + local replace + go.work use"*: the module lines alone give every mapping. `replace` and `use` only choose which copy the go command picks, and that choice depends on its working directory (§1). They aren't needed for resolution.
- *"package import → every non-test .go file in the folder"*: right, minus files starting with `_` or `.`, `//go:build ignore` files and `package documentation` files, and never including subfolders (§3).
- *"a module path declared twice is Unsure"*: right, but the index should first drop `go.mod` files under `testdata/`, `vendor/`, `_x/` and `.x/`. golang/tools has two under `testdata/`, and they would otherwise collide with real paths. A separate case is two modules that both *provide* one import's folder, through different paths. That is Go's ambiguous import, and it also yields no pair (§7).
- *"vendor/ and third-party never resolve"*: right. Go also forbids naming `x/vendor/y` in an import (§6, rule 6).
- *"tests are *_test.go"*: right, with two edges. `_test.go` (a leading underscore) is ignored rather than a test. `testdata/` is fixture data, not test code, and its `.go` files must be skipped too (§6).
- Longest-prefix matching: wrong in general. Go collects every module that provides the folder and errors on more than one. A nested `go.mod` stops the outer module, which makes it agree with longest-prefix only in well-formed layouts (§7).

## After the grill

The grill of 2026-11-25 (#537) amended these rules; the Go spec holds the settled form. A path two indexed modules declare is broken by a tracked, non-excluded `go.work`'s `use` when exactly one declarer is listed, and is otherwise Unsure, as is an ambiguous import. A `.go` file under no `go.mod` gets no pairs and is not Unsure: outside a module, "Only packages in the standard library and packages specified as `.go` files on the command line can be loaded, imported, and built" (go.dev/ref/mod, "Module commands outside a module"). **[cited]** The trial runs are etcd-io/etcd and golang/tools, both required.
