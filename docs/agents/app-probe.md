# Probing the real app in the background

> **Archived (danielrispler/plotkeeper#332).** This probe drives the Tauri desktop app (`archive/src-tauri/`), which is archived, out of the build and unmaintained. plotkeeper is a local web page now: prove it with the snap fixtures (`npm run snap`) and a real browser. The steps below are kept for reference only.

A **probe** is a throwaway debug build of Studio that runs itself through one scenario and logs what happened. It runs beside Daniel's open Studio, touches none of his data, and never takes his focus. Use it when the evidence you need sits below the page, in AppKit, tao or wry, such as which view is first responder (#23).

## Steps

1. **Tag the probe code** with a unique prefix, such as `// [DEBUG-k23]`, so that one `grep` finds it all at cleanup. Gate it behind an env var read in `.setup`:

   ```rust
   if std::env::var_os("DEBUG_K23").is_some() {
       app.set_activation_policy(tauri::ActivationPolicy::Prohibited); // the app can never activate
       debug_k23(app.handle().clone());
   }
   ```

   In `debug_k23`, first set the window's alpha to 0 on the main thread (`app.run_on_main_thread`). Then, from a thread, wait about 10 s, run the scenario's steps on the main thread, and `eprintln!` tagged lines with the state you assert on. For AppKit types, add `objc2`, `objc2-app-kit` and `objc2-foundation` to `src-tauri/Cargo.toml` (they are in the lockfile already), and remove them at cleanup.
2. **Build it with its own identifier and target dir.** The identifier keeps single-instance from handing off to Daniel's Studio. Delete the old binary first, so a failed build cannot launch it:

   ```sh
   SP=<scratchpad>; export CARGO_TARGET_DIR=$SP/target
   npm run build && rm -f $SP/target/debug/studio
   TAURI_CONFIG='{"identifier":"dev.falinks.studio.probe"}' cargo build -p studio -p studio-terminal-daemon
   ```
3. **Run it with `HOME` in the scratchpad**, which gives it its own data directory and daemon. Launch only if the binary exists, and kill it and its daemon after:

   ```sh
   [ -x $SP/target/debug/studio ] || exit 1
   HOME=$SP/home DEBUG_K23=<steps> $SP/target/debug/studio > $SP/probe.log 2>&1 & PID=$!
   sleep 26; kill $PID; pkill -f "$SP/target/debug/studio-terminal-daemon"
   grep 'DEBUG-k23' $SP/probe.log
   ```
4. **Clean up:** `grep -rn 'DEBUG-k23'` finds nothing, and `Cargo.lock` holds only the dependencies the fix keeps.

The probe is done when the same command goes red on the bug and green on the fix, over repeated runs, with every logged line showing `isActive` false.

## Limits

- **An inactive app gets no keys.** WebKit drops synthetic `NSEvent`s sent to an app that is not active, so assert on the state that decides delivery (first responder, a command reaching Rust), not on `keydown`. A probe that needs real key delivery must activate the app, which takes Daniel's focus: ask him before running it.
- **Keys from outside are blocked.** `CGEvent.postToPid` from another process needs Accessibility permission, which the terminal lacks.
- **Full screen switches Daniel's Space** even when the window is invisible. Reproduce the call that full screen makes instead. #23 queued tao's `makeFirstResponder` and then resized the window.

## Screenshots without the app

To show how the UI draws a given state, such as a ticket's screenshot criterion, use `npm run snap` rather than a probe. It renders `dist/` in headless Chrome against a stub Tauri API fed by a fixture, so it needs no Rust build, no running Studio and takes no focus. It shows what the page draws from state, not native behaviour: no terminal, no AppKit, no keys. For those, use a probe.

```sh
npm run build                                  # snap renders dist/, so build first
npm run snap -- mapBase out/map.png            # the page's base: the map beside the list
npm run snap -- list out/list.png              # the list with a nested, a selected and a landed row
npm run snap:rail -- out/rail                  # every rail fixture (running, your turn, needs you, idle) at 1728, 1100 and 864 px, the overlay open too
```

`SNAP_SIZE=1100x800` sets the window's size for a `snap` run and `SNAP_BODY=rail-open` puts classes on `<body>`, such as the rail's overlay open; with neither, the render is the 1900x900 default. The rail fixtures are in `scripts/snap/rail-fixtures.ts`.

A fixture can carry a `snapScript`: a string run in the page once the UI has drawn, as the body of an async function that gets `log`. It can click, scroll and read the page. `snap` prints what it logs and exits non-zero when a logged line starts `snap:fail` (a script that throws logs one) or the page logs an uncaught error or unhandled rejection. `railIdleScroll` and `railIdleFail` are the samples: the first scrolls the story and logs its `scrollTop`, the second fails on purpose. `element.click()` skips the pointer events a mouse sends, so a press that pointer capture could take from its target needs a real click: a fixture with `realPointer: true` gets `await __snapClick(node)`, which has snap send Chrome a real mouse click at the node's middle over Chrome's DevTools port (`mapBadgeList`). These runs need Chrome, so they stay out of `npm run check`.

The fixtures are in `scripts/snap/fixtures.ts`: a fixture maps each Tauri command to its reply, typed by `Commands` in `ui/types.ts`, so add one there for a new state; `scripts/snap/base.ts` holds the replies every fixture starts from. `npm run snap:all` renders them all, as CI does. The stub is injected into a temp copy of `dist/`, never the shipped app. `npm run check` does not run Chrome; set `CHROME` to use another binary than the default macOS Chrome. Look at the PNG yourself, to check that it shows what the fixture claims.
