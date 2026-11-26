# Python extractor: the rules a text-only `python-imports` must follow

Research notes for issue #538. They set out how a text-only `python-imports` extractor turns Python imports into `(dependent file, dependency file)` pairs over a repository's tracked files, without running `python`. They extend the "Python" section of `language-extractors.md` and don't repeat it. Studio terms follow `docs/studio/CONTEXT.md`. The extractor contract is `docs/studio/behaviour/map.md`, "The extractors", and spec #529.

## How this was read

- Language and tool rules come from primary sources only. These are the Python language reference (https://docs.python.org/3/reference/), the library docs for `sys`, `importlib`, `unittest` and the `sys.path` initialisation page, the PEPs that own a rule (328, 420, 794, 810), the typing spec, the PyPA packaging guide, and the setuptools, Hatch, Poetry, Flit, uv and pytest docs. Pages were read through WebFetch on 2026-11-25. The fetch tool summarises pages, so quotes are as it returned them. docs.python.org/3/ served the 3.15 docs that day.
- Where the docs say nothing, CPython source was read at `python/cpython` `main` on 2026-11-25, through raw.githubusercontent.com. Line numbers are for that day's `main` and will drift. The files were:
  - `Lib/importlib/_bootstrap.py` and `Lib/importlib/_bootstrap_external.py`
  - `Python/import.c`, `Python/stdlib_module_names.h`, `Tools/build/generate_stdlib_module_names.py`
  - `Doc/reference/import.rst`, `Doc/reference/datamodel.rst`, `Doc/reference/lexical_analysis.rst` (the sources of the reference pages, for exact wording)
- The pytest ini options were read from the downloaded `reference/reference.html`, since the fetch tool cut the page short.
- The candidate repositories were read through `gh api` (repository metadata, the git tree, single files, code search). Nothing was cloned, installed or run.
- Each claim is tagged **[cited]** (with its URL and section, or a source file and line), **[repo]** (checked in this repository, with `file:line`) or **[judgement]** (my own reading, not checked against a source or code).

## 1. How an absolute name maps to files

### The search, one dotted step at a time

- For `foo.bar.baz`, "Python first tries to import `foo`, then `foo.bar`, and finally `foo.bar.baz`. If any of the intermediate imports fail, a `ModuleNotFoundError` is raised." **[cited]** https://docs.python.org/3/reference/import.html#searching
- A top-level name is searched along `sys.path`. A submodule is searched only in its parent's `__path__`: `_find_and_load_unlocked` takes `path = parent_module.__path__`. **[cited]** `_bootstrap.py:1204-1225`. So only the first element of a dotted name depends on roots. The rest is a walk down folders from where the first element was found. **[judgement]**
- Python's default `sys.meta_path` has three finders: "one that knows how to import built-in modules, one that knows how to import frozen modules, and one that knows how to import modules from an import path". **[cited]** import.html, "Finders and loaders" (`import.rst:334-337`). A built-in or frozen module is therefore found before any file on `sys.path`. **[judgement]** Which modules are frozen is a build choice; I did not read it. **unverified**

### Inside one folder: what wins

`FileFinder.find_spec` checks one `sys.path` entry, or one folder of a parent's `__path__`, in this order. **[cited]** `_bootstrap_external.py:1359-1408`.
1. A directory `<name>/` holding `__init__` plus a loader suffix: a regular package. "Check if the module is the name of a directory (and thus a package)" comes first.
2. Otherwise, a file `<name>` plus a suffix. The loaders are ordered extension modules, then source, then bytecode: `extension_loaders + [source, bytecode]`. **[cited]** `_bootstrap_external.py:1571-1586`. `SOURCE_SUFFIXES` is `['.py']`, plus `.pyw` on Windows. `BYTECODE_SUFFIXES` is `['.pyc']`. **[cited]** `_bootstrap_external.py:232-238`.
3. Otherwise, if `<name>/` is a directory with no `__init__`, it is recorded as a possible namespace portion.

So `foo/__init__.py` beats `foo.py` in the same folder, and a compiled `foo.cpython-313-darwin.so` beats `foo.py`. **[judgement]**, from the order above. Matching is case-sensitive unless `PYTHONCASEOK` is set on a case-insensitive platform. **[cited]** `_bootstrap_external.py:63-79`.

### Namespace packages (PEP 420)

- "With namespace packages, there is no `parent/__init__.py` file. In fact, there may be multiple `parent` directories found during import search". **[cited]** import.html#namespace-packages
- Across the path: if `<directory>/foo/__init__.py` is found, "a regular package is imported and returned"; if `<directory>/foo.{py,pyc,so,pyd}`, "a module is imported and returned"; if a bare `foo/` directory, "it is recorded and the scan continues". Only "If the scan completes without returning a module or package, and at least one directory was recorded" is a namespace package created. **[cited]** https://peps.python.org/pep-0420/#specification. The code matches: `PathFinder` collects portions and returns a namespace spec only after every entry. **[cited]** `_bootstrap_external.py:1242-1286`.
- So a regular package or module anywhere on the path beats a namespace portion earlier on it. Several bare `foo/` folders on different roots merge into one package whose `__path__` lists them all. **[judgement]**, from the algorithm.
- A subdirectory without `__init__.py` inside a regular package "is treated as an implicit namespace package". **[cited]** import.html#regular-packages

### Stubs and extension modules

- `.pyi` is not an import suffix: it is in none of the loader lists above. **[cited]** `_bootstrap_external.py:232-238`. Stubs are "syntactically valid Python files with a `.pyi` suffix", and it is "preferred that the `*.pyi` stubs are alongside the corresponding `*.py` files". Type checkers "MUST maintain the normal resolution order of checking `*.pyi` before `*.py` files". **[cited]** https://typing.python.org/en/latest/spec/distributing.html#stub-files, #stub-only-packages, #partial-stub-packages
- A stub-only distribution is named `foopkg-stubs`. **[cited]** same page, #stub-only-packages. Such a folder is not importable as `foopkg` at run time. **[judgement]**
- Extension modules (`.so`, `.pyd`) are build output and rarely tracked. Their sources (`.c`, `.pyx` for Cython) carry no Python imports the lexer could read. **[judgement]**

### `from a.b import c`: submodule or attribute

- The `from` form: "find the module specified in the `from` clause, loading and initializing it if necessary"; then for each name, "check if the imported module has an attribute by that name"; "if not, attempt to import a submodule with that name and then check the imported module again"; if still missing, `ImportError`. **[cited]** https://docs.python.org/3/reference/simple_stmts.html#the-import-statement. The code is `_handle_fromlist`: it tries `a.b.c` only when `not hasattr(module, x)`. **[cited]** `_bootstrap.py:1391-1425`.
- Loading a submodule by any route binds it as an attribute of its parent. **[cited]** import.html, "Submodules". So once `a/b/__init__.py` has run `from . import c` or `from .c import x`, the attribute *is* the submodule. **[judgement]**
- `from foo.bar import baz` imports "foo, foo.bar, and foo.bar.baz" when `baz` is a submodule, and `import foo.bar.baz as fbb` binds `foo.bar.baz`. **[cited]** simple_stmts.html#the-import-statement
- What a static reader should do. If `a/b/c.py` or `a/b/c/__init__.py` exists under the resolved `a/b/`, pair to it. Pair to `a/b/__init__.py` in every case, since it runs either way. If `a/b/__init__.py` itself defines a `c` that is not the submodule, Python never loads `a/b/c.py`, so the first pair is an over-count. **[judgement]** That needs a name defined in `__init__.py` to shadow a sibling file, which is rare.

### Parent `__init__.py` files

"Importing `parent.one` will implicitly execute `parent/__init__.py` and `parent/one/__init__.py`." **[cited]** import.html#regular-packages. So `import a.b.c` runs `a/__init__.py` and `a/b/__init__.py` before `a/b/c.py`: they are real run-time dependencies, and pairs to them are not false edges. **[judgement]** Whether to emit them is a choice of granularity, taken up in "Open for the grill".

## 2. Roots

### What Python does

- The first `sys.path` entry is "the directory that contains the input script, if there is one. Otherwise, the first entry is the current directory", for the REPL, `-c` and `-m`. **[cited]** https://docs.python.org/3/library/sys_path_init.html. `sys.path` says the same and adds that `-P` or `PYTHONSAFEPATH` drop that entry. **[cited]** https://docs.python.org/3/library/sys.html#sys.path
- `PYTHONPATH` entries come next, then the standard library, then site-packages added by `site`. **[cited]** sys_path_init.html (the order is implied by the page's sequence, not stated as a rule).
- So which folder a top-level name comes from depends on the script run, the current directory, the environment and what is installed. No text in the repository fixes it. **[judgement]**

### What packaging tools declare

- setuptools: automatic discovery knows a "src-layout" ("a `src` directory under the project root") and a "flat-layout" (packages "directly under the project root"). Flat layout leaves out a fixed list of names, among them `tests`, `test`, `docs`, `examples`, `scripts`, `tools`, `benchmarks` and `[._]*`. **[cited]** https://setuptools.pypa.io/en/latest/userguide/package_discovery.html#automatic-discovery. Explicit config is `[tool.setuptools] package-dir = {"" = "src"}` (or per package, `mypkg = "lib"`), `[tool.setuptools.packages.find] where = [...]`, or `setup.cfg`'s `[options] package_dir = =src` with `[options.packages.find] where = src`. **[cited]** same page, #custom-discovery. `setup.py` is Python code: its `package_dir` can only be guessed from text. **[judgement]**
- Hatch's wheel default tries, in order, `<NAME>/__init__.py`, `src/<NAME>/__init__.py`, `<NAME>.py`, `<NAMESPACE>/<NAME>/__init__.py`. **[cited]** https://hatch.pypa.io/latest/plugins/builder/wheel/#default-file-selection. `packages = ["src/foo"]` equals `only-include = ["src/foo"]` with `sources = ["src"]`, which strips the `src/` prefix. **[cited]** https://hatch.pypa.io/latest/config/build/#rewriting-paths
- Poetry, with no `packages`, finds "a single module or package whose name matches the normalized project name with `-` replaced with `_`", at the `pyproject.toml` level or "inside a `src/` directory". `{ include = "my_package", from = "lib" }` names another folder. **[cited]** https://python-poetry.org/docs/pyproject/#packages
- Flit: the source "may be located either in the directory that holds the `pyproject.toml` file, or in a `src/` subdirectory". **[cited]** https://flit.pypa.io/en/stable/pyproject_toml.html#pyproject-module
- PEP 794 (accepted 2025-09-05) adds `[project] import-names` and `import-namespaces`, the import names a project provides. **[cited]** https://peps.python.org/pep-0794/#specification. Where present, these say which top-level names a root owns. They are new, so few repositories have them yet. **[judgement]**
- uv workspaces list members with `[tool.uv.workspace] members` globs; each "must contain a `pyproject.toml` file", and "Dependencies between workspace members are editable". **[cited]** https://docs.astral.sh/uv/concepts/projects/workspaces/#getting-started, #workspace-sources
- The PyPA guide: in a flat layout the working directory, "the first item on the import path", shadows the installed copy; the src layout prevents that. **[cited]** https://packaging.python.org/en/latest/discussions/src-layout-vs-flat-layout/

So all four backends agree on "the project folder, or its `src/`", and an editable install puts exactly that folder on `sys.path`. **[judgement]**

### What pytest adds

- The default import mode `prepend` searches upward from a test file "until it can find the last directory which still contains an `__init__.py` file" and inserts the folder above that package root at the front of `sys.path`. A test file outside any package puts its own folder there. `append` adds to the end, and `importlib` leaves `sys.path` alone. The default "will remain `prepend` for the foreseeable future". **[cited]** https://docs.pytest.org/en/stable/explanation/pythonpath.html#import-modes
- `pythonpath` adds folders, "relative to the rootdir", "to the head of sys.path". **[cited]** https://docs.pytest.org/en/stable/reference/reference.html#confval-pythonpath
- rootdir is found from `pytest.toml`, `pytest.ini`, `pyproject.toml` (with a `[tool.pytest]` table, or as a fallback), `tox.ini` or `setup.cfg`, then `setup.py`. But "rootdir is NOT used to modify sys.path/PYTHONPATH". **[cited]** https://docs.pytest.org/en/stable/reference/customize.html#finding-the-rootdir

So under pytest, `tests/` with an `__init__.py` makes its project folder a root, and `tests/` without one makes `tests/` itself a root, so its helper modules import each other by bare name. **[judgement]** Flask's `tests/` and its examples' `tests/` have no `__init__.py`. **[cited]** `gh api repos/pallets/flask/git/trees/main?recursive=1`.

## 3. Relative imports

- "A single leading dot indicates a relative import, starting with the current package. Two or more leading dots indicate a relative import to the parent(s) of the current package, one level per dot after the first." Relative imports "may only use the second form": `import .x` is a syntax error. **[cited]** https://docs.python.org/3/reference/import.html#package-relative-imports. Grammar: `relative_module: "."* module | "."+`, so `from . import x` and `from .. import x` are legal. **[cited]** simple_stmts.html#grammar-token-python-grammar-relative_module
- The anchor is `__spec__.parent`, with `__package__` as a deprecated fallback. **[cited]** `_bootstrap.py:1428-1451`; https://docs.python.org/3/reference/datamodel.html#module.__package__. `ModuleSpec.parent` is "the fully qualified name of the package the module is in (or the empty string for a top-level module)", and "If the module is a package then this is the same as `name`". **[cited]** https://docs.python.org/3/library/importlib.html#importlib.machinery.ModuleSpec.parent
- So in `a/b/m.py` the current package is `a.b`, and in `a/b/__init__.py` it is also `a.b`. In both, one dot means the file's own folder. That makes resolution by folder uniform: one dot is the file's folder, each further dot one folder up. **[judgement]**, from the two cited rules.
- Resolution: `_resolve_name` splits the package on its last `level - 1` dots and raises `ImportError('attempted relative import beyond top-level package')` when there are too few. **[cited]** `_bootstrap.py:1130-1136`, and the C path at `Python/import.c:3863`. An empty package raises "attempted relative import with no known parent package". **[cited]** `_bootstrap.py:1186-1199`, `import.c:3883`. A script run directly has `__main__.__spec__` set to `None` "even if the file could technically be imported directly as a module". **[cited]** import.html, "`__main__.__spec__`".
- Where the top-level package ends depends on the root the file was imported from, not on the file. **[judgement]** A text extractor can stop at the file's root: dots that climb past it give no pair. Climbing into a folder with no `__init__.py` is legal for namespace packages, so the presence of `__init__.py` is not a safe boundary either. **[judgement]**
- Python 3 has no implicit relative imports: "`import <>` is always absolute". **[cited]** https://peps.python.org/pep-0328/#guido-s-decision. So `import utils` in `pkg/m.py` never means `pkg/utils.py` unless `pkg/` is itself on `sys.path` (a script folder, or pytest's prepend). **[judgement]**

## 4. Syntax the reader must handle

### Import statements

- Grammar: `["lazy"] "import" module ["as" identifier] ("," module ["as" identifier])*`; `["lazy"] "from" relative_module "import" identifier ["as" identifier] (...)*`; the same in parentheses with an optional trailing comma; and `"from" relative_module "import" "*"`. `module: (identifier ".")* identifier`. **[cited]** simple_stmts.html#grammar-token-python-grammar-import_stmt
- `lazy` (PEP 810, final, Python 3.15) is a soft keyword, allowed only at module level and never with `*` or `__future__`. **[cited]** https://peps.python.org/pep-0810/#syntax-restrictions; lexical_analysis.html#soft-keywords. A lazy import is still a dependency. **[judgement]**
- `import` is a hard keyword. **[cited]** https://docs.python.org/3/reference/lexical_analysis.html#keywords. Outside strings and comments, every `import` token starts or continues an import statement. `from` is not that safe: `raise E from err` and `yield from g` use it too. So the reader should anchor on `import` and look back for a `from` at the start of the statement. **[judgement]**
- Statements can share a line (`import a; import b`), or follow a compound header's colon (`try: import a`). **[judgement]**, from the grammar of simple statements.
- `from __future__ import x` is a future statement and names no file. **[cited]** simple_stmts.html#future-statements. `import __future__` is "an ordinary import statement", of the stdlib module. **[cited]** same section.
- `from m import *` is "only allowed at the module level". It depends on `m` like any other `from`. **[cited]** simple_stmts.html#the-import-statement
- Imports inside functions, `if TYPE_CHECKING:`, or `try:` / `except ImportError:` use the same statement syntax. A reader that does not track blocks sees them all. **[judgement]** A `TYPE_CHECKING` import is a type-only dependency, and a `try` import is often an optional third-party one, which resolves to nothing.
- `__import__()` and `importlib.import_module()` take a computed name. **[cited]** importlib.html#importlib.import_module. They are function calls, not statements, and stay out of scope. **[judgement]**
- Identifiers are normalised to NFKC while parsing. **[cited]** `lexical_analysis.rst:539`. A non-ASCII import name may not match its file name byte for byte. **[judgement]** Rare enough to leave as an under-count.

### Lines, comments, encoding

- A comment "starts with a hash character (`#`) that is not part of a string literal, and ends at the end of the physical line". **[cited]** lexical_analysis.html#comments
- A backslash at a line's end joins lines outside strings and comments. "A line ending in a backslash cannot carry a comment." **[cited]** lexical_analysis.html#explicit-line-joining. Inside `()`, `[]` and `{}`, lines join implicitly and "can carry comments". **[cited]** #implicit-line-joining
- An encoding declaration is a comment on line 1 or 2 matching `coding[=:]\s*([-\w.]+)`; the default is UTF-8, and a UTF-8 BOM is ignored. **[cited]** lexical_analysis.html#encoding-declarations. Import names are identifiers, so a lossy UTF-8 read (`Repo`) still finds ASCII imports in a Latin-1 file. **[judgement]**

### String literals

- Prefixes: `b`, `r`, `f`, `t` (template strings, 3.14), and `u`. "Prefixes are case-insensitive", and "The 'r' prefix can be combined with 'f', 't' or 'b', so 'fr', 'rf', 'tr', 'rt', 'br', and 'rb' are also valid". `u` combines with nothing. **[cited]** lexical_analysis.html#string-prefixes. So the valid prefixes, in any case, are `r u b f t br rb fr rf tr rt`. **[judgement]**, from that list.
- Four quote forms: `'`, `"`, `'''`, `"""`. In a triple-quoted literal, "three unescaped quotes in a row terminate the literal, if they are of the same kind". **[cited]** lexical_analysis.html#string-literals
- A backslash escapes the next character, also in raw strings, where it stays in the value: `r"\""` is a string. **[judgement]**; the rule that a raw literal cannot end in an odd number of backslashes was not re-read. **unverified**
- f-strings since 3.12 (PEP 701): replacement fields can hold "nested strings, comments, and backslashes", reuse the outer quote (`f"abc {a["x"]} def"`), and nest f-strings. A `#` inside a field is a comment "even closing braces and quotes". **[cited]** lexical_analysis.html#f-strings. t-strings "follow the same syntax rules". **[cited]** #t-strings. So an f-string cannot be skipped by scanning for its closing quote. The lexer must count `{`/`}` (with `{{` and `}}` as escapes) and lex nested strings inside fields. **[judgement]**

## 5. Test files

- pytest collects `test_*.py` and `*_test.py`: `python_files` defaults to `["test_*.py", "*_test.py"]`. It recurses into folders except `norecursedirs`, by default `["*.egg", ".*", "_darcs", "build", "CVS", "dist", "node_modules", "venv", "{arch}"]`. **[cited]** reference.html#confval-python_files, #confval-norecursedirs; https://docs.pytest.org/en/stable/explanation/goodpractices.html#conventions-for-python-test-discovery
- A folder named `tests` is not special to pytest. **[cited]** goodpractices.html (the name appears only in examples).
- `conftest.py` files are "local per-directory plugins", loaded from each test path up to the config file's folder. **[cited]** https://docs.pytest.org/en/stable/how-to/writing_plugins.html#conftest-py-local-per-directory-plugins
- unittest discovery defaults to `test*.py` from `.`. Test files "must be modules or packages importable from the top-level directory". **[cited]** https://docs.python.org/3/library/unittest.html#test-discovery. `test*.py` is wider than pytest's `test_*.py`: it takes `testing.py` and `tests.py`. **[judgement]**
- Both are config: `python_files` in `[tool.pytest.ini_options]` and unittest's `-p` can change them. **[cited]** the pages above.
- Test helpers under `tests/` that match no pattern (`fixtures.py`, `utils.py`, `messages.py`) are common. langchain has `libs/langchain_v1/tests/unit_tests/agents/messages.py` and `libs/partners/qdrant/tests/integration_tests/fixtures.py`. **[cited]** `gh api search/code` on 2026-11-25. Without a folder rule, they would be read as production files. **[judgement]**

## 6. The standard library

- `sys.stdlib_module_names` (3.10+): "the same on all platforms"; it lists modules "not available on some platforms" too, and "Test modules are excluded". "For packages, only the main package is listed". **[cited]** https://docs.python.org/3/library/sys.html#sys.stdlib_module_names
- The generator leaves out `test`, `idlelib.idle_test`, the `_test*` modules and `xx*`, but keeps `doctest` and `unittest`. **[cited]** `generate_stdlib_module_names.py:18-50`. The list on `main` has 297 names, including `__future__` and `tomllib`. It no longer has `distutils` or `imp`, removed in 3.12. **[cited]** `Python/stdlib_module_names.h`
- The list changes per version, and a Rust extractor cannot ask Python for it. It would embed one: the union over the supported versions. **[judgement]**
- An import that matches no tracked file gives no pair anyway, so stdlib and third-party imports are not Unsure without a list. The list matters only when a tracked file shares a stdlib name, such as a local `types.py`, `logging/` or `test/`. Then a script in that folder gets the local file (its folder is `sys.path[0]`, before the stdlib), a module elsewhere gets the stdlib, and a built-in or frozen module always wins. **[judgement]**, from §1 and §2.

## 7. Trial repositories

| Repository | Licence | Tracked / `.py` / test-pattern files | Roots | Fit |
|---|---|---|---|---|
| encode/httpx | BSD-3-Clause | 125 / 60 / 32 | one `pyproject.toml` (hatchling), flat `httpx/`, `tests/__init__.py` | plain package |
| pallets/flask | BSD-3-Clause | 236 / 83 / 30 | root `pyproject.toml` (flit) with `src/flask/`, plus three `examples/*/pyproject.toml`; `tests/` has no `__init__.py` | `src/` layout, nested roots |
| langchain-ai/langchain | MIT | 3308 / 2672 / 646 | 22 `pyproject.toml` under `libs/`, none at the root; hatchling and `[tool.uv.sources]` | monorepo |
| python-attrs/attrs | MIT | 144 / 56 / 27 | one `pyproject.toml`, `src/attr` and `src/attrs` with 10 `.pyi` beside `.py` | stubs; a spare |

All figures come from `gh api repos/<r>` and `repos/<r>/git/trees/<branch>?recursive=1` on 2026-11-25, none truncated. All four were pushed within the last month. **[cited]**

**Recommendation. [judgement]**
- **langchain-ai/langchain** as the main run. It is the case the starting points were written for. `tests` is a top-level package name under 22 roots, and `scripts` under 21. **[cited]** tree count. `from tests.` appears in 125 files. **[cited]** `gh api search/code`. Each lib imports its own `tests`, and cross-lib imports name distinct packages (`langchain_core`, `langchain_anthropic`). So it shows directly whether "two roots is Unsure" or "nearest root wins" is the right rule.
- **pallets/flask** as the second. It has a `src/` layout, nested example roots (`examples/tutorial/flaskr` imported by `examples/tutorial/tests`) and `tests/` folders without `__init__.py`.
- **encode/httpx** is the smallest sanity run: one root, flat layout, no traps.

## What this means for the extractor

Rules, all **[judgement]** built on the citations above:

1. **Roots.** The repository root, and each folder with a tracked `pyproject.toml`, `setup.cfg` or `setup.py` outside ignored folders, each with its `src/` when that exists. Where the config says so in text, add or replace with `package-dir` / `packages.find where` (setuptools), `packages … from` (Poetry), `sources` (Hatch). Each backend reviewed already looks in the project folder and its `src/`, so the conventional pair covers the default case.
2. **A name to a file.** Under a root, walk the dotted name folder by folder. At each step take `<name>/__init__.py`, else `<name>.py`, else a bare `<name>/` folder (namespace), as `FileFinder` does. `.pyi` is a fallback target only when no `.py` exists, since a stub stands for a module that may be compiled.
3. **Several roots.** Resolve the full dotted name under every root. One root with a hit gives the pair. Two or more give Unsure, unless the rule chosen in the grill breaks the tie (nearest ancestor, §2). Bare namespace folders in several roots do not collide by themselves; only a final file found twice does.
4. **`from a.b import c`.** Pair to `a/b/c.py` or `a/b/c/__init__.py` when it exists, and to `a/b/__init__.py` always.
5. **Relative imports.** One dot is the file's folder, each further dot one folder up, for modules and `__init__.py` alike. Climbing above the file's root gives no pair.
6. **The reader.** Lex comments, backslash joins, brackets and every string form, with f- and t-string fields nested. Anchor on the `import` keyword and look back for `from`, so `raise … from` and `yield from` don't confuse it. Skip `__future__`. Read imports at any depth, with `lazy` allowed.
7. **Not read.** `__import__`, `importlib.import_module`, `sys.path` edits in code, `.pth` files, and run-time config such as `PYTHONPATH`.
8. **Tests.** `test_*.py`, `*_test.py`, `conftest.py`, and files under a folder named `tests` or `test` are test files.
9. **Standard library.** Embed the union of `sys.stdlib_module_names` over the supported versions, and use it only where a tracked file shares a stdlib name.

Stated counts:
- **Over-counts.** A pair to `a/b/c.py` when `a/b/__init__.py` defines a `c` that shadows it. Pairs to parent `__init__.py` files if the grill keeps them (they are real: they run). `TYPE_CHECKING` imports, which never run.
- **Under-counts.** Dynamic imports. Imports resolved through `sys.path` edits, `.pth` files, pytest's `pythonpath` or a `tests/` folder put on `sys.path` by prepend mode. Imports of compiled modules whose source is `.pyx` or C. Non-ASCII names that NFKC changes.

## Against the starting points

- *"roots: the repository root, plus every folder with a tracked `pyproject.toml`, `setup.py` or `setup.cfg`, each with its `src/`"*: **confirmed** as the default case, with two gaps. Explicit layouts (`package-dir`, `where`, Poetry `from`, Hatch `sources`) can name another folder (§2). pytest's prepend mode also makes any test folder without `__init__.py` a root, which this list misses (§2). The repository root as a root is right for scripts and flat repositories, but it also makes `libs/core/langchain_core` reachable as `libs.core.langchain_core`. That is harmless, since nobody imports that name.
- *"a name under two roots is Unsure"*: **corrected**. Python never reports two roots as ambiguous: the first `sys.path` entry with a regular package or module wins, and bare namespace folders merge (§1). Unsure is right only because the order is run-time state. It should apply to a final file found under two roots, not to a shared namespace folder name.
- *"relative imports resolve from the file"*: **confirmed**, with one dot meaning the file's own folder for modules and `__init__.py` alike (§3). The beyond-top-level edge is open (it depends on the root).
- *"`from a.b import c` tries `a/b/c.py`, then `a/b/__init__.py`"*: **corrected in order**. Python checks the attribute in `a/b/__init__.py` first and the submodule second (§1). And `a/b/__init__.py` always runs, so it is a dependency in both cases, not a fallback. Also try `a/b/c/__init__.py`.
- *"verify: nearest-ancestor root precedence"*: **open**. Python has no such rule; precedence is `sys.path` order (§2). In practice it matches how a monorepo is run: pytest, run from one lib, puts that lib first, and an editable install puts that lib's folder on the path. langchain's `tests` under 22 roots is where it pays (§7). What it gets wrong: a file that really imports a sibling root's package of the same name (one lib's test importing another lib's `tests` helpers), and a file at the repository root, whose nearest root is the root itself.
- *"verify: edges to parent `__init__.py` files"*: **confirmed as real** dependencies (§1). Whether to emit them is open.
- *"the lexer handles `#`, all four quote forms with `r b f u` prefixes, and multi-line parenthesised imports"*: **corrected**. It also needs `t` (3.14), the case-insensitive combinations `br rb fr rf tr rt`, backslash continuations, nested f-string fields (PEP 701), `;` and `:` on one line, and the `raise … from` / `yield from` trap (§4).
- *"tests: `test_*.py`, `*_test.py`, `conftest.py`, and `tests/` and `test/` folders"*: **confirmed** for pytest's defaults (§5). The folders are a convention, not pytest's rule, and they catch helpers no pattern names. unittest's default `test*.py` is wider (`tests.py`, `testing.py`). A configured `python_files` is not read.

## Open for the grill

1. **Nearest-ancestor root, or Unsure?** For a name found under two roots, pick the root nearest the importing file, or give Unsure. langchain decides it in practice.
2. **Parent `__init__.py` pairs.** Emit them (true, but every deep import then reaches the top package's part), or pair only to the final file and the `from` module's `__init__.py`.
3. **Read build configs?** Honour `package-dir`, `packages.find where`, Poetry `from` and Hatch `sources` in TOML, or keep only "folder plus `src/`". `setup.py` can only be guessed at.
4. **`.pyi` targets.** Pair to a stub when there is no `.py`, or never.
5. **`TYPE_CHECKING` imports.** Count them as dependencies (the map's link is about code), or skip their blocks (they never run), which needs block tracking.
6. **Test-file config.** Read `python_files` from `[tool.pytest.ini_options]`, or keep the defaults. unittest's `test*.py`: add it or not.
7. **Test folders as roots.** Model pytest prepend's `tests/`-as-root for helper imports, or leave it as an under-count, since test files make no pairs anyway.
8. **Detection.** Is one tracked non-test `.py` enough, or should it also need a `pyproject.toml` / `setup.*`? Python, unlike Go, needs no config to run, so the contract's `MissingConfig` is likely a fallback.
9. **The stdlib list.** Which versions does the union cover, and where does it live in the code?
10. **The trial run.** langchain as the required run, Flask second, as §7 suggests.
