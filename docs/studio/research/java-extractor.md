# Java extractor: the rules a text-only `java-imports` must follow

Research notes for issue #539. They set out how a text-only `java-imports` extractor turns a Java repository's imports into `(dependent file, dependency file)` pairs over its tracked files, without running `javac`, Maven or Gradle and with no classpath. They extend the "Java" section of `language-extractors.md` and the Go pilot's note, `go-extractor.md`, and don't repeat either. Studio terms follow `docs/studio/CONTEXT.md`. The extractor contract is `docs/studio/behaviour/map.md`, "The extractors", and spec #529 (Problem / Unsure / Pair, `Names<T>`).

## How this was read

- Language rules come from the Java Language Specification, Java SE 27 Edition, the latest one: "Released September 2026 as JSR 402" (https://docs.oracle.com/javase/specs/). Chapters 3, 6, 7, 8 and 9 were fetched with `curl` on 2026-11-25 and read as text, so quotes are verbatim. A link like JLS §7.6 means https://docs.oracle.com/javase/specs/jls/se27/html/jls-7.html#jls-7.6. Chapter 7 of SE 25 was also fetched; SE 25 is the first edition with the final module imports and compact source files (below).
- JEPs were read at https://openjdk.org/jeps/<n> with `curl`: their status and release fields, and the text of JEP 511 and JEP 512.
- `javac` was read at `openjdk/jdk` `master` (`c1e50d63f`) on 2026-11-25, through raw.githubusercontent.com, under `src/jdk.compiler/share/classes/com/sun/tools/javac/`: `parser/JavacParser.java`, `comp/Enter.java`, `comp/TypeEnter.java`, `code/ClassFinder.java`, `main/JavaCompiler.java`, `resources/compiler.properties`. Line numbers will drift.
- Build-tool layout comes from Maven's "Introduction to the Standard Directory Layout", the POM reference and the `maven-compiler-plugin` 3.16.0 `compile` goal, and from the Gradle 9.9.2 user manual ("The Java Plugin", "JVM Test Suite Plugin", "Testing in Java & JVM projects"), through WebFetch. The fetch tool summarises pages, so quotes are as it returned them.
- Six public repositories were shallow-cloned (`--depth 1`) into a scratch folder, not this repository: google/gson `d9b5079`, mockito/mockito `10b31ba`, apache/commons-lang `dfc6c4c`, google/error-prone `8041eff`, junit-team/junit-framework `06d45ac`, google/guava `c9977bc`. Nothing was built or run. A throwaway Python scan (a rough lexer for comments, strings and text blocks, then the package line, imports and depth-0 type declarations) counted what §7 reports.
- Each claim is tagged **[cited]** (with its URL and section, or a source file and line), **[measured]** (counted by that scan, rough: it does not model shadowing, and its counts are for scale, not exact), **[repo]** (checked in this repository) or **[judgement]** (my own reading, not checked against a source).

## 1. Compilation units and the type index

### What a file holds

- There are three kinds of compilation unit. `OrdinaryCompilationUnit: [PackageDeclaration] {ImportDeclaration} {TopLevelClassOrInterfaceDeclaration}`, `CompactCompilationUnit: {ImportDeclaration} {ClassMemberDeclarationNoMethod} MethodDeclaration {ClassMemberDeclaration}`, and `ModularCompilationUnit: {ImportDeclaration} ModuleDeclaration`. **[cited]** JLS §7.3. So imports always come after the package line, if any, and before every declaration.
- `PackageDeclaration: {PackageModifier} package Identifier {. Identifier} ;`, where a `PackageModifier` is an annotation. "The package name mentioned in a package declaration must be the fully qualified name of the package". **[cited]** JLS §7.4.1. javac rejects package annotations outside `package-info.java` (`Errors.PkgAnnotationsSbInPackageInfoJava`). **[cited]** `Enter.java:339-341`.
- An ordinary unit with no package line, and every compact unit, is in an unnamed package. **[cited]** JLS §7.4.2. A single-type import needs a type that is "a member of a named package" (or nested in one). **[cited]** JLS §7.5.1. So a type in an unnamed package can never be imported. **[judgement]**, from that sentence.

### Does `Type.java` have to match?

- The folder: JLS §7.2 leaves storage to the host. "Each host system determines how modules, packages, and compilation units are created and stored." Turning `jag.scrabble.board` into `jag/scrabble/board` is "an extremely simple example", not a rule. **[cited]** JLS §7.2. I found no javac diagnostic for a package line that disagrees with its folder in `compiler.properties`. **[cited]** by absence, medium confidence. javac only complains when it *looks up* a class on the source path in the conventional file and doesn't find it: "file does not contain class {0}", "Please remove or make sure it appears in the correct subdirectory of the sourcepath." **[cited]** `JavaCompiler.java:880-884`, `compiler.properties:2730-2733`, `:2826-2827`. Maven passes source files to javac explicitly ("Excluding a source file only prevents the plugin from passing it explicitly to the compiler"). **[cited]** maven-compiler-plugin 3.16.0, `compile`, `excludes`. So a misplaced file still builds under Maven. **[judgement]**, medium confidence.
- The file name: "If and only if packages are stored in a file system (§7.2), the host system *may choose to enforce* the restriction that it is a compile-time error if a class or interface is not found in a file under a name composed of the class or interface name plus an extension" when it is referred to by code in other compilation units of its package, or is `public`. "This restriction implies that there must be at most one such class or interface per compilation unit." **[cited]** JLS §7.6 (italics mine).
- javac enforces only the `public` half: "{0} {1} is public, should be declared in a file named {1}.java". **[cited]** `Enter.java:437-446` (`classNameMatchesFileName`), `compiler.properties:1597-1598`. A non-public top-level type may sit in any file, beside others. **[cited]**, the same code checks only `PUBLIC`.
- Measured: across the six repositories, no public top-level type was in a file of another name, and files with more than one top-level type were rare (1, 2, 4, 3, 15 and 1 files). Files whose folder does not end with their package path were 0 everywhere except error-prone (120, all fixtures under `core/src/test/java/.../testdata/`) and junit (2, under `src/test/resources/`). **[measured]**

**What this means.** The starting point holds and is a little stronger: index each file's `package` line and *every* top-level type it declares, public or not. Don't guess from paths. A non-public second type in `Foo.java` is then found too, which a path guess would miss. **[judgement]**

### Declaration kinds

- `ClassDeclaration: NormalClassDeclaration | EnumDeclaration | RecordDeclaration`. **[cited]** JLS §8.1. `class TypeIdentifier`, `enum TypeIdentifier`, `record TypeIdentifier [TypeParameters] RecordHeader`. **[cited]** JLS §8.1, §8.9, §8.10.
- `InterfaceDeclaration: NormalInterfaceDeclaration | AnnotationInterfaceDeclaration`, the second being `{InterfaceModifier} @ interface TypeIdentifier`. "the at sign (@) and the keyword interface are distinct tokens. It is possible to separate them with whitespace". **[cited]** JLS §9.1, §9.6. So `@ interface X` is legal. A scanner that matches `@interface` as one word misses it. **[judgement]**
- `class`, `interface` and `enum` are reserved keywords. `record`, `sealed`, `non-sealed`, `permits` and `module` are contextual keywords: `record` is a keyword only "when recognized as a terminal in a RecordDeclaration", and `non-sealed`, `permits` and `sealed` only in a normal class or interface declaration. **[cited]** JLS §3.9. A `TypeIdentifier` can't be `permits`, `record`, `sealed`, `var` or `yield`. **[cited]** JLS §3.8. javac reads `record` as a declaration when the next token is an identifier. **[cited]** `JavacParser.java:5010`. So at depth 0 a reader can take `record <Ident>` as a declaration, since no other depth-0 construct has that shape. **[judgement]**
- `sealed` / `non-sealed` are modifiers; `permits` names subclasses by `TypeName`. In an unnamed module "every class specified in the permits clause … must belong to the same package". **[cited]** JLS §8.1.1, §8.1.6. They add no new kind to index. A `permits a.b.C` name is a qualified type use outside an import (§3 below). **[judgement]**
- Records and sealed classes are final since JDK 16 and 17. **[cited]** JEP 395 and JEP 409, status "Closed / Delivered", release 16 and 17.
- Measured: records in error-prone (31) and junit (28), sealed classes in junit (3), `@interface` in all six. **[measured]**

### Compact source files

- A compact compilation unit "implicitly declares a top level class". Its name "is a valid identifier … determined by the host system", typically "the name of the file … minus any extension". It "is a member of an unnamed package". **[cited]** JLS §8.1.8. javac errors on a package line in one (`Errors.ImplicitClassShouldNotHavePackageDeclaration`). **[cited]** `JavacParser.java:4200`.
- The feature went through JEP 445 (preview, 21), 463 (22), 477 (23), 495 (24) and was finalised as JEP 512, "Compact Source Files and Instance Main Methods", in JDK 25. **[cited]** openjdk.org/jeps, status and release fields.
- Every compact unit implicitly has `import module java.base;`. **[cited]** JLS §7.3; `TypeEnter.java:330-336`.

**What this means.** A compact file is never a dependency target: it is in an unnamed package. It can be a dependent through its imports. Telling it from an ordinary file needs a top-level method or field, which a depth-0 scan sees. **[judgement]**

### `package-info.java` and `module-info.java`

- `package-info.java` holds the package's sole annotated package declaration, "Typically … only a package declaration, preceded immediately by the annotations on the package". **[cited]** JLS §7.4.1. It may hold imports, including module imports. **[cited]** JLS §7.5.5, Example 7.5.5-1. It declares no type. **[cited]** §7.4.1: "package-info is not a legal identifier".
- `module-info.java`: `{Annotation} [open] module Identifier {. Identifier} { {ModuleDirective} }`, with `requires`, `exports`, `opens`, `uses TypeName` and `provides TypeName with TypeName`. A host "may choose to enforce" the `module-info` file name. **[cited]** JLS §7.7. Its `uses` and `provides` name types by qualified name, with no import needed. **[cited]** §7.7, Example 7.5.5-2.
- Measured: junit has 21 `module-info.java`, the others 0 to 3. **[measured]**

## 2. Import declarations

- Five kinds. **[cited]** JLS §7.5.
  - `import TypeName ;`, single-type. "The TypeName must be the canonical name of a class or interface". **[cited]** §7.5.1.
  - `import PackageOrTypeName . * ;`, type-import-on-demand. The name "must be the canonical name … of a package, a class, or an interface". **[cited]** §7.5.2. So `import a.b.C.*` may import C's member types, not a package.
  - `import static TypeName . Identifier ;`, single-static. **[cited]** §7.5.3.
  - `import static TypeName . * ;`, static-import-on-demand. **[cited]** §7.5.4.
  - `import module ModuleName ;`, single-module. **[cited]** §7.5.5.
- In every kind but the module import, the part before the member name or `*` is a canonical type name, or for `.*` without `static` possibly a package. **[cited]** the grammars above. So a static import, single or on demand, always names one type, and its pair is that type's file. **[judgement]**
- Canonical names: a member type's canonical name is its enclosing type's canonical name plus its own. **[cited]** JLS §6.7. Member types are declared inside the enclosing type's body, so `import a.b.Outer.Inner` and `import static a.b.Outer.Inner.X` are in `Outer`'s file. **[judgement]**, from §8.5 and §6.7. An inherited member type is *not* reached by its canonical name through a subclass (`p.O2.I` is a fully qualified name, "only p.O1.I is its canonical name"). **[cited]** Example 6.7-2. So walking to the outermost type is exact.
- Where imports sit: only after the package line and before any type (§7.3). javac reads `import` only while it has seen no type declaration. **[cited]** `JavacParser.java:4097` (`firstTypeDecl && … token.kind == IMPORT`). Extra `;` between imports is an error since source 21. **[cited]** `JavacParser.java:4084-4103`. A reader can stop at the first token that is not `import` or `;`.

### Package segment or type segment?

- The language does not tell them apart by case: "The Java programming language does not actually rely on case distinctions to determine whether a name is a package name or a type name." **[cited]** JLS §6.4.2. Capitalisation is only a convention. **[cited]** JLS §6.1, "Naming Conventions".
- javac decides by membership. A qualified `Q.Id` is "reclassified as a TypeName" if "the class, interface, type parameter, or package denoted by Q has a member class or interface named Id", and otherwise as a package. **[cited]** JLS §6.5.4.2. javac resolves the import's qualifier, then the type, per import. **[cited]** `TypeEnter.java:412-445` (`doImport`).
- "A package may not contain two members of the same name": a package can't hold both a subpackage and a type named `image`. **[cited]** JLS §7.1. So, within one compiled program, the walk is unambiguous. **[judgement]**

**What this means.** The type index gives the same answer as javac, with no case guess. For `a.b.c.D.E`, try each split from the left: the first prefix `p` that the index knows as a package with a top-level type named by the next segment gives that type's file, and the rest are member names. A name that hits nothing names a JDK or third-party type and gives no pair. **[judgement]** Measured: most single-type imports that hit the index reached a top-level type, and 74 (gson), 15 (mockito), 39 (commons-lang), 1412 (error-prone), 329 (junit) and 934 (guava) imports reached a top-level type with nested names left over. So the walk-up matters. **[measured]**

### Implicit and module imports

- "Every compilation unit implicitly imports every public class or interface declared in the predefined package java.lang". **[cited]** JLS §7.3; `TypeEnter.java:319-328`. It names only JDK types, so no pair. **[judgement]**
- An on-demand import of `java.lang` or of the unit's own package "is ignored". **[cited]** JLS §7.5.2.
- Module imports: preview in JEP 476 (JDK 23) and JEP 494 (24), final in JEP 511, JDK 25. **[cited]** openjdk.org/jeps. One imports "all the public top level classes and interfaces" of every package the module exports to the current module, and of every package exported by modules it reads "due to reading the module" (`requires transitive`). **[cited]** JLS §7.5.5. It is usable in any file, modular or not. **[cited]** same section. Shadowing goes by specificity: single-type beats on-demand, which beats module imports. **[cited]** JEP 511, "Ambiguous imports".
- Telling `import module x;` from `import module.x.Y;` is lexical: `module` is a keyword only "in a SingleModuleImportDeclaration". **[cited]** JLS §3.9. javac takes it as a module import only when the token after `module` is an identifier. **[cited]** `JavacParser.java:4339-4347`.
- Measured: 4 module imports in the six repositories, all in junit's tooling fixtures. **[measured]**

**What this means.** Resolving `import module M` needs the module index (each `module-info.java`'s name and `exports`), and `requires transitive` followed. It is an on-demand import at module scale: the same over-count as a wildcard, wider. Rare today. **[judgement]**

## 3. What import-only reading cannot see

- **Same-package use.** "Code in a compilation unit automatically has access to all classes and interfaces declared in its package". **[cited]** JLS chapter 7 introduction. Scope: Example 7.6-2 compiles two classes that use each other with no import. **[cited]** JLS §7.6. An import of a type in the unit's own package is allowed and ignored by meaning. **[cited]** JLS §7.5.2, and §7.5.1's rule for a type in the unit itself.
- **Fully qualified use.** A type may be written as `a.b.C` anywhere a type goes, with no import. **[cited]** JLS §7.5: "Without the use of an appropriate import declaration, a reference … would typically need to use a fully qualified name". `extends`, `permits`, annotations, `module-info`'s `uses` and `provides`, and expressions like `a.b.C.method()` all take them.
- **Unnamed packages.** Types there can't be imported (§1). Files in one unnamed package see each other with no import. **[judgement]**
- **Types not in a `.java` file.** A Kotlin or Groovy type in a mixed module, and generated sources (`target/generated-sources/annotations` is the default for annotation processing, **[cited]** maven-compiler-plugin, `generatedSourcesDirectory`), aren't in the index. Generated files are usually untracked anyway. **[judgement]** junit tracks 154 `.kt` files. **[measured]**

Measured, per repository: non-test file pairs from imports (all kinds but package wildcards); pairs where a non-test file names a non-test sibling type of its own package with no import ("same-package", a rough upper bound); those whose two files lie in different source roots (the folder above the package path); and inline qualified names that reach a repository type the file does not import. **[measured]**

| Repository | Non-test `.java` | Import pairs | Same-package pairs | … across source roots | Inline FQN sites |
|---|---|---|---|---|---|
| google/gson | 123 | 425 | 130 | 0 | 0 |
| mockito/mockito | 528 | 1492 | 327 | 8 | 9 |
| apache/commons-lang | 264 | 243 | 270 | 0 | 3 |
| google/error-prone | 1076 | 6884 | 1653 | 473 | 2 |
| junit-team/junit-framework | 1059 | 3124 | 1272 | 20 | 100 |

Guava is left out of the table: its tests aren't under `src/test/` and every main type has an `android/` twin (§5), so its counts mean nothing here.

**What this means.** Same-package pairs are a large share of all file pairs (a third in junit, more than the import pairs in commons-lang). But a package is usually one folder in one part, so most of them would be dropped as pairs inside one part. **[judgement]** The ones that matter cross source roots: a **split package**, one package declared in two modules. error-prone's `com.google.errorprone.bugpatterns` is in `core` (502 files) and in `check_api` (1 file, `BugChecker.java`), and 625 files in `core` extend `BugChecker`, many without an import. **[measured]** junit and mockito split packages between `src/main/java` and `src/testFixtures/java`. **[measured]** spec #529 notes an under-count fails a true link between two read parts (decisions, Round 3). So a link resting *only* on same-package or inline-FQN use would fail. In error-prone, `core` also imports `check_api` in 744 files, so the part edge still shows. **[measured]**

## 4. Lexing traps

- Unicode escapes are translated before anything else: "A compiler … first recognizes Unicode escapes in its raw input, translating the ASCII characters `\u` followed by four hexadecimal digits". `\uuuu005c` style repeated `u`s are allowed, and an escaped backslash counts toward the even/odd rule. **[cited]** JLS §3.3; javac `UnicodeReader.java:211`, `:279`. So `import` is `import`, and `\u000a` inside a `//` comment ends the comment. A reader that skips this can be fooled. **[judgement]** Measured: no package or import line with `\uXXXX` in about 9,100 files. **[measured]**
- Comments: `/* */` and `//`. "Comments do not nest", and "comments do not occur within character literals, string literals, or text blocks". **[cited]** JLS §3.7.
- Text blocks: opened by `"""`, then optional space, tab or form feed, then a line terminator, closed by `"""`. Escapes may appear inside, so `\"""` doesn't close one. **[cited]** JLS §3.10.6. A text block can hold `import x.Y;` or `class Fake {` on its own line. error-prone has text blocks in 699 files, mostly test sources holding Java code. **[measured]**
- Before the imports only annotations, the package line and comments can appear, so strings there are only annotation arguments. **[judgement]**, from §7.3 and §7.4.1. The import region itself has no strings at all. The type index scans the whole body at brace depth 0 for declarations, so it needs the full lexer: comments, strings, char literals (`'{'`), text blocks, and `.class` / `.record`-style member names after a `.`. **[judgement]**

## 5. Source layout and test files

- Maven: `src/main/java` "Application/Library sources", `src/test/java` "Test sources", `src/it` "Integration Tests (primarily for plugins)". "if you can't, these settings can be overridden via the project descriptor." **[cited]** Maven, "Standard Directory Layout". The defaults are `<sourceDirectory>${project.basedir}/src/main/java</sourceDirectory>` and `<testSourceDirectory>${project.basedir}/src/test/java</testSourceDirectory>`. **[cited]** Maven POM reference, "Directories". A multi-module build lists modules as "relative paths to the directories or the POM files of those projects", each "laid out as above". **[cited]** POM reference, "Aggregation"; "Standard Directory Layout".
- Gradle: `src/main/java` "Production Java source", `src/test/java` "Test Java source", and `src/sourceSet/java` for any other source set. **[cited]** Gradle, "The Java Plugin", "Project layout". A test suite such as `integrationTest` uses `src/integrationTest/java`. **[cited]** "JVM Test Suite Plugin". The `java-test-fixtures` plugin adds `src/testFixtures/java`. **[cited]** "Testing in Java & JVM projects". Source sets are set in `build.gradle(.kts)`, which is a program. **[cited]** `language-extractors.md`, "Java".
- Overrides are common. Guava's root `pom.xml` sets `<sourceDirectory>src</sourceDirectory>` and `<testSourceDirectory>test</testSourceDirectory>` (lines 151-152), and has no file under `src/test/`. **[measured]** error-prone's `check_api/pom.xml` adds a Java 24 root, `src/main/java24`, compiled into `META-INF/versions/24` (lines 199-208). **[measured]** junit has `src/` roots named `main`, `testFixtures`, `tools`, `processStarter`, `standalone`, `jmh`, `other`, `graalVmTest` and `archUnit`. **[measured]**
- Fixtures: error-prone keeps Java test inputs under `src/test/java/.../testdata/` with packages that don't match their folders. junit keeps them under `src/test/resources/`. **[measured]**

**What this means.** `src/test/` covers both tools' defaults and every repository here but Guava. A text reader can't see `<testSourceDirectory>` overrides or Gradle source sets without parsing build files. **[judgement]**

## 6. One name, two files

- "It is a compile-time error if the name of a top level class or interface appears as the name of any other top level class or interface declared in the same package." **[cited]** JLS §7.6. So two tracked files declaring one fully qualified name are never in one compilation. They are variants: another module, a multi-release root, a platform flavour, or a fixture.
- Measured non-test examples: error-prone's `ErrorProneSignatureGenerator` in `src/main/java` and `src/main/java24`; junit's `com.example.project.Calculator` in two tooling-test projects; Guava, 1,558 names twice, `guava/` and `android/guava/`. For Guava 10,423 single-type imports hit a doubled name. **[measured]**

**What this means.** A fully qualified name with two files is the `Names<T>` ambiguous case: an import of it is Unsure, as spec #529 decides. A wildcard of a package declared in two roots is not Unsure by itself, since each file is a separate pair. **[judgement]**

## 7. A trial repository

| Repository | Licence | Tracked files / `.java` / under `src/test/` | Build | Fit |
|---|---|---|---|---|
| junit-team/junit-framework | EPL-2.0 | 2468 / 1813 / 754 | Gradle, ~20 modules, 21 `module-info.java` | recommended |
| google/error-prone | Apache-2.0 | 2623 / 2108 / 1032 | Maven, 8 modules | a good second run for traps |
| mockito/mockito | MIT | 1150 / 1015 / 487 | Gradle; 510 of 528 non-test files in `mockito-core` | few cross-part edges |
| google/gson | Apache-2.0 | 317 / 268 / 145 | Maven, small | too small |
| apache/commons-lang | Apache-2.0 | 737 / 647 / 383 | Maven, one module | one part |
| google/guava | Apache-2.0 | 3351 / 3265 / 0 | Maven, overridden roots, `android/` twin | rejected: mostly Unsure |

All figures from `gh api repos/<r>/git/trees/<branch>?recursive=1` on 2026-11-25. **[cited]** Module and layout notes are **[measured]**.

**Recommendation: junit-team/junit-framework. [judgement]**
- Its non-test code is spread over about 20 Gradle modules, from `junit-jupiter-api` (209 files) down, so cross-part edges are many.
- It exercises records, sealed classes, `@interface` (150), 21 `module-info.java`, 4 `import module` lines, 8 files with no package line, test fixtures in split packages, 154 Kotlin files beside Java, and 100 inline qualified names.
- Its licence is EPL-2.0, weak copyleft. Reading it for a trial needs nothing more, but it is not the permissive licence the Go note preferred.
- error-prone is the better second run: Maven, a split package across modules (`BugChecker`), a `java24` root that doubles a name, 120 testdata files with mismatched folders, and 699 files with text blocks.

## What this means for the extractor

Rules, all **[judgement]** built on the citations above:

1. **The type index.** Read every tracked `.java` file that is not a test file (rule 7). From its package line take the package (none means an unnamed package). Then scan the body at brace depth 0 for `class`, `interface`, `enum`, `@ interface` and `record <Ident>`, and register each `package.Name` with that file in a `Names<T>`. `package-info.java` and `module-info.java` declare no type. A compact file (a method or field at depth 0) declares none that can be imported.
2. **Resolution.** For a single-type import `a.b.C.D`, a static import's type part, or a `TypeName.*`, walk the dotted name from the left. The first prefix the index knows as a package with a top-level type named by the next segment gives that type's file. No hit gives no pair: the JDK, a library, a generated or Kotlin type. Two files give Unsure.
3. **Wildcards.** `import a.b.*`: if `a.b` is a package in the index, pair with every non-test file declaring a top-level type in it, from every root. That is an over-count, stated. If instead `a.b` resolves as a type (rule 2), pair with that type's file. `import java.lang.*` and the file's own package give nothing.
4. **Module imports.** Either skip `import module` (an under-count) or index `module-info.java` names and `exports` and treat it as a wildcard over each exported package (a wider over-count). Rare in practice.
5. **The reader.** Translate `\uXXXX` escapes first, or treat a file with a `\u` before its first `{` as unreadable for imports. Skip `//`, `/* */`, string, char and text-block literals. Read leading annotations, then `package`, then `import` and stray `;` until the first other token. `import module <Ident>` is a module import, `import module.x` is not.
6. **Same-package and inline qualified use** are invisible, and the doc lists them as the silent under-count. Most such pairs stay inside one part. Split packages across modules are the case that can fail a true link.
7. **Tests.** A file whose path has a `src/test/` segment is a test file: Maven's and Gradle's default. Its pairs are dropped and it never makes a part read. Fixtures under `src/test/java/.../testdata/` and `src/test/resources/` go with it.
8. **Detection.** `java-imports` could read a repository with a tracked, non-ignored `.java` file outside `src/test/`.

Stated counts:

- **Over-counts.** Package wildcards pair with every file of the package, used or not. Module imports, if read, pair with every exported package. An unused import still makes a pair, as in every other extractor.
- **Under-counts.** Same-package use. Inline fully qualified names, `permits`, `extends a.b.C`, `module-info` `uses` and `provides`. Types in unnamed packages. Types declared outside `.java` files (Kotlin, Groovy, generated). Imports from test files and from roots a build file renames to tests.
- **Unsure.** An import of a name two indexed files declare.
- **Exact.** Single-type and static imports of a type declared once in the repository, nested names included: the pair names the file javac would read.

## Against the starting points

- *"an index of each file's `package` line and top-level type names (JLS §7.6 does not force `Type.java` to match the folder)"*: right, and the index should take every top-level type, not only the public one. §7.6 lets a host *choose* to enforce the file name, and javac enforces it only for `public` types. The folder rule is §7.2's, not §7.6's, and javac doesn't enforce it (§1).
- *"single-type, nested (`Outer.Inner` → `Outer`), static and wildcard imports"*: right. Static imports always name a type, so they resolve exactly. `import a.b.C.*` may name a type, not a package. The package/type split needs no case guess: walk the index, as JLS §6.5.4.2 does (§2). A fifth kind, `import module`, is final since JDK 25 (JEP 511).
- *"wildcards go to the whole package, an over-count"*: right, and rare: 24 type wildcards in mockito, 2 in error-prone, 1 in junit, none in gson, commons-lang or Guava. **[measured]**
- *"same-package use is invisible: a silent under-count"*: right, and large in pairs, but mostly inside one part. Split packages are the risk (§3). Inline qualified names are the second source.
- *"verify: `record`, `enum`, `interface` and `@interface` declarations; static wildcard imports"*: all four are top-level type declarations to index. `record` is contextual, and `@` and `interface` are separate tokens (§1). Static wildcards name one type, so they are exact, not an over-count (§2).
- *"tests: under `src/test/`"*: right for Maven's and Gradle's defaults, and for five of six repositories. Wrong for Guava (`test/`), and silent about Gradle's `testFixtures`, `integrationTest` and `jmh` source sets (§5).

## Open questions for the grill

1. **Test roots beyond `src/test/`.** Are `src/testFixtures/`, `src/integrationTest/`, `src/it/` and `src/jmh/` tests? Is any folder named `test` or `tests` at any depth one, as `typescript-imports` does? Should the extractor read `<testSourceDirectory>` from `pom.xml`, which would cover Guava?
2. **Wildcards.** Pair with the whole package (the stated over-count), or narrow to files whose type names appear in the importer's body? Narrowing is still text-only but adds a name scan and its own shadowing risks.
3. **Same-package use.** Keep it a silent under-count, as spec #529 says, or recover it with the same body-name scan, at least where a package spans two roots?
4. **Module imports and `module-info.java`.** Skip `import module` or expand it? Should `requires` lines give manifest-level pairs between `module-info.java` files, as `cargo` does for `Cargo.toml`?
5. **Unicode escapes.** Translate them fully, or treat a file whose header holds `\u` as unreadable (no pairs, or a Problem)? None was seen in the header of about 9,100 files.
6. **Variant roots.** Is a doubled name from a multi-release root (`src/main/java24`) or a platform twin (`android/`) always Unsure, or should a known variant root be dropped from the index?
7. **The trial repository.** junit-framework (EPL-2.0) first, or a permissive one? error-prone as the second?

## After the grill

The grilling of 2026-11-25 changed these rules; spec #590 holds the settled design, and `local-evidence/2026-11-25-java-extractor-grilling/reports/decisions.md` (on Daniel's machine) the rounds.

- **Rule 4, module imports.** Neither skipped nor expanded: `import module M` of a module the repository declares makes the importer Unsure, with the files under M's `module-info.java` folder as candidates. `exports`, `requires transitive` and `requires` pairs are not read. junit's 4 module imports sit in fixture projects under `platform-tooling-support-tests/projects/`, not under a test source root. **[measured]**
- **Rule 5, the reader.** Escapes are translated in full, with no header fallback: 137 of 9,116 files hold valid escapes outside the header, 0 a malformed one. **[measured]** Import-site lines are counted on the raw text. A file the reader cannot lex is dropped whole, with no Problem.
- **Rule 7, tests.** Judged on the source root, the folders above the package path. Under `src/<set>/` only `main` is production: junit's `tools`, `processStarter` and `archUnit` are tooling and tests. **[measured]** A root with no `src/<set>/` shape is a test when it holds a `test` or `tests` folder, which covers Guava.
- **Ignored files.** Files the map ignores are left out of the type index, so a map author settles fixture projects and variant copies by ignoring them, and must ignore a whole variant tree (`android/`), never one module of it: `android/guava-testlib/pom.xml:37-38` builds against `android/guava`. **[measured]**
- **Doubled names** stay Unsure with no tie-break. A "nearest build file" rule fails on junit's `<module>.gradle.kts` files and on Guava's cross-module imports. **[measured]**
- **Rule 7, production sets and the fallback** (review of spec #590). Production sets are `main`, `java` and `java<N>`, not `main` alone: apache/lucene keeps 4,094 `.java` files under `src/java/` and 15 under `src/java25/`, against 83 under `src/main/`, so `main` alone would read it as zero production files. **[measured]** When the package line does not match the folders, or the file has none, the whole folder path stands in for the source root and is judged by both rules (a `src/<set>` that is no production set, else a `test` or `tests` folder), which skips error-prone's mismatched fixtures under `src/test/java/.../testdata/`.
- **Wildcards, no group** (review of spec #590, Q19, reversing round 1's Q2 on the group). `java-imports` sets no `Pair.group`: a package wildcard's pairs are each their own unit and a file node in the imports report, because a group would need node rules for Java that the report does not define (a slice has no importer side for a wildcard). Production files in the six clones hold 0 wildcards of their own repository's packages, against 1,165 to 16,791 single-type imports per repository, so a wildcard's per-file weight is a stated over-weight, not a measured one. **[measured]**
