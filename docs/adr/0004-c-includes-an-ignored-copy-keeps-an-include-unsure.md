# `c-includes`: an ignored copy keeps an include Unsure

`c-includes` resolves an `#include` that is not next to the including file by matching its path against the end of every tracked file's path (`docs/studio/research/c-cpp-extractor.md`, rounds 1-4). A file the map ignores stays a candidate: an include that matches an ignored file and one other file makes the including file Unsure, never a line to the other file, and one that matches only an ignored file gives no line. When no candidate is ignored, a tie is broken by the folders that unique matches imply: the reader picks the one candidate whose implied folder at least 5 unique matches use, and stays Unsure when none or several qualify. An ignored candidate keeps the include Unsure even then (grilling of 2026-11-28, Q5 and Q11).

This departs from `java-imports`, which leaves an ignored file out of its type index, so that an ignored copy neither resolves an import nor makes one Unsure (`docs/studio/behaviour/map.md`, `java-imports`, "`ignored`"; #607). Java's ignored copies are variant trees and fixtures, never the real target. In C a vendored library often *is* the real target, under a header name the repository's own code also uses: redis's `src/redis-cli.c:37` includes `<sds.h>` and, by its own comment, means `deps/hiredis/sds.h`, while `src/sds.h` is also tracked. With `deps/` ignored under Java's rule, the three includes of it become wrong lines to `src/sds.h` (round 3, item 3). `python-imports` already keeps an ignored config as a root for the same reason (#656).

## Considered Options

- **An ignored candidate always keeps the include Unsure** (chosen). Ticking a folder as vendored, as First map offers (spec #753, story 7), can never create a wrong line. Cost on the trial set: 128 of bitcoin's includes stay Unsure, losing no part-to-part link on a proxy map (round 4, item 1; round 4b).
- **Drop ignored files, as `java-imports` does.** Rejected: redis's three `<sds.h>` includes become wrong lines.
- **Ignored files block, but the inferred-folder tie-breaker may still pick a live candidate when the ignored folder has fewer unique matches.** Rejected: measured equal on the trial set, but a small vendored folder exists in practice (redis's `deps/linenoise` has 1 unique match, bitcoin's `src/minisketch/include` 2), and an include of its header that ties with the repository's own file would become a silent wrong line (round 4b, variants A and B).

## Consequences

- `c-includes` and `java-imports` treat `ignored` differently on purpose. Neither is to be changed to match the other.
- A repository whose own headers share names with a vendored library's keeps those includes Unsure. Only a declared include folder could settle them, which `c-includes` does not have.
- Letting the tie-breaker pass an ignored candidate reverses this record and needs a new one.
