# Probing the real app in the background

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
