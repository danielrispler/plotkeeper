//! #49 (#43, decisions 8 and 9): the studio's agy plugin. agy copies a plugin on install, so the app
//! renders it with `studio-hook`'s path, compares it with the installed copy and re-installs. The
//! tests use a scratch `HOME` and a fake `agy` that copies like the real one.

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use studio_workbench::agy_plugin::{Status, install, status};

fn shipped() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../plugin-agy")
}

/// A fake `agy` in `dir`: `validate` fails when `fail-validate` exists beside it; `install` copies
/// the folder to `$HOME/.gemini/config/plugins/studio`; `uninstall` removes it. Each call is logged.
fn fake_agy(dir: &Path) -> PathBuf {
    let agy = dir.join("agy");
    fs::write(
        &agy,
        r#"#!/bin/sh
echo "$@" >> "$(dirname "$0")/calls"
dest="$HOME/.gemini/config/plugins/studio"
case "$2" in
validate) [ ! -e "$(dirname "$0")/fail-validate" ] ;;
uninstall) rm -rf "$dest" ;;
install) mkdir -p "$dest" && cp "$3"/* "$dest"/ ;;
esac
"#,
    )
    .unwrap();
    fs::set_permissions(&agy, fs::Permissions::from_mode(0o755)).unwrap();
    agy
}

fn hook() -> PathBuf {
    PathBuf::from("/checkout with space/target/release/studio-hook")
}

#[test]
fn a_missing_plugin_is_installed_with_the_hooks_path_and_then_is_current() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let agy = fake_agy(tmp.path());
    assert_eq!(status(&shipped(), &hook(), &home).unwrap(), Status::Missing);
    install(&agy, &shipped(), &hook(), &home).unwrap();
    assert_eq!(status(&shipped(), &hook(), &home).unwrap(), Status::Current);
    let copy = fs::read_to_string(home.join(".gemini/config/plugins/studio/hooks.json")).unwrap();
    assert!(copy.contains("/checkout with space/target/release/studio-hook"));
    assert!(!copy.contains("__STUDIO_HOOK__"));
}

#[test]
fn a_copy_made_for_another_hook_path_is_stale_and_install_replaces_it() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let agy = fake_agy(tmp.path());
    install(&agy, &shipped(), Path::new("/old/studio-hook"), &home).unwrap();
    assert_eq!(status(&shipped(), &hook(), &home).unwrap(), Status::Stale);
    install(&agy, &shipped(), &hook(), &home).unwrap();
    assert_eq!(status(&shipped(), &hook(), &home).unwrap(), Status::Current);
}

#[test]
fn a_plugin_that_fails_validation_is_not_installed() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let agy = fake_agy(tmp.path());
    fs::write(tmp.path().join("fail-validate"), "").unwrap();
    let err = install(&agy, &shipped(), &hook(), &home).unwrap_err();
    assert!(err.contains("validat"), "{err}");
    assert_eq!(status(&shipped(), &hook(), &home).unwrap(), Status::Missing);
}

// The wrapper of #43, decision 9, from the shipped `hooks.json`.
fn commands() -> Vec<String> {
    let text = fs::read_to_string(shipped().join("hooks.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    let mut found = Vec::new();
    fn walk(v: &serde_json::Value, out: &mut Vec<String>) {
        match v {
            serde_json::Value::Object(o) => {
                if let Some(c) = o.get("command") {
                    out.push(c.as_str().unwrap().to_owned());
                }
                o.values().for_each(|v| walk(v, out));
            }
            serde_json::Value::Array(a) => a.iter().for_each(|v| walk(v, out)),
            _ => {}
        }
    }
    walk(&json, &mut found);
    found
}

#[test]
fn every_command_is_the_inline_wrapper_for_all_six_events() {
    let text = fs::read_to_string(shipped().join("hooks.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    let events = json["studio"].as_object().unwrap();
    let mut names: Vec<_> = events.keys().map(String::as_str).collect();
    names.sort();
    assert_eq!(
        names,
        [
            "PostInvocation",
            "PostToolUse",
            "PreInvocation",
            "PreToolUse",
            "SessionStart",
            "Stop"
        ]
    );
    for tool in ["PreToolUse", "PostToolUse"] {
        assert_eq!(events[tool][0]["matcher"], "*");
    }
    let commands = commands();
    assert_eq!(commands.len(), 6);
    for c in commands {
        assert_eq!(
            c,
            r#"p=$(cat); (printf %s "$p" | "__STUDIO_HOOK__" >/dev/null 2>&1 &) ; exit 0"#
        );
    }
    assert!(text.matches("\"timeout\"").count() == 6);
}

/// Runs the shipped wrapper, with `studio-hook` at `hook`, under `sh` on `payload`.
fn wrapper(hook: &Path, payload: &str) -> (Duration, std::process::Output) {
    let command = commands()[0].replace("__STUDIO_HOOK__", &hook.display().to_string());
    let start = Instant::now();
    let mut child = Command::new("sh")
        .args(["-c", &command])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    use std::io::Write;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(payload.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    (start.elapsed(), out)
}

fn script(dir: &Path, body: &str) -> PathBuf {
    let path = dir.join("studio-hook");
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[test]
fn the_wrapper_never_blocks_agy() {
    let tmp = tempfile::tempdir().unwrap();
    for (name, hook) in [
        ("missing", tmp.path().join("nope")),
        ("hanging", script(tmp.path(), "sleep 30")),
    ] {
        let (took, out) = wrapper(&hook, "{}");
        assert!(out.status.success(), "{name}");
        assert!(out.stdout.is_empty(), "{name}");
        assert!(took < Duration::from_secs(1), "{name}: {took:?}");
    }
}

#[test]
fn the_wrapper_hands_the_payload_on_intact() {
    let tmp = tempfile::tempdir().unwrap();
    let got = tmp.path().join("got");
    let hook = script(tmp.path(), &format!("cat > '{}'", got.display()));
    let payload = r#"{"hookEventName":"Stop","text":"a \"quoted\" line\nwith $vars and 'quotes'"}"#;
    let (_, out) = wrapper(&hook, payload);
    assert!(out.status.success() && out.stdout.is_empty());
    let deadline = Instant::now() + Duration::from_secs(10);
    while fs::read_to_string(&got).ok().as_deref() != Some(payload) {
        assert!(Instant::now() < deadline, "payload never arrived");
        std::thread::sleep(Duration::from_millis(20));
    }
}
