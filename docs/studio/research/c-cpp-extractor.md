# C and C++ extractor: how far quoted, adjacent includes reach

Research notes for issue #748. They measure, on one C and one C++ repository, how many `#include` lines a text-only reader could turn into file-level `(dependent file, dependency file)` pairs, by each resolution rule, with no compiler and no configure step. They extend the "C and C++" section of `language-extractors.md` (lines 140-148 at 3d44062aa) and don't repeat it. They are input to the grilling of #748, not a design. Studio terms follow `docs/studio/CONTEXT.md`.

Round 1 (curl, leveldb) counted resolution rules. Round 2 ("Round 2: grading the suffix-match rule", below) graded a candidate rule (adjacent first, then a unique path-suffix match over tracked files) include by include against each build's own `-I` roots, on curl, leveldb, bitcoin/bitcoin and redis/redis.

## Summary

All **[measured]** unless marked.

- **Adjacent only** gets 65.8% of curl's quoted includes and 0% of leveldb's (round 1).
- **Suffix match never picked a different tracked file from the one the build picks.** The real file always ends in `/<include path>`, so a single match is that file whenever the real file is in the match pool. Wrong lines come only from headers outside the pool: system, generated, or ignored. **[judgement]**, borne out on all four repositories.
- **Those wrong lines are real.** Most are angle includes of standard or system headers that match a repository file with the same name: redis 273 (145 from first-party files, nearly all `<stdint.h>`, `<math.h>`, `<assert.h>` and `<strings.h>` matching jemalloc's MSVC and test shims), bitcoin 37 (`<string.h>` matching `src/util/string.h`), curl 4 (`<rustls.h>` and `<schannel.h>`), leveldb 0.
- **Unsure is large where a codebase repeats names.** bitcoin has 960 Unsure includes against 8,198 agreed lines, mostly from `src/test/util/` copies of names such as `random.h` and `validation.h`. The other three have 4 or fewer.
- **Angle includes are where the rule breaks.** Taking `<…>` includes only when they resolve under a root the build names (variant iii) removes every wrong line in curl and bitcoin, and turns bitcoin's 928 angle Unsures into 13. Taking them only when the name has a folder part (variant ii) also removes the wrong lines, but drops 1,830 of bitcoin's right lines.
- **Ignoring vendored trees** cuts redis's first-party wrong lines from 145 to 3, and those 3 are new: `<sds.h>` means hiredis's copy, which is now ignored.
- **The "closest candidate" tie-breaker is worse than leaving the include Unsure.** On bitcoin it made 241 wrong lines and 56 right ones, and 663 stayed tied, because angle includes never search the including file's folder.
- **Speed:** 1.4 s in Python on bitcoin (21 MB of C/C++), 88% of it in a character-by-character comment stripper.
- **Round 3** starts from never matching system names, with vendored trees ignored. That leaves 9 wrong lines in all four repositories together.
  - Skipping test files cuts bitcoin's Unsures from 878 to 206 and costs no right line in a non-test file except 51 from `src/bench/` into test helpers.
  - Letting ignored files block a match removes redis's 3 remaining wrong lines.
  - Dropping the including file's own folder for angle includes fixes curl and redis but costs bitcoin 1,117 right lines and adds 109 wrong ones.
- **Round 4:** bitcoin's 324 remaining Unsures cost no part-to-part link on a proxy map. Each pair they touch also has a right line.
  - A single declared root (`src`) or roots inferred from unique matches settle all 324 Unsures correctly.

## How this was read

- The include search rule comes from the GCC CPP manual, "Search Path" (https://gcc.gnu.org/onlinedocs/cpp/Search-Path.html): for `#include "file"` the preprocessor searches "first relative to the directory of the current file", and `-I` directories are searched "after the current directory" and "ahead of the standard system directories". `<file>` by default looks "only in the standard system directories", plus `-I`. Read through WebFetch on 2026-11-28; the fetch tool summarises pages, so quotes are as it returned them. **[cited]**
- Two public repositories in round 1, and two more in round 2, were shallow-cloned (`--depth 1`) into a scratch folder outside this repository. Nothing in them was built, configured or run; build files were read as text. Throwaway Python scans counted the figures in the tables. They are inlined under "Method" and "Round 2 method".
- Each claim is tagged **[cited]** (URL, or `file:line` at the clone's commit), **[measured]** (counted by the scan, whose command is under "Method") or **[judgement]** (my own reading).

## Trial repositories

| Repository | Commit (date) | Licence | Tracked files / scanned | Build | Why |
|---|---|---|---|---|---|
| curl/curl | `23256cf755a9` (2026-11-27) | curl (MIT-style; GitHub reports `NOASSERTION`) | 4605 / 1081 (`.c` 821, `.h` 258, `.cpp` 2) | autotools and CMake | a public `include/curl/`, plus `lib/` and `src/` with subfolders (`lib/vtls/`, `lib/curlx/`) |
| google/leveldb | `29465588d3df` (2026-11-12) | BSD-3-Clause | 154 / 133 (`.cc` 76, `.h` 56, `.c` 1) | CMake | quoted includes written from the repository root (`#include "db/dbformat.h"`) |

Licence and push dates from `gh api repos/<r>`. **[cited]** Counts from `git ls-files`. **[measured]**

Scanned extensions: `.c .h .cc .cpp .cxx .c++ .hpp .hh .hxx .h++ .inl .ipp .tcc`. Left out: curl's 12 `.inc` files (Makefile fragments, `lib/Makefile.inc` and the like), `lib/libcurl.def` (a Windows export list) and `.rc` resource scripts. Files of either repository were the only include targets; leveldb's `third_party/googletest` and `third_party/benchmark` are submodules (`.gitmodules`), so their headers are not tracked files. **[measured]**

leveldb is small (133 scanned files, 779 include lines) but not trivial: it is the case the brief named. It was kept, not replaced. **[judgement]**

### The include roots each build passes

- **curl.** `lib/Makefile.am:65-68` passes `-I$(top_srcdir)/include -I$(top_builddir)/lib -I$(top_srcdir)/lib -I$(srcdir)`; `src/Makefile.am:44-47` the same with `src` as `srcdir`. The test programs add `-I$(top_srcdir)/src` and `tests/libtest` (`tests/libtest/Makefile.am:37-41`, `tests/unit/Makefile.am:36-40`, `tests/unit/CMakeLists.txt:45-50`). `$(top_builddir)/lib` is where the generated `curl_config.h` lands (`tests/unit/CMakeLists.txt:46`, comment "for "curl_config.h""; its template is `lib/curl_config-cmake.h.in`). **[cited]**
- **leveldb.** `CMakeLists.txt:107-110` sets `include_directories("${PROJECT_BINARY_DIR}/include" ".")`, and `CMakeLists.txt:234-237` adds `${PROJECT_SOURCE_DIR}/include` for the library. The binary-dir root holds `port/port_config.h`, generated by `configure_file` from `port/port_config.h.in` (`CMakeLists.txt:102-105`). **[cited]**

The scan tried the roots `.`, `include`, `src`, `lib` and `tests/libtest` on both repositories. **[measured]**

## Results: curl

All rows **[measured]**. "Edges" are include lines, not deduplicated pairs. "Production" is the 483 files under `include/`, `lib/` and `src/`; the rest are `tests/`, `docs/examples/` and `projects/`.

| What | All 1081 files | Production (483) |
|---|---|---|
| Include lines | 4285 | 2940 |
| Quoted / angle / macro | 3078 (71.8%) / 1204 (28.1%) / 3 (0.1%) | 2375 / 562 / 3 |
| (a) quoted, adjacent | **2026 (65.8% of quoted)** | 1653 (69.6%) |
| (b) quoted, unique under one root | 1044 (33.9%): `lib` 937, `tests/libtest` 93, `src` 12, `include` 2, `.` 0 | 720 (30.3%): `lib` 719, `src` 1 |
| ...of which repo root or `include/` alone finds | **2 (0.1%)** | 0 (0.0%) |
| (c) quoted, ambiguous | 0 | 0 |
| (d) quoted, unresolved | 8 (0.3%) | 2 (0.1%) |
| Angle naming a tracked file | 173 of 1204 (14.4%): `include` 169, `lib` 2, `src` 2 | 7 of 562 (1.2%), all `include` |
| Conditional (inside `#if`, guard excluded) | 1722 (40.2%), 1072 of them in-repo | 1538 (52.3%), 1016 in-repo |
| Files with at least one adjacent edge | 643 (59.5%) | 310 (64.2%) |
| Files with at least one in-repo edge, any rule | 1015 (93.9%) | 429 (88.8%) |
| ...of those, every in-repo edge adjacent | 622 (61.3%) | 302 (70.4%) |

- **Why (b) is so large.** curl writes `lib/`-relative paths even from subfolders: `lib/vtls/openssl.c:37` includes `"vtls/openssl.h"`, and `:32` includes `"urldata.h"` (which is `lib/urldata.h`). Adjacent resolution misses both; the `-I lib` root finds them. **[cited]**
- **Unresolved.** `curl_config.h` and `unitprotos.h` (both generated into the build's `lib/`), and six `projects/OS400/` and `src/tool_vms.c` includes such as `"curl.h"`, which a platform build resolves under `include/curl/` or a VMS folder. **[measured]**
- **Macro includes.** The three are VMS system modules written bare, `#  include descrip`, `starlet` and `iodef` (`src/tool_getpass.c:40-42`), not computed names. **[cited]**
- **Conditional.** Most are whole-file feature switches: `lib/vtls/openssl.c:30` opens `#ifdef USE_OPENSSL` right after `#include "curl_setup.h"`, and every include after it sits inside. **[cited]** 256 of 258 headers had a detected include guard. **[measured]**

## Results: leveldb

All rows **[measured]**.

| What | All 133 files |
|---|---|
| Include lines | 779 |
| Quoted / angle / macro | 536 (68.8%) / 243 (31.2%) / 0 |
| (a) quoted, adjacent | **0 (0.0%)** |
| (b) quoted, unique under one root | 499 (93.1%): `.` 295, `include` 204 |
| ...of which repo root or `include/` alone finds | **499 (93.1%)** |
| (c) quoted, ambiguous | 0 |
| (d) quoted, unresolved | 37 (6.9%): `gtest/gtest.h` 32, `port/port_config.h` 2, `gmock/gmock.h` 1, `benchmark/benchmark.h` 1, `port/port_chromium.h` 1 |
| Angle naming a tracked file | 0 of 243 |
| Conditional (guard excluded) | 8 (1.0%), 1 of them in-repo |
| Files with at least one adjacent edge | 0 (0.0%) |
| Files with at least one in-repo edge, any rule | 121 (91.0%) |

- **Why (a) is zero.** Every quoted include names a path from the repository root or from `include/`, even inside one folder: `db/db_impl.cc:5` includes `"db/db_impl.h"`, which sits next to it, but adjacent resolution looks for `db/db/db_impl.h`. **[cited]**
- **Unresolved** are submodule headers (googletest, benchmark), the generated `port_config.h`, and a Chromium port header that exists only in Chromium's tree. **[measured]**
- 56 of 56 headers had a detected include guard. **[measured]**

## Comparison

| | curl | leveldb |
|---|---|---|
| Quoted, adjacent | 65.8% | 0.0% |
| Gained by repo root + `include/` | 0.1% | 93.1% |
| Gained by the build's own `-I` roots | 33.9% (mostly `lib`) | 93.1% |
| Ambiguous under those roots | 0 | 0 |
| Macro | 0.1% | 0 |
| Conditional | 40.2% | 1.0% |
| Share of in-repo edges an adjacent-only reader sees | 2026 of 3243 (62.5%) | 0 of 499 (0%) |
| `compile_commands.json` tracked | no | no |

In-repo edges are (a) + (b) + (c) + angle includes naming a tracked file. **[measured]**

A third rule, not in the brief, was counted as well: match a non-adjacent quoted include by path suffix against every tracked file (`"urldata.h"` finds the one file whose path ends `/urldata.h`). It found exactly one file for 1050 of curl's 1052 non-adjacent quoted includes and for all 499 of leveldb's resolvable ones; it never found two. That covers curl's `lib/` and `tests/libtest/` roots and the six OS400/VMS cases without reading a build file. The counts match the root rule's, but the files were not compared one by one, and two small, single-project repositories say nothing about a monorepo or a repository with vendored copies, where two `config.h` or `util.h` files are likely. **[measured]**

## `compile_commands.json`

- Tracked in neither trial repository: `git ls-files | grep -i compile_commands` is empty in both. **[measured]**
- The clang docs: "The convention is to name the file compile_commands.json and put it at the top of the build directory." `directory` is "The working directory of the compilation", and "All paths specified in the command or file fields must be either absolute or relative to this directory." https://clang.llvm.org/docs/JSONCompilationDatabase.html **[cited]**
- CMake writes it when `CMAKE_EXPORT_COMPILE_COMMANDS` is on, "Added in version 3.5", for the Makefile and Ninja generators only: "It is ignored on other generators." https://cmake.org/cmake/help/latest/variable/CMAKE_EXPORT_COMPILE_COMMANDS.html **[cited]**
- In the wild, GitHub's code search API reports `total_count` 74,240 for `filename:compile_commands.json`, against 6,750,208 for `filename:CMakeLists.txt`. Both count files, not repositories, and the API's counts are approximate (a query for `.gitignore` files mentioning it returned 697, which is implausibly low), so no per-repository share could be derived. **[measured]** (`gh api -X GET search/code -f q='filename:compile_commands.json' --jq .total_count`, 2026-11-28)
- Of the first five hits of `gh search code --filename compile_commands.json`, two were other files (`compile_commands.json.tmp00ba4`, `compile_commands.json.md`). One real one, fenrus75/powertop's, starts `"directory": "/home/arjan/git/powertop/build"` with `-I. -I.. -I../src`: absolute paths from its author's machine, so its flags map onto a clone only after rewriting that prefix. **[measured]**

## What it means for #748's open questions

All **[judgement]**, as input to the grilling.

1. **Is quoted-and-adjacent alone worth shipping?** It depends on the repository's convention, and the two trials sit at the extremes: about two-thirds of curl's edges, none of leveldb's. A leveldb map from an adjacent-only reader would have no lines at all, which is the "no dependencies" misreading #748 warns about. The convention that defeats it (paths from the repo root or from `include/`) is the common CMake and Google style, so it is not an edge case.
2. **Repo root + `include/` as fixed guesses** gain 93% on leveldb and 0.1% on curl, whose root is `lib/`. A fixed list is therefore repository-shaped. The `-I` roots a build file names, or the suffix match above, covered both trials with no ambiguity; whether either stays unambiguous on a larger repository is not measured here (round 2 measures it: it does not, see §A to §D).
3. **Does `compile_commands.json` matter?** Not for these two: neither tracks one, and the one tracked sample seen holds machine-absolute paths. It would help a repository that commits a cleaned one, which this pass could not size. Reading it when present costs little; counting on it would leave both trials where adjacent-only leaves them.
4. **Conditional includes.** In curl, 40% of include lines (52% in production) sit inside `#if` blocks, mostly whole-file feature switches. A reader that takes every branch gets edges that exist in some configuration: an over-count to state, not a source of wrong links.
5. **What a partial reader misses, for the language line** (spec #753, story 6: "TypeScript and Go: links computed. Ruby: no reader yet"). For an adjacent-only reader the measured miss is about 37% of curl's in-repo edges and 100% of leveldb's. That spread suggests the line should say C/C++ is read partly, and possibly how many of the repository's quoted includes it resolved (per repository, as above), rather than one fixed caveat.

## Round 2: grading the suffix-match rule

The candidate rule, per include:

1. A quoted include that resolves next to the including file is a line (the GCC rule).
2. Otherwise, collect every tracked, non-ignored file whose path ends in `/<include path>` or equals it. One match is a line. Several make the including file Unsure, listing the candidates. None means a system or generated header, which gives no line.
3. Angle includes go through step 2.

The existing readers never guess: go-imports makes an import with several candidate folders Unsure (`system-map/src/extract/go/mod.rs:536-550`), and java-imports leaves ignored files out of resolution (`docs/studio/behaviour/map.md:122`). **[cited]** So below, a wrong line counts for much more than an Unsure. **[judgement]**

### Repositories

| Repository | Commit (date) | Licence | Tracked / scanned | Vendored trees |
|---|---|---|---|---|
| bitcoin/bitcoin | `afedb19cca4c` (2026-11-27) | MIT | 3095 / 1568 | `src/crc32c`, `src/crypto/ctaes`, `src/ipc/libmultiprocess`, `src/leveldb`, `src/minisketch`, `src/secp256k1`: the git subtrees listed in `test/lint/README.md:97-102`. `src/univalue` is not on that list, so it was counted as first-party. |
| redis/redis | `15d19442c4cd` (2026-11-27) | GitHub reports `NOASSERTION` | 1961 / 856 | every folder under `deps/` (croaring, fpconv, hdr_histogram, hiredis, jemalloc, linenoise, lua, tre, xxhash), each its own tree |
| curl/curl, google/leveldb | as in round 1 | | | none |

Licences from `gh api repos/<r>`; commits from `git rev-parse HEAD`. **[cited]** Neither new repository tracks a `compile_commands.json`. **[measured]**

### The yardstick: each build's `-I` roots

Each file was resolved the way GCC would: adjacent first for quoted includes, then the `-I` roots of the folder the file sits in, in order, first hit wins. The roots were read by hand from the build files and hardcoded per path prefix (longest prefix wins). They are listed below; the script under "Round 2 method" has them in full.

- **curl.** Round 1's roots, plus the OS400 build's `INCDIR('…/include/curl' '…/include' '${SRCDIR}' '…/projects/OS400')` (`projects/OS400/initscript.sh:198-200`), with `SRCDIR` taken as `lib/`. **[cited]**
- **leveldb.** Round 1's roots. **[cited]**
- **bitcoin.**
  - Every first-party target links `core_interface`, which adds `src/` (`src/CMakeLists.txt:8-11`). Linked libraries add their public include folders: `src/leveldb/include` (`cmake/leveldb.cmake:70-75`), `src/crc32c/include` (`cmake/crc32c.cmake:92-95`), `src/minisketch/include` (`cmake/minisketch.cmake:44-47`), `src/univalue/include` (`src/univalue/CMakeLists.txt:11-14`), `src/secp256k1/include` (`src/secp256k1/src/CMakeLists.txt:121-123`) and `src/ipc/libmultiprocess/include` (`src/ipc/libmultiprocess/CMakeLists.txt:193-196`).
  - Vendored leveldb gets `src/leveldb`, `src/leveldb/include`, then `src` and crc32c through its links (`cmake/leveldb.cmake:70-75, 87-91`). libmultiprocess's examples and tests add their own folder (`example/CMakeLists.txt:11,18,25`, `test/CMakeLists.txt:36`). **[cited]**
- **redis.**
  - `src/` gets `-I../deps/hiredis -I../deps/linenoise -I../deps/lua/src -I../deps/hdr_histogram -I../deps/fpconv -I../deps/xxhash -I../deps/croaring/include` (`src/Makefile:279`), plus `-I../deps/jemalloc/include` when `MALLOC=jemalloc` (`src/Makefile:318-320`), the Linux default (`src/Makefile:80-84`).
  - The rest: croaring `-Iinclude -Isrc` (`deps/croaring/Makefile:13`), tre `-Ilocal_includes -Ilib` (`deps/tre/Makefile:25`), jemalloc `-I$(srcroot)include` (`deps/jemalloc/Makefile.in:27`, and `test/include` at `:452`), hiredis `-I.` (`deps/hiredis/Makefile:187`), lua's `etc/` `-I$(INC)` with `INC=$(TOP)/src` (`deps/lua/etc/Makefile:5,11`), and `tests/modules` `-I../../src` (`tests/modules/Makefile:107`). **[cited]**
- **Where this is approximate.** **[judgement]**
  - bitcoin's first-party files all get one union of roots in one order, not each target's own list.
  - Build-folder roots are left out, so generated headers (`bitcoin-build-config.h`, curl's `curl_config.h`) count as "none".
  - Platform-only roots are left out: jemalloc's `msvc_compat/C99`, added only for MSVC (`deps/jemalloc/configure.ac:343-345`), the OpenSSL prefix, and the VMS build.
  - secp256k1's own sources get only its `include/`.
  - So a "line, `-I` none" was checked by hand before being called wrong (below).

### A. Step 2 graded include by include

Step 2 handles every non-adjacent quoted include and every angle include. All rows **[measured]**.

All files, vendored trees in the match pool:

| | curl | leveldb | bitcoin | redis |
|---|---|---|---|---|
| Step 1, adjacent (exact) | 2026 | 0 | 521 | 994 |
| Step 2 line, same file as `-I` | 1222 | 499 | 8198 | 940 |
| Step 2 line, `-I` picks a different tracked file | 0 | 0 | 0 | 0 |
| Step 2 line, `-I` finds none (**wrong line**) | 4 (+1, see below) | 0 | 37 | 273 |
| Unsure, `-I` finds one | 0 | 0 | 956 | 3 |
| Unsure, `-I` finds none | 0 | 0 | 4 | 1 |
| Step 2 none, `-I` finds one | 0 | 0 | 0 | 0 |
| Both none (system or generated) | 1029 | 280 | 5683 | 1416 |

First-party files only:

| | bitcoin, vendored in | bitcoin, vendored ignored | redis, vendored in | redis, vendored ignored |
|---|---|---|---|---|
| Adjacent | 2 | 2 | 594 | 593 |
| Step 2 line, agrees | 7611 | 7594 | 103 | 48 |
| Wrong line | 2 | 2 | 145 | 3 |
| Unsure, `-I` finds one | 924 | 878 | 3 | 0 |
| `-I` target is in an ignored tree, step 2 gives none | — | 63 | — | 56 |
| Both none | 4916 | 4916 | 689 | 834 |

- **No step-2 line ever disagreed with `-I` about which tracked file it is.** The file `-I` finds always ends in `/<include path>`, so it is always among the suffix candidates. A single candidate is therefore that file whenever `-I` finds one, and a wrong line needs the real header to be outside the pool. **[judgement]**, and borne out: the "different tracked file" row is 0 in all four. **[measured]** With vendored trees ignored, the real header can itself be ignored, and that is where redis's 3 come from (§C).
- **Every wrong line, by name:**
  - **curl (4).** `<rustls.h>` matched `lib/vtls/rustls.h` (in `lib/vtls/rustls.c:30` and `tests/libtest/cli_hx_download.c:43`): the real one is the rustls library's header, and the file includes curl's own as `"vtls/rustls.h"` at `:40`. `<schannel.h>` matched `lib/vtls/schannel.h`, which includes `<schannel.h>` itself at `:31`, so the rule draws a self-line where the real header is the Windows SDK's. **[cited]**
  - **curl, the +1.** `src/tool_vms.c` includes `"curlmsg_vms.h"`, matched to `projects/vms/curlmsg_vms.h`. The VMS build was not modelled, so this one is not graded; it is probably right. **[judgement]**
  - **bitcoin (37).** `<string.h>` matched `src/util/string.h` 33 times, all from vendored C code: leveldb, secp256k1, crc32c, minisketch and ctaes, for example `src/leveldb/port/port.h:8`. `<cpuid.h>` matched `src/compat/cpuid.h` 3 times, one of them `src/compat/cpuid.h:11` including itself. `<process.h>` under `#ifdef WIN32` (`src/util/exec.cpp:16-17`) matched `src/ipc/process.h`. **[cited]**
  - **redis (273, 145 of them from first-party files).**
    - jemalloc's MSVC shims and test headers stand in for standard headers: `<stdint.h>` → `deps/jemalloc/include/msvc_compat/C99/stdint.h` (89), `<assert.h>` → `deps/jemalloc/include/jemalloc/internal/assert.h` (68), `<math.h>` → `deps/jemalloc/test/include/test/math.h` (47), `<strings.h>` (22) and `<stdbool.h>` (20). First-party examples: `src/dict.h:47` `<stdint.h>`, and most of `tests/modules/*.c`.
    - Others: `<memory.h>` → croaring (4), `<poll.h>` → `deps/hiredis/adapters/poll.h` (3) and `<regex.h>` → tre (2, from `src/monotonic.c:75` and `src/redis-check-aof.c:35`, whose `-I` list has no tre folder). `<config.h>` from tre's sources → `src/config.h` (15; it sits under `#ifdef HAVE_CONFIG_H`, `deps/tre/lib/regcomp.c:9-10`), `<glib.h>` (1), and hiredis's `"redismodule.h"` and `"testhelp.h"` → `src/` (2). **[measured]**, lines **[cited]**

### B. Angle-bracket false positives

The list of standard and system names is the C and C++ standard headers (https://en.cppreference.com/w/c/header, https://en.cppreference.com/w/cpp/header), the POSIX headers (https://pubs.opengroup.org/onlinepubs/9799919799/idx/head.html), any name under `sys/`, `linux/`, `mach/`, `machine/`, `bits/`, `netinet/`, `arpa/` or `net/`, and a short platform list of my own (`byteswap.h`, `malloc.h`, `cpuid.h`, `windows.h`, the `*intrin.h` family and a few more). **[cited]** for the three lists, **[judgement]** for the rest.

| Angle includes that step 2 matched | curl | leveldb | bitcoin | redis |
|---|---|---|---|---|
| Name on the system list, wrongly matched | 0 | 0 | 36 | 251 |
| Name on the system list, Unsure | 0 | 0 | 4 (`<time.h>` vs `src/util/time.h` and `src/test/util/time.h`) | 0 |
| Name on the system list, truly in-repo | 0 | 0 | 0 | 0 |
| Wrong lines the list misses | 4 (`rustls.h`, `schannel.h`) | 0 | 1 (`process.h`) | 20 (`config.h`, `memory.h`, `glib.h`) |

**[measured]**

- **The `<endian.h>`-against-`compat/` case did not occur.** bitcoin has `src/compat/endian.h` and `src/compat/byteswap.h` but never writes a bare `<endian.h>` or `<byteswap.h>`: it writes `<compat/endian.h>`. The collision that did occur is `<string.h>` against `src/util/string.h`. **[measured]**
- **No quoted include anywhere named a standard or system header** without resolving adjacent: 0 in all four. Quoted bare names (`"config.h"`, `"util.h"`) that step 2 resolved were 673 agree and 1 ungraded in curl, 112 agree and 2 wrong in redis, and none in leveldb or bitcoin. **[measured]**

The three variants of step 3, angle includes only, all files, vendored in. Each cell is agree / wrong / Unsure, with skipped includes in brackets. **[measured]**

| Variant | curl | leveldb | bitcoin | redis |
|---|---|---|---|---|
| (i) as written | 173 / 4 / 0 | 0 / 0 / 0 | 7713 / 37 / 928 | 231 / 271 / 4 |
| (ii) only names with a folder part | 173 / 0 / 0 (607 skipped) | 0 / 0 / 0 (232) | 5883 / 0 / 184 (7792) | 176 / 0 / 0 (1325) |
| (iii) only through a root some build file names with `-I` | 173 / 0 / 0 | 0 / 0 / 0 | 8624 / 0 / 13 | 231 / 18 / 3 |
| (iii), vendored ignored | — | — | 8472 / 0 / 0 | 0 / 3 / 0 |

- **(ii)** removes every wrong angle line in all four. It also drops right lines: 1,830 in bitcoin, whose first-party code writes its own headers bare (`#include <hash.h>`, `<random.h>`, resolved through `-I src`), and 55 in redis.
- **(iii)** here uses the union of every root found in the repository's build files, not each file's own list.
  - It keeps all of bitcoin's right lines, adds 911 more by settling Unsures, and has no wrong ones.
  - Its 18 wrong redis lines are roots crossing between builds: tre's `<config.h>` finds `src/config.h` through `tests/modules`' `-I../../src`, and `src/`'s `<regex.h>` finds `deps/tre/local_includes/regex.h` through tre's own root.
  - It needs the build files read, which round 1 called cheap (one `-I` line per Makefile or CMake target), but it is still a parser per build system. **[judgement]**

### C. Vendored trees in the pool or ignored

First-party includes whose step-2 outcome changes between the two runs. **[measured]**

- **bitcoin, 13,455 includes.**
  - 63 go from a line to none: lines into vendored headers such as `<leveldb/db.h>`, which the map drops anyway once the target is ignored.
  - 46 go from Unsure to a line, all agreeing with `-I` (`<hash.h>` had `src/hash.h`, `src/leveldb/util/hash.h` and `src/secp256k1/src/hash.h`).
  - 147 stay Unsure with fewer candidates. The 2 wrong lines stay.
  - 878 Unsures remain: they are first-party name repeats (`src/random.h` and `src/test/util/random.h`; `src/protocol.h`, `src/rpc/protocol.h`, `src/ipc/protocol.h` and `src/ipc/capnp/protocol.h`), which ignoring vendored code cannot fix.
- **redis, 1,534 includes.**
  - 201 go from a line to none: the 145 wrong lines, and 56 right lines into `deps/` that the map no longer draws.
  - 3 go from Unsure to a line, and all 3 are wrong. `src/redis-cli.c:37` and `src/cli_common.c:21` write `#include <sds.h> /* use sds.h from hiredis, … */`, and with hiredis ignored only `src/sds.h` is left to match. **[cited]** Ignoring a tree can turn an honest Unsure into a wrong line whenever first-party code reaches into that tree by a name it also has.

Do vendored files' own includes stay inside their tree? Counted per tree (bitcoin's six subtrees, redis's nine `deps/` folders). **[measured]**

| | `-I` inside / outside own tree | Step 2 inside / outside / Unsure |
|---|---|---|
| bitcoin | 1137 / 1 | 1105 / 36 / 36 |
| redis | 1236 / 1 | 1242 / 123 / 1 |

By the build, vendored code is self-contained apart from one include each (leveldb reaching crc32c in bitcoin). Step 2's lines out of a vendored tree are almost all wrong: 35 of bitcoin's 36, and in redis mostly croaring and tre reaching jemalloc's shims or `src/config.h`. **[measured]**

### D. Tie-breaker, graded

The tie-breaker as asked, "the candidate closest to the including file", means the candidate sharing the deepest folder prefix with it. Candidates still level after that stay tied. A second rule was graded as well: "the shortest candidate path", meaning fewest folders. **[measured]**

| Unsures | Closest: right / wrong / still tied | Shortest path: right / wrong / still tied |
|---|---|---|
| bitcoin, vendored in (960) | 56 / 241 (+4 on `<time.h>`, where `-I` finds none) / 663 | 924 / 32 (+4 `<time.h>`) / 0 |
| bitcoin, vendored ignored (878) | 24 / 241 / 613 | 878 / 0 / 0 |
| redis (4) | 0 / 3 (+1 where `-I` finds none) / 0 | 0 / 3 (+1) / 0 |
| curl, leveldb | no Unsures | no Unsures |

- **Why "closest" fails.** An angle include never searches the including file's folder. `src/kernel/chain.cpp:7` writes `#include <chain.h>`, which means `src/chain.h` through `-I src`, but the closest candidate is `src/kernel/chain.h`. Most of the 241 are tests: `src/test/*.cpp` including `<validation.h>`, `<random.h>` or `<util/time.h>` get `src/test/util/…` instead of the `src/` file. 13 Unsures list the including file itself among the candidates (`src/node/psbt.h` includes `<psbt.h>`). **[cited]** / **[measured]**
- **Shortest path happens to fit bitcoin**, whose one real root is `src/`. Its 32 misses are vendored leveldb's `"util/…"` includes, which mean `src/leveldb/util/…`. **[measured]** It is a guess about layout, not a rule of the language. **[judgement]**

### E. Speed

`python3 -I grade_suffix.py clones/bitcoin bitcoin --time` lists the files, reads them, strips comments, finds includes, builds the suffix index and resolves every include. It took 1.38 to 1.39 s in three runs (`/usr/bin/time -p`: real 1.42 to 1.44 s), on an Apple M5 with 10 cores at a load average of about 5. That covers 3,095 tracked files, 1,568 scanned (21.3 MB of C/C++, by `wc -c`) and 15,399 includes. **[measured]**

Under `cProfile`, the character-by-character comment stripper takes 1.69 s of 1.92 s (88%). Listing, the index and resolution together are under 0.25 s. **[measured]** A compiled scanner would be well under spec #753's "under a second" (story 1) on this repository. **[judgement]**

### What round 2 means for #748

All **[judgement]**, as input to the grilling.

1. **Steps 1 and 2 on quoted includes are safe.** No quoted include drew a wrong line except hiredis's two, which point at a Redis-module build (curl's VMS one is ungraded). Quoted step-2 lines agreed with `-I` 1,049 times in curl, 499 in leveldb, 485 in bitcoin and 709 in redis.
2. **Step 3 as written is not safe.** Bare angle names collide with system headers whenever a repository has a file of the same name anywhere in it: 271 wrong lines in redis, including most of its test modules. That is the false-edge kind the existing readers avoid.
3. **The safe forms of step 3**, from the measurements:
   - (ii) costs right lines in repositories that write their own headers bare (bitcoin).
   - (iii) keeps them and fixes Unsures, but needs the `-I` roots read from build files.
   - A third option, not measured: a bare angle name on the system list (§B) gives no line. It would have caught 36 of bitcoin's 37 and 251 of redis's 273 wrong lines, not curl's library names.
4. **Ignoring vendored trees** removes most of redis's wrong lines. But a first-party include that means the vendored copy (redis's `<sds.h>`) then becomes a wrong line. Grading it as Unsure needs the ignored files kept as candidates that block a match without drawing one. java-imports instead leaves ignored files out entirely (`map.md:122`).
5. **No tie-breaker should turn an Unsure into a line.** "Closest" was wrong four times as often as right. "Shortest path" was right on bitcoin by its layout, not by any rule, and wrong on every redis case.
6. **Speed is not the constraint**; the comment lexer is the only cost.

## Round 3: four refinements

**Base case:** steps 1-3 as in round 2, but a name on §B's system list is never matched, and vendored trees are ignored and dropped from the pool. First-party files only, graded against the same `-I` yardstick; a line to an ignored or skipped file counts as wrong. All **[measured]** with `grade_r3.py` (under "Round 3 method").

| Base case | right | wrong | Unsure |
|---|---|---|---|
| curl | 1222 | 5 (`<rustls.h>` ×2, `<schannel.h>` ×2, VMS ×1) | 0 |
| leveldb | 499 | 0 | 0 |
| bitcoin | 7594 | 1 (`<process.h>`) | 878 |
| redis | 48 | 3 (`<sds.h>`) | 0 |

1. **Skip test files.**
   - Patterns, exactly as given, none added: a folder named `test`, `tests`, `testing`, `unittest` or `fuzz`, or a file name matching `*_test.*`, `*_tests.*` or `test_*.*`.
   - bitcoin's Unsure falls from 878 to **206**. In non-test files, 297 Unsures become lines, all right, and none turns wrong.
   - 51 right lines go, from `src/bench/` (which matches no pattern) into test helpers such as `<test/util/setup_common.h>`.
   - curl's wrong lines fall from 5 to 3. redis loses its 48 right step-2 lines (all in `tests/modules`) and 1 adjacent line (`modules/vector-sets/expr.c:887` includes `"fastjson_test.c"`). leveldb changes only by its skipped files.
2. **Angle includes don't search the including file's folder** (GCC "Search Path", quoted under "How this was read"). Variant: drop the candidate `dirname(file)/name` for angle includes.
   - It removes curl's 2 self-folder lines (`<rustls.h>` in `lib/vtls/rustls.c`, `<schannel.h>` in `lib/vtls/schannel.h`) and redis's 3 `<sds.h>` lines, at no cost there.
   - On bitcoin it loses **1,117** right lines and adds **109** wrong ones. With tests skipped as well, it loses 1,217 and adds 25.
   - The cause: bitcoin's own folder `src/` is also its `-I` root, so `src/addrdb.cpp`'s `<addrdb.h>` really is the file next to it. When that candidate is dropped, a `src/test/util/` or `src/kernel/` twin wins instead.
3. **Ignored files block** (ignored files stay candidates; a match of only ignored files gives no line).
   - bitcoin: 46 right lines become Unsure (`<hash.h>` against vendored `hash.h` copies); wrong stays 1.
   - redis: the 3 wrong `<sds.h>` lines become Unsure; wrong goes to 0.
4. **Unsure in the map's units.** The core keeps one finding per (file, name, reason) (`system-map/src/extract/mod.rs:250-251`). **[cited]**
   - bitcoin base: 878 findings in 455 files. With tests skipped: 206 findings in 138 files.
   - Top names after skipping: `validation.h` 55, `chain.h` 48, `chainparams.h` 38, `rpc/util.h` 29, `protocol.h` 14. Each is a first-party name in two to four folders (`src/chain.h`, `src/interfaces/chain.h`, `src/kernel/chain.h`).
   - curl, leveldb and redis have none.

Full tables, with the base-case top 10 names: `local-evidence/2026-11-28-c-cpp-extractor-grilling/reports/research-round3.md` (on Daniel's machine).

## Round 4: do the Unsures cost links, and what fixes them

**New base case** (`grade_r4.py`, under "Round 4 method"):
- Round 3's base, with the system list applied to `<…>` only.
- Windows CRT and SDK names are added (`process.h`, `schannel.h`, `windows.h`, `winsock2.h`, `ws2tcpip.h`, `io.h`, `direct.h`, `malloc.h`, `conio.h`, `tchar.h`), plus `memory.h`. Cited for `process.h`, `direct.h`, `memory.h` and `schannel.h` in the local report; the rest are **[judgement]**.
- Test files are skipped, ignored files block, and vendored trees are ignored.

All **[measured]**.

| Base | right (adjacent) | wrong | Unsure findings / files |
|---|---|---|---|
| curl | 2541 (1657) | 2 (`<rustls.h>`, VMS) | 0 |
| leveldb | 354 | 0 | 0 |
| bitcoin | 4765 (2) | 0 | 324 / 208 |
| redis | 592 (592) | 0 | 3 / 3 |

1. **Part links on a proxy map** (no real map exists).
   - **Partitions:**
     - **bitcoin:** each folder under `src/` plus `core` for `src/`'s own files (23 parts), and a 15-part version keeping the 14 largest and folding the rest into one.
     - **redis:** `src`, `src/modules`, `modules/vector-sets`, `utils`.
     - **curl:** top-level folders.
   - **No part pair is unproven or lost in any repository:** bitcoin 187 of 187 proved (117 of 117 at ≤15 parts), redis 3 of 3, curl 6 of 6.
   - The Unsures thin 20 of bitcoin's pairs but never remove one. The thinnest is `src/zmq → src/rpc`, with 1 of 2 lines proved.
   - 28 bitcoin pairs rest on a single line, all proved.
2. **Inferred roots.** A unique match's path minus its include path implies a root (bitcoin: `src` 4,711). Picking the one Unsure candidate under a root implied at least N times settled all 324 of bitcoin's Unsures correctly at N = 1, 5 and 20, `src/kernel/chain.cpp`'s `<chain.h>` → `src/chain.h` among them. It sent redis's `<sds.h>` to ignored hiredis (no line, right) at N = 1 and 5.
3. **Roots read from build files as text.**
   - **Union** (CMake `include_directories`/`target_include_directories`, Makefile `-I`): bitcoin 5,045 right, 0 wrong, 44 Unsure (secp256k1's own `src` root makes `<hash.h>` ambiguous).
   - **Per folder and its ancestors:** 0 Unsure, but 49 `<univalue.h>` lines missed, since that root belongs to a linked target (`src/univalue/CMakeLists.txt:11-14`).
   - Round 2's 18 redis crossing errors are already 0 in the new base.
4. **A declared root.**
   - bitcoin with `src` declared: 5,089 right, 0 wrong, 0 Unsure, the best of every variant.
   - redis: `src` first draws the 3 wrong `<sds.h>` lines; the build's order (`deps/…` roots, then `src`) draws none.
5. **Test-skip risk.**
   - No skipped file is compiled by a non-test target in any repository, judged from the build files as text.
   - One cost: bitcoin's `src/bench/` (a non-test executable that links `test_util`) loses 51 lines into `src/test/util/` helpers.
   - The only `unit*` folder is curl's `tests/unit`, already skipped.

Full tables and lists: `local-evidence/2026-11-28-c-cpp-extractor-grilling/reports/research-round4.md` (on Daniel's machine).

## Method

```sh
S=<scratchpad>/c-trial
git clone --depth 1 https://github.com/curl/curl     $S/clones/curl
git clone --depth 1 https://github.com/google/leveldb $S/clones/leveldb
python3 -I $S/scripts/scan_includes.py $S/clones/curl    .,include,src,lib,tests/libtest
python3 -I $S/scripts/scan_includes.py $S/clones/curl    .,include,src,lib,tests/libtest include/,lib/,src/
python3 -I $S/scripts/scan_includes.py $S/clones/leveldb .,include,src,lib,tests/libtest
```

What the scan does:

- Reads `git ls-files`; every tracked file is a possible include target, and files with the extensions above are scanned.
- Joins backslash-newline continuations, then blanks `/* */` and `//` comments, skipping over string and character literals, so a commented-out `#include` is not counted.
- A directive is a line matching `^\s*#\s*name`, so `# include` and leading whitespace count. `#include`, `#include_next` and `#import` are read (neither trial has the last two).
- Form: `"..."` quoted, `<...>` angle, anything else macro.
- Quoted: (a) if `dirname(file)/target`, normalised, is tracked; else (b) if exactly one root gives a tracked file; (c) if two or more do; (d) if none. Angle: in-repo if any root gives a tracked file.
- Conditional depth: `#if`, `#ifdef` and `#ifndef` add one, `#endif` takes one. A header's guard is excluded when its first two directives are `#ifndef X` / `#define X` and the `#endif` closing that `#ifndef` is its last directive. `#pragma once` needs no exclusion.
- Rough edges: one include written under two `#if` branches counts twice; `lstrip("./")` in the suffix match is crude; no `-iquote` or `-isystem` modelling.

`scan_includes.py`:

```python
"""Count #include forms and how they resolve, over a repo's tracked C/C++ files.

Usage: python3 -I scan_includes.py <clone> <root>[,<root>...] [<path prefix>[,<prefix>...]]
Roots are repo-relative folders a build file passes as -I ("." is the repo root).
Reads files as text; runs nothing from the clone except `git ls-files`.
"""
import collections, os, posixpath, re, subprocess, sys

EXTS = {".c", ".h", ".cc", ".cpp", ".cxx", ".c++", ".hpp", ".hh", ".hxx", ".h++",
        ".inl", ".ipp", ".tcc"}
DIRECTIVE = re.compile(r"^\s*#\s*([A-Za-z_]+)\b(.*)$")
GUARD_DEF = re.compile(r"^\s*([A-Za-z_]\w*)\s*$")


def strip_comments(text):
    """Blank out /* */ and // comments, keep string/char literals and newlines."""
    out, i, n = [], 0, len(text)
    while i < n:
        c = text[i]
        if c == "/" and i + 1 < n and text[i + 1] == "*":
            j = text.find("*/", i + 2)
            j = n if j < 0 else j + 2
            out.append(re.sub(r"[^\n]", " ", text[i:j]))
            i = j
        elif c == "/" and i + 1 < n and text[i + 1] == "/":
            j = text.find("\n", i)
            j = n if j < 0 else j
            i = j
        elif c in "\"'":
            j = i + 1
            while j < n and text[j] != c and text[j] != "\n":
                j += 2 if text[j] == "\\" else 1
            out.append(text[i:j + 1])
            i = j + 1
        else:
            out.append(c)
            i += 1
    return "".join(out)


def directives(text):
    text = strip_comments(text.replace("\\\r\n", "").replace("\\\n", ""))
    for line in text.split("\n"):
        m = DIRECTIVE.match(line)
        if m:
            yield m.group(1), m.group(2).strip()


def includes_with_depth(text):
    """(kind, arg, depth) per include; depth excludes a classic include guard."""
    ds = list(directives(text))
    guard = False
    if len(ds) >= 2 and ds[0][0] == "ifndef" and ds[1][0] == "define":
        name = ds[0][1].split()[0] if ds[0][1] else ""
        d = GUARD_DEF.match(ds[1][1].split()[0] if ds[1][1] else "")
        if name and d and d.group(1) == name and ds[-1][0] == "endif":
            # the guard's #endif must close the first #ifndef
            depth = 0
            for k, (dname, _) in enumerate(ds):
                if dname in ("if", "ifdef", "ifndef"):
                    depth += 1
                elif dname == "endif":
                    depth -= 1
                    if depth == 0:
                        guard = k == len(ds) - 1
                        break
    depth, out = 0, []
    for dname, arg in ds:
        if dname in ("if", "ifdef", "ifndef"):
            depth += 1
        elif dname == "endif":
            depth = max(0, depth - 1)
        elif dname in ("include", "include_next", "import"):
            out.append((dname, arg, depth - (1 if guard else 0)))
    return out, guard


def form(arg):
    if arg.startswith('"'):
        j = arg.find('"', 1)
        return ("quoted", arg[1:j]) if j > 0 else ("macro", arg)
    if arg.startswith("<"):
        j = arg.find(">", 1)
        return ("angle", arg[1:j]) if j > 0 else ("macro", arg)
    return ("macro", arg)


def pct(a, b):
    return f"{a} ({100.0 * a / b:.1f}%)" if b else f"{a} (-)"


def main():
    clone, roots = sys.argv[1], sys.argv[2].split(",")
    tracked = subprocess.run(["git", "-C", clone, "ls-files", "-z"], capture_output=True,
                             check=True).stdout.decode().split("\0")
    tracked = [t for t in tracked if t]
    tset = set(tracked)
    by_suffix = collections.defaultdict(list)
    for t in tracked:
        parts = t.split("/")
        for k in range(len(parts)):
            by_suffix["/".join(parts[k:])].append(t)
    prefixes = tuple(sys.argv[3].split(",")) if len(sys.argv) > 3 else ("",)
    scanned = [t for t in tracked if posixpath.splitext(t)[1].lower() in EXTS
               and t.startswith(prefixes) and os.path.isfile(os.path.join(clone, t))]
    ext_count = collections.Counter(posixpath.splitext(t)[1].lower() for t in scanned)

    def under(root, target):
        p = posixpath.normpath(target if root == "." else posixpath.join(root, target))
        return p if p in tset else None

    c = collections.Counter()
    by_root = collections.Counter()
    angle_root = collections.Counter()
    unresolved = collections.Counter()
    ambiguous_ex = collections.Counter()
    macro_ex = collections.Counter()
    suffix_ex = collections.Counter()
    guards = 0
    files_with = collections.defaultdict(set)
    rr_inc = [r for r in roots if r in (".", "include")]
    for f in scanned:
        raw = open(os.path.join(clone, f), "rb").read().decode("utf-8", "replace")
        fdir = posixpath.dirname(f)
        file_inrepo, file_adj = 0, 0
        incs, guard = includes_with_depth(raw)
        guards += guard
        for kind, arg, depth in incs:
            fm, target = form(arg)
            c["all"] += 1
            c[fm] += 1
            if kind != "include":
                c["kind:" + kind] += 1
            files_with["any"].add(f)
            cond = depth > 0
            c["cond"] += cond
            inrepo = False
            if fm == "macro":
                macro_ex[arg] += 1
            elif fm == "quoted":
                files_with["quoted"].add(f)
                adj = posixpath.normpath(posixpath.join(fdir, target)) if fdir else posixpath.normpath(target)
                if adj in tset:
                    c["q_adj"] += 1
                    files_with["adj"].add(f)
                    inrepo = True
                    file_adj += 1
                else:
                    nsfx = len(by_suffix.get(posixpath.normpath(target).lstrip("./"), []))
                    c["q_nonadj_suffix" + ("0" if nsfx == 0 else "1" if nsfx == 1 else "N")] += 1
                    hits = {r: under(r, target) for r in roots}
                    hits = {r: p for r, p in hits.items() if p}
                    files = set(hits.values())
                    rri = {under(r, target) for r in rr_inc} - {None}
                    if len(rri) == 1:
                        c["q_rootinc_unique"] += 1
                    if len(files) == 1:
                        c["q_root"] += 1
                        by_root[next(iter(hits))] += 1
                        inrepo = True
                    elif len(files) > 1:
                        c["q_amb"] += 1
                        ambiguous_ex[target] += 1
                        inrepo = True
                    else:
                        sfx = by_suffix.get(posixpath.normpath(target).lstrip("./"), [])
                        if len(sfx) == 1:
                            c["q_unres_suffix1"] += 1
                            suffix_ex[f + " -> " + target] += 1
                        elif len(sfx) > 1:
                            c["q_unres_suffixN"] += 1
                        else:
                            unresolved[target] += 1
                        c["q_unres"] += 1
            else:
                hits = {r: under(r, target) for r in roots}
                hits = {r: p for r, p in hits.items() if p}
                if hits:
                    c["a_inrepo"] += 1
                    inrepo = True
                    if len(set(hits.values())) > 1:
                        c["a_amb"] += 1
                    for r in hits:
                        angle_root[r] += 1
            if inrepo:
                file_inrepo += 1
                if cond:
                    c["cond_inrepo"] += 1
        if file_inrepo:
            files_with["inrepo"].add(f)
            if file_adj == file_inrepo:
                files_with["inrepo_all_adj"].add(f)

    q = c["quoted"]
    print("scanned files:", len(scanned), dict(ext_count))
    print("files with >=1 include:", len(files_with["any"]))
    print("include lines:", c["all"], {k: v for k, v in c.items() if k.startswith("kind:")})
    print("  quoted:", pct(q, c["all"]), " angle:", pct(c["angle"], c["all"]), " macro:", pct(c["macro"], c["all"]))
    print("quoted resolution (of quoted):")
    print("  (a) adjacent:", pct(c["q_adj"], q))
    print("  (b) unique under one root:", pct(c["q_root"], q), dict(by_root))
    print("      of which root-or-include/ alone gives a unique file:", pct(c["q_rootinc_unique"], q))
    print("  non-adjacent, by path-suffix match over all tracked files: one",
          c["q_nonadj_suffix1"], "several", c["q_nonadj_suffixN"], "none", c["q_nonadj_suffix0"])
    print("  (c) ambiguous:", pct(c["q_amb"], q), ambiguous_ex.most_common(8))
    print("  (d) unresolved:", pct(c["q_unres"], q),
          "| a basename/suffix match finds 1:", c["q_unres_suffix1"], ">1:", c["q_unres_suffixN"], list(suffix_ex)[:8])
    print("      no tracked file:", unresolved.most_common(15))
    print("angle naming a tracked file:", pct(c["a_inrepo"], c["angle"]), dict(angle_root), "ambiguous:", c["a_amb"])
    print("macro examples:", macro_ex.most_common(8))
    print("files with a detected include guard:", guards,
          "headers:", sum(1 for f in scanned if f.endswith((".h", ".hh", ".hpp", ".hxx"))))
    print("conditional (depth>0, guard excluded):", pct(c["cond"], c["all"]),
          "of which in-repo:", c["cond_inrepo"])
    n = len(scanned)
    print("files with >=1 quoted:", pct(len(files_with["quoted"]), n))
    print("files with >=1 adjacent edge:", pct(len(files_with["adj"]), n))
    print("files with >=1 in-repo edge (any rule):", pct(len(files_with["inrepo"]), n))
    print("  of those, all in-repo edges adjacent:", pct(len(files_with["inrepo_all_adj"]), len(files_with["inrepo"])))


if __name__ == "__main__":
    main()
```

## Round 2 method

```sh
S=<scratchpad>/c-trial
git clone --depth 1 https://github.com/bitcoin/bitcoin $S/clones/bitcoin
git clone --depth 1 https://github.com/redis/redis     $S/clones/redis
for r in curl leveldb bitcoin redis; do
  python3 -I $S/scripts/grade_suffix.py $S/clones/$r $r
done
python3 -I $S/scripts/grade_suffix.py $S/clones/bitcoin bitcoin --time   # §E
```

What the script does, beyond round 1's scan:

- Includes are read as in round 1, with comments and line continuations handled the same way. It has no `#if` tracking: every branch counts.
- Step 2's pool is every tracked file of any extension. In the "ignored" run the vendored prefixes are removed from it, and from the scanned files.
- The yardstick (`truth`) is GCC's order over the hand-read roots in `CONFIG`. It always sees every tracked file, ignored or not, because the compiler does.
- Variant (iii) resolves an angle include against the union of every root in `CONFIG` for that repository.
- The step-2 suffix match does not strip `../` from an include path. `"../x.h"` that misses adjacent finds no candidates.

`grade_suffix.py`:

```python
"""Grade the suffix-match rule against each build's own -I roots, include by include.

Usage: python3 -I grade_suffix.py <clone> <repo> [--time]
<repo> picks a CONFIG entry. --time runs only the rule (list, read, resolve) and prints the time.
Reads files as text; runs nothing from the clone except `git ls-files`.
"""
import collections, os, posixpath, re, subprocess, sys, time

T0 = time.perf_counter()
EXTS = {".c", ".h", ".cc", ".cpp", ".cxx", ".c++", ".hpp", ".hh", ".hxx", ".h++",
        ".inl", ".ipp", ".tcc"}

# Per repo: (path prefix, -I roots in order); the longest matching prefix wins.
# Read by hand from the build files; the note cites the lines.
CONFIG = {
    "curl": {"vendored": [], "rules": [
        ("", ["include"]),
        ("lib/", ["include", "lib"]),
        ("src/", ["include", "lib", "src"]),
        ("tests/libtest/", ["include", "lib", "src", "tests/libtest"]),
        ("tests/unit/", ["include", "lib", "tests/libtest", "tests/unit"]),
        ("tests/tunit/", ["include", "lib", "src", "tests/libtest", "tests/tunit"]),
        ("tests/server/", ["include", "lib", "src", "tests/server"]),
        ("tests/perf/", ["include", "lib", "src", "tests/perf"]),
        ("projects/OS400/", ["include/curl", "include", "lib", "projects/OS400"]),
    ]},
    "leveldb": {"vendored": [], "rules": [("", [".", "include"])]},
    "bitcoin": {
        "vendored": ["src/crc32c/", "src/crypto/ctaes/", "src/ipc/libmultiprocess/",
                     "src/leveldb/", "src/minisketch/", "src/secp256k1/"],
        "rules": [
            ("", ["src", "src/leveldb/include", "src/crc32c/include", "src/minisketch/include",
                  "src/secp256k1/include", "src/univalue/include", "src/ipc/libmultiprocess/include"]),
            ("src/leveldb/", ["src/leveldb", "src/leveldb/include", "src", "src/crc32c/include"]),
            ("src/crc32c/", ["src/crc32c/include", "src"]),
            ("src/minisketch/", ["src/minisketch/include", "src"]),
            ("src/secp256k1/", ["src/secp256k1/include"]),
            ("src/univalue/", ["src/univalue/include", "src"]),
            ("src/ipc/libmultiprocess/", ["src/ipc/libmultiprocess/include"]),
            ("src/ipc/libmultiprocess/example/", ["src/ipc/libmultiprocess/include",
                                                  "src/ipc/libmultiprocess/example"]),
            ("src/ipc/libmultiprocess/test/", ["src/ipc/libmultiprocess/include",
                                               "src/ipc/libmultiprocess/test"]),
        ]},
    "redis": {
        "vendored": ["deps/croaring/", "deps/fpconv/", "deps/hdr_histogram/", "deps/hiredis/", "deps/jemalloc/",
                     "deps/linenoise/", "deps/lua/", "deps/tre/", "deps/xxhash/"],
        "rules": [
            ("", []),
            ("src/", ["deps/hiredis", "deps/linenoise", "deps/lua/src", "deps/hdr_histogram",
                      "deps/fpconv", "deps/xxhash", "deps/croaring/include", "deps/jemalloc/include"]),
            ("deps/croaring/", ["deps/croaring/include", "deps/croaring/src"]),
            ("deps/tre/", ["deps/tre/local_includes", "deps/tre/lib"]),
            ("deps/jemalloc/", ["deps/jemalloc/include"]),
            ("deps/jemalloc/test/", ["deps/jemalloc/include", "deps/jemalloc/test/include"]),
            ("deps/hiredis/", ["deps/hiredis"]),
            ("deps/lua/etc/", ["deps/lua/src"]),
            ("tests/modules/", ["src"]),
            ("src/modules/", ["src/modules"]),
            ("modules/vector-sets/", ["modules/vector-sets"]),
        ]},
}

# C (C23) and C++ standard headers: https://en.cppreference.com/w/c/header, /w/cpp/header.
# POSIX: https://pubs.opengroup.org/onlinepubs/9799919799/idx/head.html.
STD = set("""assert.h complex.h ctype.h errno.h fenv.h float.h inttypes.h iso646.h limits.h
locale.h math.h setjmp.h signal.h stdalign.h stdarg.h stdatomic.h stdbit.h stdbool.h stdckdint.h
stddef.h stdint.h stdio.h stdlib.h stdnoreturn.h string.h tgmath.h threads.h time.h uchar.h
wchar.h wctype.h aio.h arpa/inet.h cpio.h devctl.h dirent.h dlfcn.h endian.h fcntl.h fmtmsg.h
fnmatch.h ftw.h glob.h grp.h iconv.h langinfo.h libintl.h libgen.h monetary.h mqueue.h ndbm.h
net/if.h netdb.h netinet/in.h netinet/tcp.h nl_types.h poll.h pthread.h pwd.h regex.h sched.h
search.h semaphore.h spawn.h strings.h syslog.h tar.h termios.h unistd.h utmpx.h wordexp.h
algorithm any array atomic barrier bit bitset cassert ccomplex cctype cerrno cfenv cfloat charconv
chrono cinttypes ciso646 climits clocale cmath codecvt compare complex concepts condition_variable
contracts coroutine csetjmp csignal cstdalign cstdarg cstdbool cstddef cstdint cstdio cstdlib
cstring ctgmath ctime cuchar cwchar cwctype debugging deque exception execution expected
filesystem flat_map flat_set format forward_list fstream functional future generator
hazard_pointer hive initializer_list inplace_vector iomanip ios iosfwd iostream istream iterator
latch limits linalg list locale map mdspan memory memory_resource meta mutex new numbers numeric
optional ostream print queue random ranges ratio rcu regex scoped_allocator semaphore set
shared_mutex simd source_location span spanstream sstream stack stacktrace stdexcept stdfloat
stop_token streambuf string string_view strstream syncstream system_error text_encoding thread
tuple type_traits typeindex typeinfo unordered_map unordered_set utility valarray variant vector
version""".split())
# Common platform headers outside those standards (my own short list).
PLATFORM = set("""byteswap.h malloc.h alloca.h execinfo.h features.h cpuid.h windows.h winsock2.h
ws2tcpip.h io.h intrin.h immintrin.h x86intrin.h emmintrin.h arm_neon.h""".split())
SYS_PREFIX = ("sys/", "linux/", "mach/", "machine/", "bits/", "netinet/", "arpa/", "net/")


def is_system_name(t):
    return t in STD or t in PLATFORM or t.startswith(SYS_PREFIX)


DIRECTIVE = re.compile(r"^\s*#\s*(include|include_next|import)\b\s*(.*)$")


def strip_comments(text):
    out, i, n = [], 0, len(text)
    while i < n:
        c = text[i]
        if c == "/" and i + 1 < n and text[i + 1] == "*":
            j = text.find("*/", i + 2)
            j = n if j < 0 else j + 2
            out.append(re.sub(r"[^\n]", " ", text[i:j]))
            i = j
        elif c == "/" and i + 1 < n and text[i + 1] == "/":
            j = text.find("\n", i)
            i = n if j < 0 else j
        elif c in "\"'":
            j = i + 1
            while j < n and text[j] != c and text[j] != "\n":
                j += 2 if text[j] == "\\" else 1
            out.append(text[i:j + 1])
            i = j + 1
        else:
            out.append(c)
            i += 1
    return "".join(out)


def includes(text):
    text = strip_comments(text.replace("\\\r\n", "").replace("\\\n", ""))
    for line in text.split("\n"):
        m = DIRECTIVE.match(line)
        if not m:
            continue
        arg = m.group(2).strip()
        if arg.startswith('"') and arg.find('"', 1) > 0:
            yield "q", arg[1:arg.find('"', 1)]
        elif arg.startswith("<") and arg.find(">", 1) > 0:
            yield "a", arg[1:arg.find(">", 1)]


def suffix_index(paths):
    idx = collections.defaultdict(list)
    for t in paths:
        parts = t.split("/")
        for k in range(len(parts)):
            idx["/".join(parts[k:])].append(t)
    return idx


def common_depth(a, b):
    a, b = posixpath.dirname(a).split("/"), posixpath.dirname(b).split("/")
    n = 0
    while n < min(len(a), len(b)) and a[n] == b[n] and a[n]:
        n += 1
    return n


def main():
    clone, repo = sys.argv[1], sys.argv[2]
    timing = "--time" in sys.argv
    cfg = CONFIG[repo]
    tracked = [t for t in subprocess.run(["git", "-C", clone, "ls-files", "-z"], capture_output=True,
               check=True).stdout.decode().split("\0") if t]
    tset = set(tracked)
    vend = tuple(cfg["vendored"])
    scanned = [t for t in tracked if posixpath.splitext(t)[1].lower() in EXTS
               and os.path.isfile(os.path.join(clone, t))]
    parsed = {}
    for f in scanned:
        with open(os.path.join(clone, f), "rb") as fh:
            parsed[f] = list(includes(fh.read().decode("utf-8", "replace")))
    pools = {"in": tracked}
    if vend:
        pools["ignored"] = [t for t in tracked if not t.startswith(vend)]
    idx = {k: suffix_index(v) for k, v in pools.items()}
    pset = {k: set(v) for k, v in pools.items()}
    all_roots = sorted({r for _, rs in cfg["rules"] for r in rs})

    def step2(f, form, target, pool, variant):
        """('line', file) | ('unsure', [files]) | ('none', None) | ('skip', None)."""
        d = posixpath.dirname(f)
        if form == "q":
            adj = posixpath.normpath(posixpath.join(d, target))
            if adj in pset[pool]:
                return ("adj", adj)
        if form == "a" and variant == "ii" and "/" not in target:
            return ("skip", None)
        if form == "a" and variant == "iii":
            c = sorted({p for r in all_roots
                        for p in [posixpath.normpath(target if r == "." else posixpath.join(r, target))]
                        if p in pset[pool]})
        else:
            c = sorted(idx[pool].get(posixpath.normpath(target), []))
        if len(c) == 1:
            return ("line", c[0])
        return ("unsure", c) if c else ("none", None)

    if timing:
        n = 0
        for f, incs in parsed.items():
            for form, target in incs:
                step2(f, form, target, "in", "i")
                n += 1
        print(f"{repo}: {len(tracked)} tracked, {len(scanned)} scanned, {n} includes, "
              f"{time.perf_counter() - T0:.3f} s (interpreter start excluded)")
        return

    rules = sorted(cfg["rules"], key=lambda r: -len(r[0]))

    def truth(f, form, target):
        if form == "q":
            adj = posixpath.normpath(posixpath.join(posixpath.dirname(f), target))
            if adj in tset:
                return adj
        roots = next(rs for p, rs in rules if f.startswith(p))
        for r in roots:
            p = posixpath.normpath(target if r == "." else posixpath.join(r, target))
            if p in tset:
                return p
        return None

    print(f"== {repo}: {len(tracked)} tracked, {len(scanned)} scanned, "
          f"{sum(len(v) for v in parsed.values())} include lines (quoted+angle)")

    def grade(pool, variant, only_first_party):
        g = collections.Counter()
        wrong, sysflag, bare, unsures, outcomes = [], collections.Counter(), collections.Counter(), [], {}
        for f, incs in parsed.items():
            if (only_first_party or pool == "ignored") and vend and f.startswith(vend):
                continue
            for k, (form, target) in enumerate(incs):
                s, val = step2(f, form, target, pool, variant)
                t = truth(f, form, target)
                outcomes[(f, k)] = (s if s != "adj" else "line", val if s != "unsure" else tuple(val))
                key = form + ":"
                if pool == "ignored" and t and t.startswith(vend):
                    g[key + "-I target is ignored, step 2 gives " + s] += 1
                    if s == "line":
                        wrong.append(("line into ignored target's place", f, form, target, val, t))
                    continue
                if s == "adj":
                    g[key + "adjacent"] += 1
                    continue
                if s == "skip":
                    g[key + "skipped"] += 1
                    continue
                if s == "line":
                    cat = "agree" if t == val else ("wrong" if t else "line, -I none")
                    if cat != "agree":
                        wrong.append((cat, f, form, target, val, t))
                        g["wrong by origin: " + ("vendored" if vend and f.startswith(vend) else "first-party")] += 1
                    if val == f:
                        g["self-match line"] += 1
                    if form == "a" and is_system_name(target):
                        sysflag["in-repo" if t == val else "wrongly matched"] += 1
                    if form == "q" and "/" not in target:
                        bare[cat] += 1
                elif s == "unsure":
                    cat = "unsure, -I finds one" if t else "unsure, -I none"
                    unsures.append((f, target, val, t))
                    if form == "a" and is_system_name(target):
                        sysflag["unsure"] += 1
                else:
                    cat = "none, -I finds one" if t else "both none"
                g[key + cat] += 1
        return g, wrong, sysflag, bare, unsures, outcomes

    for pool in pools:
        for variant in ("i", "ii", "iii"):
            g, wrong, sysflag, bare, unsures, _ = grade(pool, variant, False)
            print(f"-- pool={pool} variant={variant}")
            for form in ("q", "a"):
                if variant != "i" and form == "q":
                    continue
                print("  ", form, {k[2:]: v for k, v in sorted(g.items()) if k.startswith(form + ":")})
            print("   angle names that are standard/system headers, matched:", dict(sysflag))
            print("   ", {k: v for k, v in g.items() if k.startswith(("wrong by", "self"))})
            for side in ("first-party", "vendored"):
                ws = [w for w in wrong if (bool(vend) and w[1].startswith(vend)) == (side == "vendored")]
                print(f"   wrong or unbacked lines from {side} files ({len(ws)}), by name:",
                      collections.Counter((w[2], w[3], w[4]) for w in ws).most_common(30))
            if variant == "i":
                print("   quoted bare names, non-adjacent, step-2 line:", dict(bare))
                # tie-breaker: deepest shared folder with the including file
                tb = collections.Counter()
                sp = collections.Counter()
                twrong = collections.Counter()
                for f, target, cands, t in unsures:
                    shortest = min(len(c) for c in cands)
                    stop = [c for c in cands if len(c) == shortest]
                    sp["still tied" if len(stop) > 1 else "picked, -I none" if t is None
                       else "right" if stop[0] == t else "wrong"] += 1
                    best = max(common_depth(f, c) for c in cands)
                    top = [c for c in cands if common_depth(f, c) == best]
                    if len(top) > 1:
                        tb["still tied"] += 1
                    elif t is None:
                        tb["picked, -I none"] += 1
                    else:
                        tb["right" if top[0] == t else "wrong"] += 1
                        if top[0] != t:
                            twrong[(target, top[0], t)] += 1
                print("   tie-breaker (closest) on", len(unsures), "unsures:", dict(tb))
                print("     closest picks the wrong file, top:", twrong.most_common(8))
                print("     self among unsure candidates:", sum(1 for u in unsures if u[0] in u[2]))
                print("   tie-breaker (shortest path, extra) on", len(unsures), "unsures:", dict(sp))
                for u in unsures[:6]:
                    print("      unsure sample:", u[0], u[1], u[2][:4], "| -I:", u[3])

    if vend:
        g_in, _, sf_in, _, _, o_in = grade("in", "i", True)
        g_ig, _, sf_ig, _, _, o_ig = grade("ignored", "i", True)
        print("-- first-party files only, vendored in:", dict(sorted(g_in.items())), "system names:", dict(sf_in))
        print("-- first-party files only, vendored ignored:", dict(sorted(g_ig.items())), "system names:", dict(sf_ig))
        ch = collections.Counter()
        for k in o_in:
            a, b = o_in[k], o_ig[k]
            if a != b:
                ch[f"{a[0]} -> {b[0]}"] += 1
        print("-- first-party includes, vendored in vs ignored: changed", dict(ch), "of", len(o_in))
        out = collections.Counter()
        for f, incs in parsed.items():
            if not f.startswith(vend):
                continue
            own = next(v for v in vend if f.startswith(v))
            for form, target in incs:
                s, val = step2(f, form, target, "in", "i")
                t = truth(f, form, target)
                if s in ("adj", "line"):
                    out["step2 line inside own tree" if val.startswith(own) else "step2 line outside own tree"] += 1
                if s == "unsure":
                    out["unsure"] += 1
                if t:
                    out["-I inside own tree" if t.startswith(own) else "-I outside own tree"] += 1
        print("-- vendored files' own includes:", dict(out))


if __name__ == "__main__":
    main()
```

## Round 3 method

`python3 -I $S/scripts/grade_r3.py $S/clones/<repo> <repo>` for each of the four repositories. It loads `grade_suffix.py` from its own folder for `CONFIG`, the reader and the system-name list.

`grade_r3.py`:

```python
"""Round 3: the suffix rule with system names never matched and vendored trees ignored, plus
test skipping (1), no own-folder candidate for angle includes (2), ignored files that block (3),
and Unsure counted in the map's units (4). Graded against round 2's -I yardstick.

Usage: python3 -I grade_r3.py <clone> <repo>
Reuses grade_suffix.py (same folder) for CONFIG, the reader and the system-name list.
"""
import collections, os, posixpath, re, runpy, subprocess, sys

G = runpy.run_path(os.path.join(os.path.dirname(os.path.abspath(__file__)), "grade_suffix.py"))
TEST_DIRS = {"test", "tests", "testing", "unittest", "fuzz"}
TEST_FILE = re.compile(r"(_test\.|_tests\.|^test_.*\.)")


def is_test(path):
    parts = path.split("/")
    return any(p in TEST_DIRS for p in parts[:-1]) or bool(TEST_FILE.search(parts[-1]))


def main():
    clone, repo = sys.argv[1], sys.argv[2]
    cfg = G["CONFIG"][repo]
    vend = tuple(cfg["vendored"])
    tracked = [t for t in subprocess.run(["git", "-C", clone, "ls-files", "-z"], capture_output=True,
               check=True).stdout.decode().split("\0") if t]
    tset = set(tracked)
    scanned = [t for t in tracked if posixpath.splitext(t)[1].lower() in G["EXTS"]
               and os.path.isfile(os.path.join(clone, t))]
    parsed = {}
    for f in scanned:
        with open(os.path.join(clone, f), "rb") as fh:
            parsed[f] = list(G["includes"](fh.read().decode("utf-8", "replace")))
    idx = G["suffix_index"](tracked)
    rules = sorted(cfg["rules"], key=lambda r: -len(r[0]))

    def truth(f, form, target):
        if form == "q":
            adj = posixpath.normpath(posixpath.join(posixpath.dirname(f), target))
            if adj in tset:
                return adj
        for r in next(rs for p, rs in rules if f.startswith(p)):
            p = posixpath.normpath(target if r == "." else posixpath.join(r, target))
            if p in tset:
                return p
        return None

    def run(skip_tests=False, no_own_folder=False, block=False):
        ignored = lambda p: bool(vend) and p.startswith(vend)
        skipped = lambda p: skip_tests and is_test(p)
        out = {}
        for f, incs in parsed.items():
            if ignored(f) or skipped(f):
                continue
            d = posixpath.dirname(f)
            for k, (form, target) in enumerate(incs):
                t = truth(f, form, target)
                if form == "q":
                    adj = posixpath.normpath(posixpath.join(d, target))
                    if adj in tset and not ignored(adj) and not skipped(adj):
                        out[(f, k)] = ("adj", adj, target, t)
                        continue
                if G["is_system_name"](target):
                    out[(f, k)] = ("none", None, target, t)
                    continue
                c = [p for p in idx.get(posixpath.normpath(target), []) if not skipped(p)]
                if no_own_folder and form == "a":
                    c = [p for p in c if p != posixpath.normpath(posixpath.join(d, target))]
                live = [p for p in c if not ignored(p)]
                if block and len(c) > len(live):          # an ignored candidate blocks a line
                    r = ("unsure", tuple(sorted(c))) if live else ("none", None)
                elif len(live) == 1:
                    r = ("line", live[0])
                elif live:
                    r = ("unsure", tuple(sorted(live)))
                else:
                    r = ("none", None)
                out[(f, k)] = (r[0], r[1], target, t)
        return out, ignored, skipped

    def grade(res, ignored, skipped):
        g = collections.Counter()
        for (f, k), (s, val, target, t) in res.items():
            gone = t is not None and (ignored(t) or skipped(t))   # the map draws no line to it
            if s == "adj":
                g["adjacent"] += 1
            elif s == "line":
                g["right" if (t == val and not gone) else "wrong"] += 1
            elif s == "unsure":
                g["unsure"] += 1
            elif t is not None and not gone:
                g["missed (-I finds a live file)"] += 1
            else:
                g["none, correct"] += 1
        return g

    def wrongs(res, ignored, skipped):
        return {key: v for key, v in res.items()
                if v[0] == "line" and not (v[3] == v[1] and not (ignored(v[3]) or skipped(v[3])))}

    def unsure_units(res):
        u = [(f, v[2]) for (f, k), v in res.items() if v[0] == "unsure"]
        pairs = set(u)
        names = collections.Counter(n for _, n in pairs)
        return len(u), len(pairs), len({f for f, _ in pairs}), names.most_common(10)

    print(f"== {repo}")
    base = run()
    b1 = run(skip_tests=True)
    b2 = run(no_own_folder=True)
    b3 = run(block=True)
    b12 = run(skip_tests=True, no_own_folder=True)
    for label, r in (("base", base), ("1 skip tests", b1), ("2 no own folder", b2), ("3 ignored block", b3),
                     ("1+2", b12)):
        print(f"-- {label}:", dict(sorted(grade(*r).items())))
    w0 = wrongs(*base)
    print("   base wrong lines:", collections.Counter((v[2], v[1]) for v in w0.values()).most_common(10))

    # 1: what changes between base and skip-tests, for files both runs read
    ch = collections.Counter()
    for key, v in b1[0].items():
        a = base[0][key]
        if a[:2] != v[:2]:
            ch[f"{a[0]} -> {v[0]}"] += 1
    print("   1: changed, sample:", collections.Counter(
        (base[0][key][0], v[0], key[0].split("/")[1] if "/" in key[0] else key[0], v[2])
        for key, v in b1[0].items() if base[0][key][:2] != v[:2] and v[0] != "unsure").most_common(8))
    w1 = wrongs(*b1)
    print("   1: outcome changes in non-test files:", dict(ch))
    print("   1: wrong lines in base / skip-tests:", len(w0), len(w1),
          "| new wrong:", [(k[0], v[2], v[1]) for k, v in w1.items() if k not in w0][:10],
          "| skipped test files:", sum(1 for f in parsed if is_test(f) and not (vend and f.startswith(vend))))
    # 2: wrong lines removed, right lines lost
    w2 = wrongs(*b2)
    lost = [(k[0], v[2], v[1]) for k, v in base[0].items()
            if v[0] == "line" and k not in w0 and b2[0][k][0] != "line"]
    print("   2: wrong removed:", [(k[0], v[2], v[1]) for k, v in w0.items() if k not in w2],
          "| wrong added:", len([k for k in w2 if k not in w0]),
          collections.Counter((w2[k][2], w2[k][1], w2[k][3]) for k in w2 if k not in w0).most_common(6))
    print("   2: right lines lost:", len(lost), "| by kind:",
          collections.Counter(b2[0][(f, k)][0] for (f, k), v in base[0].items()
                              if v[0] == "line" and (f, k) not in w0 and b2[0][(f, k)][0] != "line"),
          "| sample:", lost[:5])
    for lab, a, b in (("base vs 2", base, b2), ("1 vs 1+2", b1, b12)):
        wa, wb = wrongs(*a), wrongs(*b)
        right = lambda r, w, k: r[0][k][0] == "line" and k not in w
        print(f"   2 ({lab}): right lost", sum(1 for k in a[0] if right(a, wa, k) and not right(b, wb, k)),
              "| right gained", sum(1 for k in a[0] if right(b, wb, k) and not right(a, wa, k)),
              "| wrong added", sum(1 for k in wb if k not in wa), "| wrong removed", sum(1 for k in wa if k not in wb))
    w3 = wrongs(*b3)
    print("   3: wrong dropped/block:", len(w0), len(w3), "| block wrong:",
          collections.Counter((v[2], v[1]) for v in w3.values()).most_common(5))
    for label, r in (("base", base), ("1 skip tests", b1)):
        n, pairs, files, top = unsure_units(r[0])
        print(f"   4 {label}: unsure include lines {n}, distinct (file, name) {pairs}, files {files}")
        print("      top names:", top)


if __name__ == "__main__":
    main()
```

## Round 4 method

`python3 -I $S/scripts/grade_r4.py $S/clones/<repo> <repo>` for each of the four repositories. It loads `grade_suffix.py` and `grade_r3.py` from its own folder.

`grade_r4.py`:

```python
"""Round 4: new base case (system names for <...> only, Windows names added, tests skipped,
ignored files block, vendored ignored), then part links on a proxy map (1), inferred roots (2),
roots read from build files as text (3), declared roots (4) and test-skip risk (5).

Usage: python3 -I grade_r4.py <clone> <repo>
Loads grade_suffix.py (CONFIG = the -I yardstick, reader, system list) and grade_r3.py (is_test).
"""
import collections, os, posixpath, re, runpy, subprocess, sys

HERE = os.path.dirname(os.path.abspath(__file__))
G = runpy.run_path(os.path.join(HERE, "grade_suffix.py"))
R3 = runpy.run_path(os.path.join(HERE, "grade_r3.py"))
is_test = R3["is_test"]
# Windows CRT and SDK headers, plus memory.h (see the round 4 report for the citations).
WIN = set("""process.h schannel.h windows.h winsock2.h ws2tcpip.h io.h direct.h malloc.h conio.h
tchar.h memory.h""".split())
DECLARED = {
    "curl": [["include", "lib"]],
    "leveldb": [[".", "include"]],
    "bitcoin": [["src"]],
    "redis": [["src", "deps/hiredis", "deps/linenoise", "deps/lua/src", "deps/hdr_histogram", "deps/fpconv",
               "deps/xxhash", "deps/croaring/include", "deps/jemalloc/include"],
              ["deps/hiredis", "deps/linenoise", "deps/lua/src", "deps/hdr_histogram", "deps/fpconv",
               "deps/xxhash", "deps/croaring/include", "deps/jemalloc/include", "src"]],
}


def norm(p):
    return posixpath.normpath(p)


def join(root, target):
    return norm(target if root in (".", "") else posixpath.join(root, target))


def sysname(t):
    return G["is_system_name"](t) or t in WIN


# ---- roots read from build files as text (item 3) ----
CM_BLOCK = re.compile(r"\b(include_directories|target_include_directories)\s*\(([^)]*(?:\)[^)]*?)*?)\)", re.S)
CM_KEYWORDS = {"PUBLIC", "PRIVATE", "INTERFACE", "SYSTEM", "BEFORE", "AFTER"}


def cmake_roots(text, folder, project_dir):
    roots = []
    sets = dict(re.findall(r'set\(\s*(\w+)\s+"?([^")\s]+)"?\s*\)', text))
    for m in re.finditer(r"\b(include_directories|target_include_directories)\s*\(", text):
        depth, i = 1, m.end()
        while i < len(text) and depth:
            depth += {"(": 1, ")": -1}.get(text[i], 0)
            i += 1
        body = text[m.end():i - 1]
        body = re.sub(r"\$<INSTALL_INTERFACE:[^>]*>", " ", body)
        body = re.sub(r"\$<BUILD_INTERFACE:([^>]*)>", r"\1", body)
        body = re.sub(r"#[^\n]*", " ", body)
        toks = re.findall(r'"[^"]*"|\S+', body)
        if m.group(1) == "target_include_directories" and toks:
            toks = toks[1:]
        for t in toks:
            t = t.strip('"')
            if t in CM_KEYWORDS or not t:
                continue
            for _ in range(3):
                t = re.sub(r"\$\{(\w+)\}", lambda v: sets.get(v.group(1), "${%s}" % v.group(1)), t)
            if "BINARY_DIR" in t or "$<" in t:
                continue
            anchored = re.match(r"\$\{(PROJECT_SOURCE_DIR|CMAKE_SOURCE_DIR|CMAKE_CURRENT_SOURCE_DIR|CMAKE_CURRENT_LIST_DIR)\}", t)
            t = (t.replace("${PROJECT_SOURCE_DIR}", project_dir).replace("${CMAKE_SOURCE_DIR}", ".")
                  .replace("${CMAKE_CURRENT_SOURCE_DIR}", folder).replace("${CMAKE_CURRENT_LIST_DIR}", folder))
            if "${" in t or t.startswith("/"):
                continue
            roots.append(norm(t) if anchored else join(folder, t))
    return roots


def make_roots(text, folder, top):
    text = text.replace("\\\n", " ")
    roots = []
    for t in re.findall(r"(?<![\w$])-I\s*([^\s'\"`]+)", text):
        anchored = t.startswith(("$(top_srcdir)", "$(srcdir)"))
        t = t.replace("$(top_srcdir)", top or ".").replace("$(srcdir)", folder or ".")
        if t.startswith("/") or "$(" in t or "${" in t or "@" in t:
            continue
        roots.append(norm(t) if anchored else join(folder, t))
    return roots


def build_roots(clone, tracked):
    """{folder: [roots]} for every tracked build file that names -I roots, read as text."""
    out = collections.defaultdict(list)
    cm_projects = set()
    for t in tracked:
        if posixpath.basename(t) == "CMakeLists.txt":
            txt = open(os.path.join(clone, t), errors="replace").read()
            if re.search(r"^\s*project\s*\(", txt, re.M | re.I):
                cm_projects.add(posixpath.dirname(t))
    am_tops = {posixpath.dirname(t) for t in tracked if posixpath.basename(t) in ("configure.ac", "configure.in")}

    def nearest(folder, pool):
        f = folder
        while True:
            if f in pool:
                return f or "."
            if not f:
                return "."
            f = posixpath.dirname(f)

    for t in tracked:
        b = posixpath.basename(t)
        folder = posixpath.dirname(t)
        try:
            if b == "CMakeLists.txt" or b.endswith(".cmake"):
                txt = open(os.path.join(clone, t), errors="replace").read()
                own = folder if not b.endswith(".cmake") else ""
                rs = cmake_roots(txt, own if b == "CMakeLists.txt" else ".", nearest(own, cm_projects))
            elif b in ("Makefile", "Makefile.am", "GNUmakefile", "Makefile.in") or b.endswith(".mk"):
                txt = open(os.path.join(clone, t), errors="replace").read()
                rs = make_roots(txt, folder, nearest(folder, am_tops) if b.endswith((".am", ".in")) else ".")
            else:
                continue
        except OSError:
            continue
        rs = [r for r in (x.strip() or "." for x in rs)
              if r == "." or any(p.startswith(r + "/") for p in tracked)]
        if rs:
            key = folder if not b.endswith(".cmake") else ""
            out[key].extend(r for r in rs if r not in out[key])
    return out


def main():
    clone, repo = sys.argv[1], sys.argv[2]
    cfg = G["CONFIG"][repo]
    vend = tuple(cfg["vendored"])
    tracked = [t for t in subprocess.run(["git", "-C", clone, "ls-files", "-z"], capture_output=True,
               check=True).stdout.decode().split("\0") if t]
    tset = set(tracked)
    ignored = lambda p: bool(vend) and p.startswith(vend)
    skipped = lambda p: is_test(p)
    live = lambda p: not ignored(p) and not skipped(p)
    scanned = [t for t in tracked if posixpath.splitext(t)[1].lower() in G["EXTS"]
               and os.path.isfile(os.path.join(clone, t))]
    parsed = {}
    for f in scanned:
        with open(os.path.join(clone, f), "rb") as fh:
            parsed[f] = list(G["includes"](fh.read().decode("utf-8", "replace")))
    first = [f for f in scanned if live(f)]
    idx = G["suffix_index"](tracked)
    rules = sorted(cfg["rules"], key=lambda r: -len(r[0]))

    def truth(f, form, target):
        if form == "q":
            adj = join(posixpath.dirname(f), target)
            if adj in tset:
                return adj
        for r in next(rs for p, rs in rules if f.startswith(p)):
            p = join(r, target)
            if p in tset:
                return p
        return None

    def step2(form, target):
        if form == "a" and sysname(target):
            return ("none", None, ())
        cands = sorted(p for p in idx.get(norm(target), []) if not skipped(p))
        livec = [p for p in cands if not ignored(p)]
        if len(cands) > len(livec):
            return ("unsure", None, tuple(cands)) if livec else ("none", None, ())
        if len(livec) == 1:
            return ("line", livec[0], ())
        return ("unsure", None, tuple(livec)) if livec else ("none", None, ())

    def hit(p):
        return ("line", p, ()) if live(p) else ("none", None, ())

    def resolve(f, form, target, roots=None, roots_mode=None):
        """roots_mode: None (base), 'declared' (ordered, both forms, then step 2),
        'union' (angle only through any root; several distinct -> Unsure)."""
        if form == "q":
            adj = join(posixpath.dirname(f), target)
            if adj in tset:
                return ("adj",) + hit(adj)[1:] if live(adj) else ("none", None, ())
        if roots_mode == "declared":
            for r in roots:
                p = join(r, target)
                if p in tset and not skipped(p):
                    return hit(p)
        if roots_mode == "union" and form == "a":
            if sysname(target):
                return ("none", None, ())
            hits = sorted({join(r, target) for r in roots} & tset)
            hits = [p for p in hits if not skipped(p)]
            if len(hits) == 1:
                return hit(hits[0])
            livec = [p for p in hits if not ignored(p)]
            return ("unsure", None, tuple(hits)) if livec and len(hits) > 1 else ("none", None, ())
        return step2(form, target)

    def run(**kw):
        res = {}
        for f in first:
            rs = kw.get("roots_for", lambda f: None)(f)
            for k, (form, target) in enumerate(parsed[f]):
                res[(f, k)] = resolve(f, form, target, rs, kw.get("roots_mode")) + (form, target, truth(f, form, target))
        return res

    def grade(res):
        g = collections.Counter()
        for (f, k), (s, val, cands, form, target, t) in res.items():
            tl = t is not None and live(t)
            if s in ("adj", "line"):
                g["right" if (val == t and tl) else "wrong"] += 1
                if s == "adj":
                    g["(adjacent)"] += 1
            elif s == "unsure":
                g["unsure"] += 1
            else:
                g["missed" if tl else "none ok"] += 1
        units = {(f, r[4]) for (f, k), r in res.items() if r[0] == "unsure"}
        g["unsure findings"] = len(units)
        g["unsure files"] = len({f for f, _ in units})
        return g

    def missed_list(res):
        return collections.Counter((r[4], r[5]) for r in res.values()
                                   if r[0] == "none" and r[5] and live(r[5])).most_common(5)

    def unsure_list(res):
        return collections.Counter((r[4], r[2][:3]) for r in res.values() if r[0] == "unsure").most_common(4)

    def wrong_list(res):
        return collections.Counter((r[4], r[1], f) for (f, k), r in res.items()
                                   if r[0] in ("adj", "line") and not (r[1] == r[5] and live(r[5])))

    print(f"== {repo}: {len(first)} first-party live files")
    base = run()
    gb = grade(base)
    print("BASE", dict(sorted(gb.items())))
    print("BASE wrong:", wrong_list(base).most_common(12))
    print("BASE missed:", collections.Counter((r[4], r[5]) for r in base.values()
                                              if r[0] == "none" and r[5] and live(r[5])).most_common(8))

    # ---- item 1: part links on a proxy map ----
    def part_fn(kind):
        if repo == "bitcoin":
            def p(f):
                if not f.startswith("src/"):
                    return None
                rest = f[4:]
                return "src/" + rest.split("/")[0] if "/" in rest else "core"
            return p
        if repo == "redis":
            def p(f):
                if f.startswith("src/modules/"):
                    return "src/modules"
                if f.startswith("modules/"):
                    return "/".join(f.split("/")[:2])
                return f.split("/")[0] if "/" in f else "(root)"
            return p
        return lambda f: f.split("/")[0] if "/" in f else "(root)"

    def fold(p, cap=15):
        sizes = collections.Counter(p(f) for f in tracked if live(f) and p(f)
                                    and posixpath.splitext(f)[1].lower() in G["EXTS"])
        if len(sizes) <= cap:
            return p, sizes, {}
        keep = {n for n, _ in sizes.most_common(cap - 1)}
        folded = sorted(n for n in sizes if n not in keep)
        return (lambda f: (p(f) if p(f) in keep else "(folded)") if p(f) else None), sizes, folded

    def part_links(p, label):
        ys = collections.defaultdict(set)      # part pair -> yardstick file pairs
        proved = collections.defaultdict(set)
        unproven = collections.defaultdict(set)
        false = collections.defaultdict(set)
        for (f, k), (s, val, cands, form, target, t) in base.items():
            pf = p(f)
            if pf is None:
                continue
            if t is not None and live(t) and p(t) and p(t) != pf:
                pair = (pf, p(t))
                ys[pair].add((f, t))
                if s in ("adj", "line") and val == t:
                    proved[pair].add((f, t))
                elif s == "unsure" and t in cands:
                    unproven[pair].add((f, t))
            if s in ("adj", "line") and not (val == t and t is not None and live(t)) and p(val) and p(val) != pf:
                false[(pf, p(val))].add((f, val))
        pp = set(proved)
        up = {q for q in unproven if q not in pp}
        lost = {q for q in ys if q not in pp and q not in up}
        print(f"-- 1 {label}: yardstick part pairs {len(ys)}, proved {len(pp)}, only unproven {len(up)}, lost {len(lost)},"
              f" false pairs from wrong lines {len([q for q in false if q not in ys])}")
        lines = sorted(len(v) for v in ys.values())
        print("   lines per yardstick pair: min/median/max", lines[0] if lines else 0,
              lines[len(lines) // 2] if lines else 0, lines[-1] if lines else 0,
              "| pairs resting on 1 line:", sum(1 for x in lines if x == 1))
        for q in sorted(up):
            print("   UNPROVEN", q, "lines", len(ys[q]), "unsure-backed", len(unproven[q]),
                  "e.g.", sorted(unproven[q])[:2])
        for q in sorted(lost):
            print("   LOST", q, "lines", len(ys[q]), "e.g.", sorted(ys[q])[:2])
        for q in sorted(false):
            if q not in ys:
                print("   FALSE", q, sorted(false[q])[:2])
        # pairs proved, with how many proving lines vs yardstick lines
        thin = [(q, len(proved[q]), len(ys[q])) for q in pp if len(proved[q]) < len(ys[q])]
        print("   proved pairs where some yardstick lines are unproven/lost:", len(thin),
              sorted(thin, key=lambda x: x[1]))
        if len(ys) <= 12:
            print("   all yardstick pairs (proving lines / yardstick lines):",
                  sorted((q, len(proved[q]), len(ys[q])) for q in ys))

    p0 = part_fn("plain")
    part_links(p0, "folders")
    pf_, sizes, folded = fold(p0)
    print("   parts (code files):", sizes.most_common())
    if folded:
        print("   folded into one part:", folded)
        part_links(pf_, "<=15 parts")

    # ---- item 2: inferred roots ----
    roots_count = collections.Counter()
    for (f, k), r in base.items():
        s, val, cands, form, target, t = r
        c = sorted(p for p in idx.get(norm(target), []) if not skipped(p))
        if s != "adj" and len(c) == 1 and not (form == "a" and sysname(target)):
            p = c[0]
            roots_count[p[:-len(norm(target))].rstrip("/") or "."] += 1
    print("-- 2 implied roots (unique matches):", roots_count.most_common(12))

    def iroot(c, target):
        return c[:-len(norm(target))].rstrip("/") or "."

    for N in (1, 5, 20):
        g = collections.Counter()
        notes = []
        for (f, k), r in base.items():
            s, val, cands, form, target, t = r
            if s != "unsure":
                continue
            q = [c for c in cands if roots_count[iroot(c, target)] >= N]
            if len(q) != 1:
                g["still unsure"] += 1
                continue
            pick = q[0]
            if ignored(pick):
                g["pick ignored -> none, " + ("right" if t == pick else "wrong")] += 1
            else:
                g["right" if (pick == t and live(t)) else "wrong"] += 1
                if not (pick == t and live(t)):
                    notes.append((f, target, pick, t))
            if (repo == "redis" and target == "sds.h") or (f == "src/kernel/chain.cpp" and target == "chain.h"):
                notes.append(("CHECK", f, target, pick, "-I:", t))
        print(f"   N={N}:", dict(g), "| wrong e.g.", collections.Counter((n[1], n[2], n[3]) for n in notes
                                                                          if n[0] != "CHECK").most_common(4))
        for n in notes:
            if n[0] == "CHECK":
                print("     ", n)
    for (f, k), r in base.items():
        if (repo == "redis" and r[4] == "sds.h") or (f == "src/kernel/chain.cpp" and r[4] == "chain.h"):
            print("   check", f, r[4], r[0], r[2], "| -I:", r[5], "| root counts:",
                  {iroot(c, r[4]): roots_count[iroot(c, r[4])] for c in r[2]})

    # ---- item 3: roots from build files as text ----
    br = build_roots(clone, tracked)
    union = sorted({x for v in br.values() for x in v})
    print("-- 3 build-file roots by folder:", dict(sorted(br.items())))
    u = run(roots_for=lambda f: union, roots_mode="union")
    print("   union", dict(sorted(grade(u).items())), "| wrong:", wrong_list(u).most_common(6),
          "| missed:", missed_list(u), "| unsure:", unsure_list(u))

    def per_dir(f):
        """Roots named by build files in the file's folder and every folder above it."""
        out, d = [], posixpath.dirname(f)
        while True:
            out += [r for r in br.get(d, []) if r not in out]
            if not d:
                return out
            d = posixpath.dirname(d)
    pdr = run(roots_for=per_dir, roots_mode="union")
    print("   per-dir", dict(sorted(grade(pdr).items())), "| wrong:", wrong_list(pdr).most_common(6),
          "| missed:", missed_list(pdr))

    # ---- item 4: declared roots ----
    for roots in DECLARED[repo]:
        d = run(roots_for=lambda f, roots=roots: roots, roots_mode="declared")
        print("-- 4 declared", roots[:3], "...", dict(sorted(grade(d).items())), "| wrong:", wrong_list(d).most_common(6),
              "| missed:", missed_list(d))

    # ---- item 5: test-skip risk ----
    code = [t for t in tracked if posixpath.splitext(t)[1].lower() in G["EXTS"] and not ignored(t)]
    sk = [t for t in code if skipped(t)]
    print("-- 5 skipped code files:", len(sk), collections.Counter(posixpath.dirname(t) for t in sk).most_common(12))
    builds = [t for t in tracked if (posixpath.basename(t) in ("CMakeLists.txt", "Makefile", "Makefile.am", "Makefile.inc")
              or t.endswith((".cmake", ".mk"))) and not ignored(t)]
    hits = []
    skset = set(sk)
    names = collections.defaultdict(list)
    for t in sk:
        names[posixpath.basename(t)].append(t)
    for b in builds:
        if any(part in R3["TEST_DIRS"] for part in b.split("/")[:-1]):
            continue
        txt = open(os.path.join(clone, b), errors="replace").read()
        ctx = ""
        for i, line in enumerate(txt.split("\n"), 1):
            m = re.search(r"(add_executable|add_library|target_sources|\w+_SOURCES|\w+_SRCS|\w+_OBJ|\w+_test|if)\s*[(=]\s*(\S*)", line)
            if m:
                ctx = f"{m.group(1)} {m.group(2)}".strip()
            for n in re.findall(r"[\w./${}()-]+\.(?:c|cc|cpp|cxx|h|hpp|o)\b", line):
                n2 = re.sub(r"^\$[({]\w+[)}]/", "", n)
                stems = [n2] if not n2.endswith(".o") else [n2[:-2] + e for e in (".c", ".cc", ".cpp")]
                for st in stems:
                    full = {join(posixpath.dirname(b), st), norm(st)}
                    if full & skset:
                        hits.append((b, i, n, ctx))
    print("   skipped files named in non-test build files:", len(hits),
          collections.Counter((h[0], h[3]) for h in hits).most_common(20), hits[:8])
    prod = collections.Counter((f, r[5]) for (f, k), r in base.items() if r[5] and skipped(r[5]))
    print("   non-test files including a skipped file (-I):", len(prod),
          collections.Counter(posixpath.dirname(f) + " -> " + t for f, t in prod).most_common(10))
    unitdirs = collections.Counter()
    for t in code:
        for part in t.split("/")[:-1]:
            if part.lower().startswith("unit"):
                unitdirs[(part, "already skipped" if is_test(t.replace(part, "x")) else "only via unit*")] += 1
    print("   path elements starting with 'unit':", dict(unitdirs))


if __name__ == "__main__":
    main()
```
