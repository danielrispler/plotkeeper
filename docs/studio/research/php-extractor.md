# PHP extractor: the rules a text-only `php-imports` must follow

Research notes for issue #540. They set out how a text-only `php-imports` extractor turns a PHP repository's `use` declarations and file includes into `(dependent file, dependency file)` pairs over its tracked files, without running `php` or `composer`. They extend the "PHP" section of `language-extractors.md` and the Go and Java notes (`go-extractor.md`, `java-extractor.md`), and don't repeat them. Studio terms follow `docs/studio/CONTEXT.md`. The extractor contract is `docs/studio/behaviour/map.md`, "The extractors", and spec #529 (Problem / Unsure / Pair, `Names<T>`).

## How this was read

- The language described is **PHP 8.6**, at php-src tag `php-8.6.0` (`8aaada3f9`, 2026-11-17). Its scanner and grammar were read as the authority where the manual is thin: `Zend/zend_language_scanner.l`, `Zend/zend_language_parser.y`, `Zend/zend_compile.c`, `Zend/zend_execute_API.c`, `Zend/zend_autoload.c`, `Zend/Makefile.frag`, `main/fopen_wrappers.c`, `main/main.c`, `php.ini-production` and `php.ini-development`. They were fetched with `gh api …/contents/<path>?ref=php-8.6.0`, so quotes are verbatim. A cite like `scanner.l:2489` means `Zend/zend_language_scanner.l` at that tag; `parser.y` and `compile.c` likewise.
- The PHP manual was read from its source, `php/doc-en` at `b9ea47a0b` (2026-11-25): `language/namespaces.xml`, `basic-syntax.xml`, `attributes.xml`, `constants.xml`, `control-structures/include.xml` and `require.xml`, `oop5/autoload.xml`, `appendices/ini.core.xml`, `reference/filesystem/functions/dirname.xml`. Each is published at php.net; a cite like *manual, "Name resolution rules"* means https://www.php.net/manual/en/language.namespaces.rules.php.
- RFCs were read at https://wiki.php.net/rfc/<name> (`?do=export_raw`, with `curl`), for their status lines and text.
- Composer is **2.11.3** (`e4a580d4d`, released 2026-11-24): `src/Composer/Autoload/ClassLoader.php`, `src/Composer/Autoload/AutoloadGenerator.php`, `doc/04-schema.md` (https://getcomposer.org/doc/04-schema.md) and `doc/articles/autoloader-optimization.md`. Its class-map scanner lives in `composer/class-map-generator`, read at `main` (`e3bfe2246`, 2026-11-03; latest release 1.9.0): `ClassMapGenerator.php`, `PhpFileParser.php`, `PhpFileCleaner.php`.
- PSR-4 and PSR-0 were read at https://www.php-fig.org/psr/psr-4/ and https://www.php-fig.org/psr/psr-0/ with `curl`.
- Test tools: PHPUnit **13.4.8** (`src/TextUI/Configuration/Xml/Loader.php`) and its documentation's `13.4` branch (`101fd8e5c`): `organizing-tests.rst`, `xml-configuration-file.rst`, `textui.rst`. Pest **5.3.7**, whose documentation was read at `pestphp/docs` `5.x` (`6e368f64f`): `installation.md`, `writing-tests.md`, `configuring-tests.md`.
- Seven public repositories were shallow-cloned into a scratch folder, not this repository (§9 lists them with commits). Nothing was built or run. A throwaway Python scan lexed every `.php` file roughly (PHP tags, comments, strings with interpolation, heredoc and nowdoc, `#[`), then read namespace lines, top-level `use` declarations, `require`/`include` expressions and class declarations, and resolved each class `use` through every tracked `composer.json`'s PSR-4 and PSR-0 tables the way Composer's `ClassLoader` walks them. Its "production" files are those with no `tests` or `test` folder in their path and no `Test.php` suffix.
- Each claim is tagged **[cited]** (with its URL and section, or a source file and line), **[measured]** (counted by that scan, rough: it does not parse PHP, and its counts are for scale), **[repo]** (checked in this repository) or **[judgement]** (my own reading, not checked against a source).

## 1. Which text is PHP code

### Tags and inline HTML

- Outside PHP mode, everything is inline HTML. PHP mode starts at `<?php` followed by a space, tab or newline (or the end of the file), or at `<?=`, which is always available. **[cited]** `scanner.l:2330-2359`; manual, "PHP tags". The tag match is case-insensitive, so `<?PHP` counts. **[cited]** the inline handler's `strncasecmp(…, "php", 3)` at `scanner.l:2387-2392`. The scanner is built with re2c's `--case-inverted` flag (`Zend/Makefile.frag:9`), under which its double-quoted rules such as `"use"` and `"<?php"` match any case. **[cited]** for the flag; its meaning is from re2c's documentation, which I did not read (unverified, high confidence).
- A bare `<?` opens PHP mode only when `short_open_tag` is on. **[cited]** `scanner.l:2361-2368`. Its default is on (`"1"`), and both ini files php-src ships set it `Off`. **[cited]** manual, `ini.core.xml` table (`main/main.c:849` registers it from `DEFAULT_SHORT_OPEN_TAG`); `php.ini-production:198`, `php.ini-development:198`. An RFC to deprecate short tags was declined. **[cited]** https://wiki.php.net/rfc/deprecate_php_short_tags_v2, "Status: Declined". So whether `<? … ?>` is code depends on the server's config. **[judgement]** Measured: 1 file with a bare `<?`, a WordPress test fixture about short tags. **[measured]**
- `?>` leaves PHP mode, counts as a `;`, and swallows one newline right after it. **[cited]** `scanner.l:2555-2564`; manual, "Instruction separation".
- **A `//` or `#` comment ends at the end of the line or at `?>`, whichever comes first.** **[cited]** `scanner.l:2489-2511` (the loop breaks on `?` followed by `>`); manual, "Comments": "The "one-line" comment styles only comment to the end of the line or the current block of PHP code". A `/* */` comment and a string do not end at `?>`. **[cited]** `scanner.l:2513-2546`, and the string rules. So `<?php // note ?></ul>` is a comment, then HTML. Measured: 207 such sites in 87 WordPress files, and a few in Filament and Symfony templates. **[measured]**
- Inline HTML may sit inside a function or class body: Filament's class files write `ob_start(); ?> … <?php } ?>`, so braces open in one PHP block and close in another. 99 Filament files use `<?=`. **[measured]** A reader keeps its brace depth across HTML runs. **[judgement]**
- `__halt_compiler();` stops the scanner: the rest of the file is data, often a phar payload. **[cited]** `parser.y:405-408` (`zend_stop_lexing()`).

### Comments, attributes and strings

- `#[` always starts an attribute in PHP mode; any other `#` starts a comment. **[cited]** `scanner.l:1452-1455` (`T_ATTRIBUTE`), and the `HASH_COMMENT` pattern `"#"(([^[\x00]…` at `scanner.l:1421`. The `#[` syntax is PHP 8.0's. **[cited]** https://wiki.php.net/rfc/shorter_attribute_syntax_change, "Status: Implemented", target 8.0. In PHP 7 the same text was a comment. **[judgement]**, from the RFC's choice of a form that was a comment before. Attributes take names and arguments, so strings and nested brackets: `#[Route('/x', methods: ['GET'])]`. Measured: attributes in 3,033 Symfony files and 1,698 PHPUnit files. **[measured]**
- Single-quoted strings take only `\\` and `\'` escapes. Double-quoted strings, backticks and heredocs interpolate. `{$` switches back to full PHP code until the matching `}`, so `"{$a["k"]}"` holds a nested string. **[cited]** `scanner.l:2875-2880` (`yy_push_state(ST_IN_SCRIPTING)`, `enter_nesting('{')`), and `"${"` at `scanner.l:2010`. Measured: `{$` interpolation in 505 WordPress files and 421 Filament files. **[measured]**
- Heredoc and nowdoc open with `<<<`, optional spaces or tabs, then `LABEL`, `"LABEL"` or `'LABEL'` (nowdoc), then a newline; an optional `b` prefix is allowed. **[cited]** `scanner.l:2707`. Since PHP 7.3 the closing label may be indented and may be followed by more code on its line. **[cited]** https://wiki.php.net/rfc/flexible_heredoc_nowdoc_syntaxes, "Implemented (in PHP 7.3)". The scanner closes the body at a line that starts with spaces or tabs and then the label, not followed by a label character. **[cited]** `scanner.l:3155-3160` (nowdoc; heredoc at `:3031`). Measured: indented closing labels in 299 Symfony files and 107 Filament files. **[measured]**

### Names and keywords

- Since PHP 8.0 a namespaced name is one token: `Foo\Bar` is `T_NAME_QUALIFIED` (`{LABEL}("\\"{LABEL})+`), `\Foo` is `T_NAME_FULLY_QUALIFIED`, and `namespace\Foo` is `T_NAME_RELATIVE`. **[cited]** `scanner.l:2468-2482`; https://wiki.php.net/rfc/namespaced_names_as_token, "Implemented", target 8.0. So no space or comment can sit inside a name, and a segment may be a reserved word (`iter\fn`). **[cited]** same RFC, "Proposal". `LABEL` is `[a-zA-Z_\x80-\xff][a-zA-Z0-9_\x80-\xff]*`, bytes not characters. **[cited]** `scanner.l:1408`.
- Keywords are case-insensitive: `USE`, `Require_Once` and `__dir__` all work. **[judgement]**, from `--case-inverted` above.
- A keyword after `->`, `?->` or `::` is a member name: the grammar's `identifier` takes any `semi_reserved` word. **[cited]** `parser.y:316-334`. So `$this->include('x')` is a method call, not an include. Measured: 39 such calls in Symfony. **[measured]**
- Composer's own class-map scanner is a text-only reader of this kind. It skips to `<?`, treats `?>` as leaving PHP, ends `#` and `//` comments at `?>`, skips strings and heredocs, and keeps `#[` on PHP 8 or later. It does not follow `{$` nesting. **[cited]** `PhpFileCleaner.php:86-154` (`clean`), `:156-167` (`skipToPhp`), `:206-219` (`skipToNewline`, "which PHP also ends at a ?> close tag"), `:115`.

**What this means.** The reader is a small state machine: HTML mode, PHP mode, strings with a `{$` stack, heredoc and nowdoc bodies, `/* */`, and line comments that stop at `?>`. It carries brace depth across HTML runs and stops at `__halt_compiler`. Short tags are the one place where the answer depends on server config. **[judgement]**

## 2. Namespace declarations

- Three forms, all top-level statements: `namespace Name;`, `namespace Name { … }` and `namespace { … }` for global code. **[cited]** `parser.y:409-416`. A name may be `T_NAME_QUALIFIED` or an identifier, never fully qualified. **[cited]** `parser.y:341-345`; manual, "Defining namespaces".
- The first namespace declaration must be the first statement, after only `declare(…)`. **[cited]** `compile.c:10526-10531`; manual: "no non-PHP code may precede a namespace declaration". So a file with inline HTML before `<?php namespace X;` does not compile. **[judgement]**, from the manual's example.
- A file may hold several namespaces. Braced and unbraced forms cannot be mixed, and braced ones cannot nest. Global code beside namespaced code needs the braced form. **[cited]** `compile.c:10507-10523`; manual, "Defining multiple namespaces in the same file".
- Each declaration resets the import tables: a `use` in one namespace block does not carry to the next. **[cited]** `compile.c:10549` (`zend_reset_import_tables()`).
- Measured: files with more than one namespace are rare (0 to 6 per repository, mostly test fixtures and stubs), and braced namespaces likewise (0 to 6). **[measured]**

## 3. `use` declarations

### Grammar

At top level (`parser.y:418-421`, rules at `:424-482`):

```
top_statement: T_USE mixed_group_use_declaration ';'
             | T_USE use_type group_use_declaration ';'
             | T_USE use_declarations ';'
             | T_USE use_type use_declarations ';'
use_type:      T_FUNCTION | T_CONST
use_declaration:            legacy_namespace_name [T_AS T_STRING]   (leading \ allowed)
group_use_declaration:      legacy_namespace_name '\' '{' unprefixed_use_declarations [','] '}'
mixed_group_use_declaration: legacy_namespace_name '\' '{' inline_use_declarations [','] '}'
inline_use_declaration:     [use_type] unprefixed_use_declaration
unprefixed_use_declaration: namespace_name [T_AS T_STRING]          (no leading \)
```

- So: `use A\B, C\D as E;` (a comma list), `use function A\f;`, `use const A\C;`, `use A\{B, C\D as E,};` (trailing comma allowed), `use function A\{f, g};`, and the mixed form `use A\{B, function f, const C};`. In a group the items may be qualified (`C\D`) but not fully qualified, and `function`/`const` may be written per item only when the group itself has no `use_type`. **[cited]** the rules above. Group use is PHP 7.0's, the trailing comma 7.2's. **[cited]** https://wiki.php.net/rfc/group_use_declarations, "Implemented (in PHP 7.0)"; https://wiki.php.net/rfc/list-syntax-trailing-commas, "Implemented in PHP 7.2 (Grouped namepaces only)".
- A group item is the prefix joined to the item's name, and takes the group's type or else the item's own. **[cited]** `compile.c:10406-10423`.
- `use A\B;` means `use A\B as B`. **[cited]** `compile.c:10333`. A leading `\` is allowed and ignored outside groups: "import names must be fully qualified, and are not processed relative to the current namespace". **[cited]** manual, "Using namespaces: Aliasing/Importing".
- `use` declarations live only in `top_statement`, never in `inner_statement`, so never inside a function, `if` or block: "it cannot be block scoped". **[cited]** `parser.y:497-503`; manual, "Scoping rules for importing". So a conditional `use` cannot exist. A `use` inside a class body is a trait use (`parser.y:1007`), and `function () use ($x)` is a closure's captured variables (`parser.y:1431`). **[cited]** Both are told apart by brace depth and by the `(` after `use`. **[judgement]**
- "Importing rules are per file basis, meaning included files will NOT inherit the parent file's importing rules." **[cited]** manual, same section.
- A `use` may follow inline HTML in a later PHP block of the same file, since inline HTML is a statement. **[judgement]**, from `top_statement: statement`.

### What a `use` names

- A class `use` imports a class-like name **or a namespace**: "PHP can alias(/import) constants, functions, classes, interfaces, traits, enums and namespaces", and after `use My\Full\NSname;` the code calls `NSname\subns\func()`. **[cited]** manual, "Aliasing/Importing", example 1. A qualified name's first segment goes through the class import table: "if the namespace A\B\C is imported as C, the name C\D\E is translated to A\B\C\D\E". **[cited]** manual, "Name resolution rules".
- Imports are compile-time aliases and load nothing: "Importing is performed at compile-time, and so does not affect dynamic class, function or constant names". **[cited]** manual, "Aliasing/Importing". A class is loaded when code uses it and it is not yet defined. **[cited]** `zend_execute_API.c:1282-1344` (`zend_lookup_class_ex`). So an unused `use`, or one that only a docblock mentions (`@param Foo`), still names the class but nothing loads it. **[judgement]** Measured, production files: `use` items whose alias appears in no code token but in a comment: 561 in Symfony, 385 in Filament, 309 in PHPUnit, 93 in Doctrine; with no mention at all, 0 to 9. **[measured]**
- Class and function imports are looked up case-insensitively, constant imports case-sensitively. **[cited]** `compile.c:10318`, `:10348`.
- **Functions and constants cannot be autoloaded.** PHP 8.6 keeps one autoloader table, for classes (`zend_class_autoload_functions`). **[cited]** `zend_autoload.c:26`, `:46`, `:86`. The manual's autoload page covers "classes, interfaces, traits, and enumerations". **[cited]** manual, "Autoloading Classes". Every function-autoloading RFC is unfinished: `function_autoloading` and `function_autoloading_v2` "Draft", `function_autoloading4` "Discarded (or Accepted or Declined)", `core-autoloading` "Under Discussion", target 8.3. **[cited]** wiki.php.net/rfc, status lines. So `use function A\f;` names a function that some earlier `require` or a Composer `files` entry has loaded. PSR-4 never maps it. **[judgement]**, from the above.
- Measured, production files: most `use function` and `use const` items import global, unqualified names, PHP's own functions and constants (`use function count;` is Doctrine's and PHPUnit's style: 896 and 2,228 items; all 896 of Doctrine's `src/` ones are unqualified). Laravel's 206 and Filament's 254 include repository helpers such as `Illuminate\Support\enum_value`, declared in `src/Illuminate/Collections/functions.php`. Helper files wrap declarations in `if (! function_exists('…'))` (64 guards in Laravel's `Foundation/helpers.php`, 22 in Filament's `support/src/helpers.php`), so the declaration sits inside a block. **[measured]**
- Measured, production files: group `use` 0 (2 in tests), mixed group 0, a leading `\` 0 (4 in Symfony tests), and aliases with a new name 20 to 484 per repository. A class `use` that names a namespace folder rather than a file: 37 in Filament, 13 in Symfony, 12 in Doctrine. **[measured]**

**What this means.** The `use` grammar is small and fully readable. A class `use` resolves through the autoload rules (§4). A namespace `use` resolves to no file, and the classes reached through it (`Alias\Sub\Name`) need a body scan (§7). `use function` and `use const` cannot go through PSR-4: they need an index of function declarations, nested ones included, or they are skipped. **[judgement]**

## 4. Composer autoloading

### PSR-4, as specified

- "A contiguous series of one or more leading namespace and sub-namespace names … (a "namespace prefix") corresponds to at least one "base directory"." Sub-namespaces after the prefix map to subfolders, "The subdirectory name MUST match the case of the sub-namespace names", and the class name maps to a `.php` file whose name "MUST match the case of the terminating class name". A fully qualified class name "MUST have a top-level namespace name". **[cited]** PSR-4, §2.
- PSR-4 says nothing about choosing between prefixes. **[cited]** by absence, §2.

### PSR-4, as Composer runs it

- `psr-4` maps prefixes to paths "relative to the package root". A prefix "must end in `\\`". A prefix may map to an array of folders, and the empty prefix `""` is a fallback for any namespace. **[cited]** 04-schema.md, "PSR-4". Composer rejects a non-empty prefix without the trailing separator. **[cited]** `AutoloadGenerator.php:582-588`, `ClassLoader.php:250`. A leading `\` is stripped. **[cited]** `AutoloadGenerator.php:1305`.
- The lookup, `findFileWithExtension($class, '.php')`, is **[cited]** `ClassLoader.php:442-559`:
  1. the class map first (`:444`);
  2. PSR-4: from the longest namespace prefix of the class down to the shortest, for each prefix registered, try each of its folders in order, and return the first `<dir>/<rest>.php` that exists (`:500-511`). A prefix whose folders lack the file **falls through to shorter prefixes**;
  3. the empty-prefix fallback folders (`:514-519`);
  4. PSR-0, then the PSR-0 fallbacks, then `include_path` if enabled (`:521-556`).
- Only `.php` is tried (`.hh` on HHVM). **[cited]** `ClassLoader.php:458-462`. The loader checks only that the file exists, never that it declares the class. **[cited]** the same lines (`file_exists`).
- Measured: in Filament, 55 class `use` items resolve only through a shorter prefix. `Filament\FontProviders\` maps to `packages/spatie-laravel-google-fonts-plugin/src`, and `Filament\FontProviders\Contracts\FontProvider` lives under `Filament\`'s `packages/panels/src`. Laravel maps `Illuminate\Support\` to four folders and `Illuminate\` to `src/Illuminate/`, which holds `Support/` too. **[measured]**
- Order across packages: the root package first, then its dependents, so the root can override. **[cited]** `AutoloadGenerator.php:626-631`. Composer reads the root `composer.json` and those of installed packages; "packages that are not installed cannot autoload anything". **[cited]** `AutoloadGenerator.php:1284-1287`. So a monorepo's nested `composer.json` files count only when the package is the root or installed, for example as a path repository. **[judgement]**
- Measured, across all tracked `composer.json` files (Laravel 40, Symfony 216): **no** class `use` resolved to two different files. Laravel's and Symfony's per-component files (`"Symfony\\Component\\Console\\": ""`) map the same file as the root's prefix. Composer's own repository tracks 34 `composer.json` files, 33 of them test fixtures. **[measured]**

### Case

- PHP's class table is case-insensitive, but the autoloader receives the name as written at the use site. **[cited]** `zend_execute_API.c:1282` (lowercased key), `:1344` (`autoload_name = zend_string_copy(name)`). So Composer looks for the path with the written case. On a case-insensitive filesystem a wrong-case name loads; on Linux it fails. Composer warns when two paths "only differ in casing and merge into one on case-insensitive filesystems (e.g. Windows and macOS), which breaks autoloading there". **[cited]** `AutoloadGenerator.php:381-385`.
- Measured: one test `use` names `…\Models\Cms\CmsUser` for `tests/Tests/Models/CMS/CmsUser.php` (Doctrine). In Filament, `packages/upgrade/src/rector.php` has `use Filament\Upgrade\Rector;`, a namespace import of the folder `Rector/`; matched case-insensitively it would hit `rector.php` itself. **[measured]**

### PSR-0, `classmap`, `files` and `exclude-from-classmap`

- **PSR-0** is "Deprecated - As of 2014-10-21". It keeps the prefix in the path, and turns `_` in the class name into a folder separator. **[cited]** PSR-0. Composer's PSR-0 also accepts class-level and PEAR-style keys (`"UniqueGlobalClass": ""`). **[cited]** 04-schema.md, "PSR-0"; `ClassLoader.php:521-547`. Measured: only Composer's own repository uses it, in 10 fixture files. **[measured]**
- **`classmap`** is built "by scanning for classes in all `.php` and `.inc` files in the given directories/files", with `*` wildcards. **[cited]** 04-schema.md, "Classmap"; `ClassMapGenerator.php:55`. Composer's scanner is the text-only `PhpFileParser` (§1). **[cited]** Lost by not reading it: **PHPUnit autoloads only by `classmap: ["src/"]`**, with no PSR-4 prefix, so 2,778 of its 4,288 production class `use` items name repository classes no PSR-4 rule finds. **[measured]**
- **`files`** are required "whenever `vendor/autoload.php` is included", for "PHP functions that cannot be autoloaded". **[cited]** 04-schema.md, "Files". Lost by not reading them: the pairs from `use function` (and from any call) to helper files. Measured: 17 `files` entries in Laravel, 9 in Filament, and Filament's root lists `tests/helpers.php` among them. **[measured]**
- **`exclude-from-classmap`** filters only classmap generation, including an optimized dump's scan of PSR folders. **[cited]** 04-schema.md, "Exclude files from classmaps"; `AutoloadGenerator.php:324-353`. The runtime PSR-4 lookup ignores it. **[cited]** `ClassLoader.php:492-559` reads no exclusion. Symfony's root lists `**/Tests/`, yet its tests stay PSR-4-loadable through `"Symfony\\Component\\Console\\": ""`. **[measured]**
- Optimized and authoritative dumps (`-o`, `-a`) turn PSR rules into a class map, and an authoritative map loads nothing else. **[cited]** autoloader-optimization.md, levels 1 and 2/A. A class whose file breaks PSR-4 is then "Skipping"ed. **[cited]** `ClassMapGenerator.php:221-280`. `include-path` and `target-dir` are deprecated. **[cited]** 04-schema.md.
- Measured, production class declarations outside any PSR-4 path: all of WordPress's 865 (no Composer autoload for core; its bundled Requests library registers its own loader, so 750 `use` items name repository classes no rule finds) and PHPUnit's 1,429; elsewhere few (Laravel 61 in `types/`, Symfony 22 in `.github/`). **[measured]**

**What this means.** PSR-4 is resolvable by text, but by Composer's walk, not by "the longest prefix": try every prefix from the longest down and every folder of each, and take the files that exist. Two distinct existing files for one name is the `Names<T>` ambiguous case; none was seen. Matching must be exact-case, as on Linux. `classmap` needs a class-declaration index over its folders; without it, classmap-only repositories (PHPUnit) and custom autoloaders (WordPress) resolve nothing. **[judgement]**

## 5. `autoload-dev` and tests

- `autoload-dev` is root-only, and is meant for "Classes needed to run the test suite". **[cited]** 04-schema.md, "autoload-dev". Composer merges it only for the root package, and only in dev mode. **[cited]** `AutoloadGenerator.php:1290-1292`.
- It does not mark tests reliably, in either direction. **[measured]**
  - Symfony's tests live in `src/**/Tests/` and load through the production PSR-4 prefixes: 6,398 of its 6,470 test-folder files are outside `autoload-dev`. Its `autoload-dev` maps `Symfony\Bridge\PhpUnit\`, which is 37 production files.
  - Filament maps `Filament\Tests\` to `tests/src` under `autoload`, not `autoload-dev`.
  - PHPUnit's `autoload-dev` is a `classmap` and a `files` list.
  - Laravel (`Illuminate\Tests\` → `tests/`) and Doctrine match it exactly, Composer all but 5 files.
- A production file can import a class under a test folder. Laravel's `GeneratorCommand` uses `src/Illuminate/Console/Concerns/CreatesMatchingTest.php`, and Symfony's `KernelBrowser` uses `src/Symfony/Bundle/FrameworkBundle/Test/TestContainer.php`. **[measured]**

**What this means.** Read `autoload` and `autoload-dev` alike for resolution, so a production import of a dev-mapped class still resolves. Decide test files by path, not by which section maps them. **[judgement]**

## 6. `require` and `include`

- `include`, `include_once`, `require` and `require_once` take any expression and may sit anywhere, inside functions and conditions too; parentheses are optional. **[cited]** `parser.y:1704-1715`; manual, "include".
- Resolution. "If a path is defined — whether absolute … or relative to the current directory (starting with . or ..) — the include_path will be ignored altogether." Otherwise PHP tries each `include_path` entry, then "the calling script's own directory and the current working directory". **[cited]** manual, "include". The code: paths starting `./`, `../` or `/` go straight to `realpath`, against the working directory; others try each `include_path` entry, then the folder of the file being executed. **[cited]** `fopen_wrappers.c:522-664`, reached through `main.c:1687-1689`. `include_path` defaults to `".;/path/to/php/pear"` per the manual's table, and `.` is the working directory. **[cited]** `ini.core.xml`.
- So `require './x.php'` is relative to the **working directory**, not to the file. A bare `'x.php'` finds the file's own folder only after the `include_path` misses. Neither is static. **[judgement]**, from the code above. Laravel's tests use the bare form (`require_once 'Fixtures/Enums.php'`, 20 sites). **[measured]**
- `__DIR__` is "The directory of the file, equivalent to `dirname(__FILE__)`. It has no trailing slash unless it is the root directory." `__FILE__` is the full path "with symlinks resolved". **[cited]** manual, "Magic constants". `dirname(string $path, int $levels = 1)` "operates naively on the input string". **[cited]** manual, `dirname`. So these are static: `__DIR__ . '/x.php'`, `dirname(__FILE__) . '/x.php'`, `dirname(__DIR__) . '/x.php'`, `dirname(__DIR__, n) . '/x.php'`, `dirname(__FILE__, n) . …` and `dirname(dirname(__FILE__)) . …`, each also with `\dirname`. `realpath(…)` around one only normalises it. **[judgement]** Two traps: a literal without a leading `/` (`__DIR__ . 'x.php'`) gives `<dir>x.php`, and a symlinked file's `__DIR__` is its target's folder. **[judgement]**
- Measured, `require`/`include` expressions in production files:

| Repository | `__DIR__ . '…'` | `dirname(__FILE__) . '…'` | `dirname(__DIR__[, n])` and nested | other with `__DIR__` | dynamic | static ones that hit a tracked file |
|---|---|---|---|---|---|---|
| WordPress/wordpress-develop | 149 | 18 | 23 | 3 | 1,281 | 190 of 190 |
| sebastianbergmann/phpunit | 21 | 0 | 0 | 21 | 12 | 19 of 21 |
| symfony/symfony | 10 | 0 | 2 | 1 | 47 | 7 of 12 |
| laravel/framework | 0 | 0 | 0 | 1 | 29 | 0 |
| filamentphp/filament | 4 | 0 | 0 | 0 | 5 | 2 of 4 |

  The rest go to `vendor/` (untracked) or name generated files. PHPUnit's "other" 21 are in its tracked `tools/.phpstan/vendor/`. **[measured]** WordPress's dynamic ones are mostly `require_once ABSPATH . WPINC . '/…'` (about 1,100 sites start `require ABSPATH .` or `require_once ABSPATH .`): constants set by `define()` at run time. **[measured]**

**What this means.** The two starting forms are safe, and so are `dirname(__DIR__, n)`, nested `dirname` calls, `\dirname` and a `realpath()` wrapper. `./x` and bare literals are not. In Composer-autoloaded code, requires are rare in production; WordPress-style code is all requires, through constants a text reader cannot evaluate. **[judgement]**

## 7. What `use`-only reading cannot see

- **Same-namespace use.** An unqualified class name with no import "the current namespace is prepended". **[cited]** manual, "Name resolution rules".
- **Qualified and fully qualified names inline.** `\A\B\C`, `Sub\Name` (relative to the current namespace) and `Alias\Sub\Name` (through an imported namespace) need no `use` of the class. **[cited]** same section.
- **Dynamic names.** `new $class`, `$class::m()`, `'A\B'` strings and `A::class` in config: imports "do not affect dynamic class … names". **[cited]** manual, "Aliasing/Importing".
- Measured, production file pairs that a rough body scan found and no `use` or static require gave: references after `new`, `extends`, `implements`, `instanceof`, `insteadof`, before `::`, in `#[`, and trait `use` in class bodies. Type hints were not scanned. **[measured]**

| Repository | `use` pairs | same namespace | … in another folder | relative qualified | through an imported namespace | fully qualified |
|---|---|---|---|---|---|---|
| symfony/symfony | 17,321 | 3,631 | 5 | 323 | 346 | 12 |
| laravel/framework | 4,234 | 958 | 25 | 77 | 4 | 149 |
| filamentphp/filament | 4,437 | 283 | 9 | 466 | 127 | 30 |
| doctrine/orm | 895 | 451 | 1 | 126 | 100 | 0 |
| composer/composer | 1,378 | 363 | 0 | 29 | 38 | 1 |

**What this means.** Under PSR-4 a namespace is a folder, so same-namespace pairs almost never leave their folder, and rarely their part. The pairs that can cross parts are the qualified ones: relative, through an imported namespace, and fully qualified, a few hundred per large repository. All three are text-resolvable with the file's import table, by a body scan of class-reference positions. Without it they are a silent under-count, and a link resting only on them fails. **[judgement]**

## 8. Test files

- PHPUnit: "If you point the PHPUnit command-line test runner to a directory it will look for `*Test.php` files." **[cited]** organizing-tests.rst. A `<testsuite>`'s `<directory>` defaults to the suffix `Test.php`, overridable by `suffix` and `prefix`, and `<file>` adds single files. **[cited]** `Loader.php:1397-1420`; xml-configuration-file.rst, "The `<testsuite>` Element". `.phpt` files are tests too. **[cited]** textui.rst. The documentation's examples use `tests/unit` and `tests/integration`. **[cited]** organizing-tests.rst.
- Pest: a `tests` folder with `Unit` and `Feature`, a `TestCase.php` and a `Pest.php` configuration; "Typically, test files are suffixed with `Test.php`"; `phpunit.xml` configures it too. **[cited]** writing-tests.md, configuring-tests.md. Pest test files are function calls (`test(…)`, `it(…)`) with no class. **[cited]** writing-tests.md, "Your First Test".
- Measured: a `tests`/`test` folder (any case) holds the test code of all seven repositories: 1,371 files in Laravel, 6,470 in Symfony (`Tests/` inside `src/`), 3,003 in PHPUnit. A `Test.php` suffix outside such a folder marks production code: Laravel's `CreatesMatchingTest.php` and `Attributes/UnitTest.php`, PHPUnit's `src/Framework/Attributes/Test.php`. Fixtures that aren't tests by name sit in test folders: `Fixtures/`, `_files/`, `stubs/`, fixture `vendor/` trees in Composer's and PHPUnit's tests. PHPUnit tracks 206 `phpunit.xml` files, Symfony 208. **[measured]**
- Measured traps outside test folders: Laravel's `types/` (static-analysis assertions, 61 declarations off PSR-4 and 5 doubled names), Filament's `packages/*/.stubs.php` (one name in three files), PHPUnit's tracked tool installs (`tools/.phpstan/vendor/`, `tools/.php-cs-fixer/vendor/`, `build/scripts/phar-set-timestamps/vendor/`, each with Composer's `ClassLoader`). **[measured]**
- Blade templates (`*.blade.php`) are not PHP until compiled. Laravel's `@use` directive imports a class from one. **[cited]** laravel/framework `321caf77d`, `src/Illuminate/View/Compilers/Concerns/CompilesUseStatements.php:13`, and `src/Illuminate/View/Factory.php:66` (`'blade.php' => 'blade'`). Filament tracks 205 Blade files, 73 of its package views with `@use` or `@php`. **[measured]**

## 9. A trial repository

| Repository | Licence | Commit | Tracked / `.php` / under a test folder | `composer.json` | Autoload | Fit |
|---|---|---|---|---|---|---|
| laravel/framework | MIT | `321caf77d` (`13.x`) | 3,532 / 3,209 / 1,371 | 40 (root + 39 components) | PSR-4, arrays, `files` | recommended |
| filamentphp/filament | MIT | `1eb10d5fb` (`4.x`) | 8,723 / 6,215 / 864 | 18 | PSR-4, prefix fall-through, `files` | a good second run for traps |
| symfony/symfony | MIT | `922264783` (`8.2`) | 17,017 / 13,197 / 6,470 | 216 | PSR-4 per component, `exclude-from-classmap` | large; tests inside `src/` |
| doctrine/orm | MIT | `f7e7c1ac9` (`3.8.x`) | 1,712 / 1,529 / 1,046 | 2 | PSR-4, one package | one part |
| composer/composer | MIT | `c4899ab08` (`main`) | 1,151 / 653 / 330 | 34 (33 fixtures) | PSR-4 | one part |
| sebastianbergmann/phpunit | BSD-3-Clause | `992ac2e20` (`main`) | 7,358 / 4,470 (+1,974 `.phpt`) / 3,003 | 4 | `classmap` only | rejected unless `classmap` is read |
| WordPress/wordpress-develop | GPL-2.0 | `90582b1af` (`trunk`) | 6,956 / 3,874 / 1,840 | 2 | none for core | rejected: requires through constants |

Licences and default branches from `gh api repos/<r>`, 2026-11-26. **[cited]** Counts **[measured]** on the clones.

**Recommendation: laravel/framework. [judgement]**
- Its 38 component folders under `src/Illuminate/` are natural parts with many cross-component `use` lines: 4,234 production pairs, 4,235 of 6,413 class `use` items resolved.
- It exercises an array prefix (`Illuminate\Support\` over four folders) beside its parent prefix, per-component `composer.json` files that agree with the root, `files` helpers reached by `use function`, a `tests/` tree under `autoload-dev`, and production `*Test.php` names.
- Filament is the better second run: prefix fall-through (55 resolutions), `Filament\Tests\` in `autoload`, Pest, templates inside class files, `<?php // … ?>`, Blade, and `.stubs.php` duplicates.

## What this means for the extractor

Rules, all **[judgement]** built on the citations above:

1. **The reader.** Lex as §1: HTML mode until `<?php`+whitespace or `<?=` (any case); `?>` leaves PHP mode, also from inside a `//` or `#` comment; `#[` is an attribute; strings with a `{$`/`${` stack; heredoc and nowdoc with PHP 7.3's closing rule; stop at `__halt_compiler`. A bare `<?` is not code (the shipped ini files' setting). Keep brace depth across HTML runs.
2. **Statements read.** Track `namespace` declarations (both forms, several per file, imports reset at each) and read `use` only at namespace top level, not followed by `(`, with the grammar of §3: comma lists, aliases, group and mixed group, `function` and `const`. Read `require`/`include` anywhere, unless a `->`, `?->` or `::` comes before.
3. **The autoload index.** Read every tracked `composer.json` outside `vendor/` and test folders: `autoload` and `autoload-dev`, `psr-4` (string or array, `""` as fallback, leading `\` stripped) and `psr-0`. Paths are relative to the `composer.json`'s folder. A prefix without its trailing `\` is a Problem, as Composer refuses it. Malformed JSON is a Problem.
4. **Class resolution.** For a class `use` of `A\B\C`, walk Composer's order: each registered prefix from the longest down, each of its folders, then the fallbacks, then PSR-0, collecting every tracked `<dir>/<rest>.php` that exists, matched with exact case. One file gives a pair. Two distinct files give Unsure through `Names<T>`. None gives no pair: an external class, a namespace import, or a class only a classmap or custom loader finds.
5. **`use function` and `use const`.** Not through PSR-4. Either skip them (an under-count, mostly of helper files), or index function declarations, nested ones included, in the files `files` entries name.
6. **Requires.** Pair only the static forms of §6: `__DIR__`, `dirname(__FILE__)`, `dirname(__DIR__[, n])`, `dirname(__FILE__, n)`, nested `dirname`, `\dirname`, and a `realpath()` around any of them, each joined to string literals with `.`, the joined path normalised against the file's folder. Skip `./`, `../`, bare and absolute literals and everything dynamic.
7. **Tests.** A file under a folder named `tests` or `test` (any case) is a test file. A `*Test.php` file elsewhere is production. `.phpt` files are not read. `*.blade.php` files are not read as PHP.
8. **Detection.** `php-imports` could read a repository with a tracked, non-ignored `.php` file outside a test folder and at least one tracked `composer.json` with a `psr-4` or `psr-0` table.

Stated counts:

- **Over-counts.** A `use` that only a docblock or nothing in the code uses (hundreds per large repository). A pair to a PSR-4 file that does not declare the class, since Composer would load it too.
- **Under-counts.** Same-namespace use. Inline relative, alias-qualified and fully qualified names (§7). Dynamic names. Classes found only by `classmap` or a custom autoloader. `use function` and `use const` targets (unless rule 5 indexes them). Non-static requires. `@use` in Blade. Imports from test files.
- **Unsure.** A class name two tracked files satisfy through the autoload index.
- **Exact.** A class `use` that Composer's lookup finds in one tracked file with the case written.

## Against the starting points

- *"`use A\B\C;` (and group `use`) resolve through the longest `autoload.psr-4` / `autoload-dev.psr-4` prefix of a tracked `composer.json`"*: right in shape, wrong in rule. Composer walks from the longest prefix down and falls through when a prefix's folders lack the file, which Filament needs 55 times (§4). Arrays of folders and the empty fallback prefix are part of the rule. A class `use` may also name a namespace, which resolves to no file. Match exact case. Group use is easy grammar but rare (2 sites in seven repositories).
- *"`require`/`include` are read only as `__DIR__ . '/x.php'` or `dirname(__FILE__) . '/x.php'`"*: right that these are safe and `./x` is not (it is cwd-relative). `dirname(__DIR__)`, `dirname(__DIR__, n)`, nested `dirname`, `\dirname` and a `realpath()` wrapper are equally static and in use (§6).
- *"`classmap` and `files` autoloading are not read"*: a valid choice with a named cost. Without `classmap`, PHPUnit resolves 0 of its 4,288 production class `use` items. Without `files`, `use function` reaches nothing.
- *"verify: `autoload-dev` against skipped tests"*: `autoload-dev` is not a test marker. Symfony's tests load through `autoload` and its `autoload-dev` maps production code; Filament maps tests in `autoload`. Read both sections for resolution and judge tests by path (§5).
- *"verify: `use function` and `use const`"*: they parse like class imports, but PHP 8.6 cannot autoload functions or constants, so PSR-4 never maps them. Most name built-in globals (§3).
- *"the lexer handles code only inside `<?php … ?>` / `<?=`, heredoc and nowdoc, and `#[` attributes (not comments)"*: right, plus three traps: `?>` ends a `//` or `#` comment and PHP mode (207 sites in WordPress), `{$ … }` interpolation nests code with strings inside strings, and `__halt_compiler` ends the code. Short tags depend on server config (§1).
- *"tests: `tests/` and `*Test.php`"*: the folder rule is right if it includes `Tests` and `test`; the suffix rule is wrong outside a test folder, where `*Test.php` names production classes in Laravel and PHPUnit (§8).

## Open questions for the grill

1. **`classmap`.** Index class declarations in `classmap` folders (Composer's own scanner is text-only), or leave classmap-only repositories unread? PHPUnit is the case.
2. **`use function` and `use const`.** Skip them, or index function declarations in `files` entries? Helpers are often declared inside `if (! function_exists(…))`.
3. **Qualified names in the body.** Keep the inline relative, alias-qualified and fully qualified references a silent under-count, or add a body scan of class-reference positions, which the import table makes exact (§7)?
4. **Which `composer.json` files.** Every tracked one outside `vendor/` and test folders, or the root plus those a path repository names? The measured repositories agree either way.
5. **Bare `require 'x.php'`.** Skip it, or pair it with the file's own folder when that file exists, a stated over-count that depends on `include_path`?
6. **Short tags.** Treat a bare `<?` as HTML (the shipped ini files), or as code?
7. **Test folders.** Is `tests`/`test` in any case enough, or should `Fixtures/`, `_files/` and `stubs/` outside test folders, `types/`, and `phpunit.xml` `<directory>` entries count too?
8. **The trial repository.** laravel/framework first, and Filament or Symfony second?
