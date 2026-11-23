//! `plotkeeper install` and `uninstall` (#357; #320, "Install and distribution") on a temporary home, with a fake
//! `claude`, a fake `agy` and a fake `codex` on the PATH. The real ones are never on it, so nothing here touches
//! Daniel's setup.

use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::{FileTypeExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

/// A scratch machine: `home`, and `tools` holding a copy of `plotkeeper` with fake hook binaries beside it and,
/// when asked, the fake `claude` and `agy`.
struct Machine {
    _root: tempfile::TempDir,
    home: PathBuf,
    tools: PathBuf,
}

impl Machine {
    fn new(claude: bool, agy: bool) -> Machine {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let tools = root.path().join("tools");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&tools).unwrap();
        fs::copy(env!("CARGO_BIN_EXE_plotkeeper"), tools.join("plotkeeper")).unwrap();
        for name in ["plotkeeper-hook", "plotkeeper-answer"] {
            script(&tools.join(name), &format!("echo {name}"));
        }
        if claude {
            // Logs its arguments and writes nothing, as plotkeeper may not look at what the real one writes.
            script(
                &tools.join("claude"),
                r#"echo "$@" >> "$(dirname "$0")/claude-calls""#,
            );
        }
        if agy {
            // Copies and removes plugin folders by name, like the real one.
            script(
                &tools.join("agy"),
                r#"echo "$@" >> "$(dirname "$0")/agy-calls"
plugins="$HOME/.gemini/config/plugins"
case "$2" in
validate) ;;
uninstall) [ -d "$plugins/$3" ] || exit 1; rm -rf "$plugins/$3" ;;
install) name=$(basename "$3"); mkdir -p "$plugins/$name" && cp -R "$3"/. "$plugins/$name"/ ;;
esac"#,
            );
        }
        Machine {
            _root: root,
            home,
            tools,
        }
    }

    /// Puts a fake `codex` on the PATH, which logs any call: install edits Codex's hook file itself, so it calls none.
    fn with_codex(self) -> Machine {
        script(
            &self.tools.join("codex"),
            r#"echo "$@" >> "$(dirname "$0")/codex-calls""#,
        );
        self
    }

    /// Puts a fake `copilot` on the PATH, which logs its arguments and writes nothing, as plotkeeper may not look at
    /// what the real one writes (#361).
    fn with_copilot(self) -> Machine {
        script(
            &self.tools.join("copilot"),
            r#"echo "$@" >> "$(dirname "$0")/copilot-calls""#,
        );
        self
    }

    fn run(&self, verb: &str) -> Output {
        let mut child = Command::new(self.tools.join("plotkeeper"))
            .arg(verb)
            .env("HOME", &self.home)
            .env("PATH", format!("{}:/usr/bin:/bin", self.tools.display()))
            .env("PLOTKEEPER_NO_BROWSER", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        // A process that opens a FIFO waits for a writer forever: the deadline turns that into a failure.
        let deadline = Instant::now() + Duration::from_secs(30);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() > deadline {
                child.kill().unwrap();
                panic!("plotkeeper {verb} did not finish (did it open a FIFO?)");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "plotkeeper {verb} failed:\n{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        out
    }

    fn calls(&self, tool: &str) -> Vec<String> {
        fs::read_to_string(self.tools.join(format!("{tool}-calls")))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn write(&self, rel: &str, text: &str) {
        let path = self.home.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn data(&self) -> PathBuf {
        self.home.join("Library/Application Support/plotkeeper")
    }

    fn bin(&self) -> String {
        self.data().join("bin").display().to_string()
    }
}

fn script(path: &Path, body: &str) {
    fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

/// Every file and link under `dir`, by its path from `dir`, with its bytes (a link's target).
fn files(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries {
            let path = entry.unwrap().path();
            let rel = path.strip_prefix(root).unwrap().display().to_string();
            let kind = fs::symlink_metadata(&path).unwrap().file_type();
            if kind.is_symlink() {
                let target = fs::read_link(&path).unwrap();
                out.insert(rel, target.display().to_string().into_bytes());
            } else if kind.is_dir() {
                walk(root, &path, out);
            } else {
                out.insert(rel, fs::read(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

/// A `~/.codex/hooks.json` the user wrote: a hook of their own, in their own layout.
const CODEX_OWN: &str = "{\n    \"description\": \"mine\",\n    \"hooks\": { \"Stop\": [ { \"hooks\": [ { \"type\": \"command\", \"command\": \"python3 ~/.codex/hooks/stop.py\", \"timeout\": 30 } ] } ] }\n}\n";

const PLOTKEEPER_LINE: &str = "export CLAUDE_CODE_PLUGIN_DIRS=\"$HOME/Library/Application Support/plotkeeper/checkout/plugin\"";
const FALINKS_LINE: &str = "export CLAUDE_CODE_PLUGIN_DIRS=\"/Users/me/Library/Application Support/dev.falinks.studio/checkout/plugin:/Users/me/own-plugin\"";

/// The legacy setup: the archived app's `~/.zshrc` line with `line`, its agy plugin `studio`, and the plotkeeper
/// plugin it installed for agy with a checkout's hook.
fn legacy(machine: &Machine, line: &str) {
    machine.write(
        ".zshrc",
        &format!(
            "alias g=git\n# The studio plugin's hooks, for every claude session (#21).\n{line}\nexport EDITOR=vim\n"
        ),
    );
    machine.write(
        ".gemini/config/plugins/studio/plugin.json",
        r#"{ "name": "studio" }"#,
    );
    machine.write(
        ".gemini/config/plugins/studio/hooks.json",
        r#"{ "studio": { "Stop": [{ "type": "command", "command": "/old/checkout/target/release/studio-hook" }] } }"#,
    );
    machine.write(
        ".gemini/config/plugins/plotkeeper/hooks.json",
        r#"{ "plotkeeper": { "Stop": [{ "type": "command", "command": "/old/checkout/target/release/plotkeeper-hook --agent agy" }] } }"#,
    );
}

/// Puts a home's files in place.
type Setup = fn(&Machine);

#[test]
fn install_then_uninstall_leaves_every_file_byte_identical() {
    let absent = |_: &Machine| {};
    let present = |m: &Machine| {
        m.write(".zshrc", "alias g=git\n");
        m.write(".codex/hooks.json", "{}\n");
        m.write(".claude/settings.json", "{ \"theme\": \"dark\" }\n");
        m.write(".gemini/config/plugins/other/plugin.json", "{}");
        m.write(
            "Library/Application Support/plotkeeper/events.jsonl",
            "{}\n",
        );
    };
    let customised = |m: &Machine| {
        legacy(m, FALINKS_LINE);
        m.write(".claude/settings.json", "{ \"enabledPlugins\": {} }\n");
        m.write(".gemini/config/plugins/other/plugin.json", "{}");
        m.write(".codex/hooks.json", CODEX_OWN);
    };
    let homes: [(&str, Setup); 3] = [
        ("absent", absent),
        ("present", present),
        ("customised", customised),
    ];
    for (name, setup) in homes {
        for (claude, agy) in [(true, true), (false, false)] {
            let machine = Machine::new(claude, agy);
            // Codex and Copilot go with the others, so Codex's hook file is changed and put back too.
            let machine = if claude {
                machine.with_codex().with_copilot()
            } else {
                machine
            };
            setup(&machine);
            let before = files(&machine.home);
            machine.run("install");
            assert_ne!(
                files(&machine.home),
                before,
                "{name}: install changed nothing"
            );
            machine.run("uninstall");
            assert_eq!(
                files(&machine.home),
                before,
                "{name}, claude {claude}, agy {agy}"
            );
        }
    }
}

#[test]
fn a_harness_not_found_is_skipped_with_one_line() {
    let machine = Machine::new(false, false);
    let out = machine.run("install");
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        text.lines().filter(|l| l.contains("Claude Code")).count(),
        1,
        "{text}"
    );
    assert_eq!(
        text.lines().filter(|l| l.contains("agy")).count(),
        1,
        "{text}"
    );
    assert_eq!(
        text.lines().filter(|l| l.contains("Codex")).count(),
        1,
        "{text}"
    );
    assert_eq!(
        text.lines().filter(|l| l.contains("Copilot")).count(),
        1,
        "{text}"
    );
    assert!(text.contains("not found"), "{text}");
}

/// The commands of a `hooks.json`, with the event each sits under.
fn commands(path: &Path) -> Vec<(String, String)> {
    fn walk(v: &serde_json::Value, event: &str, out: &mut Vec<(String, String)>) {
        match v {
            serde_json::Value::Object(o) => {
                if let Some(c) = o.get("command") {
                    out.push((event.to_owned(), c.as_str().unwrap().to_owned()));
                }
                for (k, v) in o {
                    let event = if k.chars().next().unwrap().is_uppercase() {
                        k
                    } else {
                        event
                    };
                    walk(v, event, out);
                }
            }
            serde_json::Value::Array(a) => a.iter().for_each(|v| walk(v, event, out)),
            _ => {}
        }
    }
    let text = fs::read_to_string(path).unwrap();
    let mut out = Vec::new();
    walk(&serde_json::from_str(&text).unwrap(), "", &mut out);
    out
}

/// The `hooks.json` files of every plugin the agents run after install: Claude Code's rendered in the data folder,
/// which `claude plugin install` takes, and agy's installed copies (agy copies its rendered one). Backups are not among them.
fn installed_hooks(machine: &Machine) -> Vec<PathBuf> {
    let mut found: Vec<_> = files(&machine.home)
        .into_keys()
        .filter(|p| p.ends_with("hooks.json"))
        .filter(|p| {
            p.starts_with("Library/Application Support/plotkeeper/plugins/claude/")
                || p.starts_with(".gemini/config/plugins/")
                || p == ".codex/hooks.json"
        })
        .map(|p| machine.home.join(p))
        .collect();
    found.sort();
    found
}

#[test]
fn with_the_legacy_setup_present_exactly_one_hook_records_each_event() {
    for line in [PLOTKEEPER_LINE, FALINKS_LINE] {
        let machine = Machine::new(true, true);
        legacy(&machine, line);
        machine.run("install");

        let zshrc = fs::read_to_string(machine.home.join(".zshrc")).unwrap();
        assert!(!zshrc.contains("checkout/plugin"), "{zshrc}");
        assert!(!zshrc.contains("for every claude session"), "{zshrc}");
        assert!(zshrc.contains("alias g=git\n") && zshrc.contains("export EDITOR=vim\n"));
        if line == FALINKS_LINE {
            // The entries that are not plotkeeper's stay.
            assert!(
                zshrc.contains("export CLAUDE_CODE_PLUGIN_DIRS=\"/Users/me/own-plugin\""),
                "{zshrc}"
            );
        }
        let plugins = machine.home.join(".gemini/config/plugins");
        assert!(!plugins.join("studio").exists());
        assert_eq!(
            machine
                .calls("claude")
                .iter()
                .filter(|c| c.starts_with("plugin install"))
                .count(),
            1
        );

        // Per agent, each event has one command that records.
        for hooks in installed_hooks(&machine) {
            let recording: Vec<_> = commands(&hooks)
                .into_iter()
                .filter(|(_, c)| c.contains("plotkeeper-hook"))
                .map(|(event, _)| event)
                .collect();
            let mut unique = recording.clone();
            unique.sort();
            unique.dedup();
            assert!(!recording.is_empty(), "{}", hooks.display());
            assert_eq!(recording.len(), unique.len(), "{}", hooks.display());
        }
        // One Claude Code plugin and one agy plugin.
        assert_eq!(
            installed_hooks(&machine).len(),
            2,
            "{:?}",
            installed_hooks(&machine)
        );
    }
}

#[test]
fn after_install_every_hook_command_names_the_data_folders_bin() {
    let machine = Machine::new(true, true).with_codex();
    legacy(&machine, PLOTKEEPER_LINE);
    machine.run("install");
    let hooks = installed_hooks(&machine);
    assert!(
        hooks.iter().any(|p| p.starts_with(machine.data()))
            && hooks
                .iter()
                .any(|p| p.starts_with(machine.home.join(".gemini")))
            && hooks.contains(&machine.home.join(".codex/hooks.json")),
        "{hooks:?}"
    );
    for path in hooks {
        for (_, command) in commands(&path) {
            let quoted_bin = format!("\"{}/plotkeeper-", machine.bin());
            assert!(command.contains(&quoted_bin), "{command}");
            for elsewhere in [
                "target/",
                "checkout",
                "node_modules",
                "CLAUDE_PLUGIN_ROOT",
                "__PLOTKEEPER",
            ] {
                assert!(!command.contains(elsewhere), "{command}");
            }
        }
    }
    for name in ["plotkeeper-hook", "plotkeeper-answer"] {
        assert_eq!(
            fs::read_to_string(machine.data().join("bin").join(name)).unwrap(),
            format!("#!/bin/sh\necho {name}\n")
        );
    }
}

// @law: install-never-opens-claude-settings
#[test]
fn claude_code_goes_through_claude_plugin_and_its_settings_file_is_never_opened() {
    let machine = Machine::new(true, false);
    fs::create_dir_all(machine.home.join(".claude")).unwrap();
    // A FIFO: opening it, to read or to write, waits for the other end, so `run` fails on its deadline.
    let settings = machine.home.join(".claude/settings.json");
    assert!(
        Command::new("mkfifo")
            .arg(&settings)
            .status()
            .unwrap()
            .success()
    );

    machine.run("install");
    let marketplace = machine.data().join("plugins/claude");
    assert_eq!(
        machine.calls("claude"),
        [
            format!("plugin marketplace add {}", marketplace.display()),
            "plugin install plotkeeper@plotkeeper".to_owned(),
        ]
    );
    machine.run("uninstall");
    assert_eq!(
        machine.calls("claude")[2..],
        [
            "plugin uninstall plotkeeper@plotkeeper".to_owned(),
            "plugin marketplace remove plotkeeper".to_owned(),
        ]
    );
    assert!(
        fs::symlink_metadata(&settings)
            .unwrap()
            .file_type()
            .is_fifo()
    );
}

#[test]
fn a_second_install_keeps_the_first_backups() {
    let machine = Machine::new(true, true);
    legacy(&machine, PLOTKEEPER_LINE);
    let before = files(&machine.home);
    machine.run("install");
    machine.run("install");
    machine.run("uninstall");
    assert_eq!(files(&machine.home), before);
}

/// Commands of a hook file, each with the event it sits under.
type Commands = Vec<(String, String)>;

/// plotkeeper's commands in `~/.codex/hooks.json`, by event, and every other command.
fn codex_commands(machine: &Machine) -> (Commands, Commands) {
    let all = commands(&machine.home.join(".codex/hooks.json"));
    all.into_iter()
        .partition(|(_, c)| c.contains("plotkeeper-hook"))
}

#[test]
fn codex_gets_one_recording_hook_per_event_beside_the_users_own_and_the_trust_step() {
    let machine = Machine::new(false, false).with_codex();
    machine.write(".codex/hooks.json", CODEX_OWN);
    let out = machine.run("install");
    machine.run("install");

    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("/hooks"), "{text}");
    let (ours, theirs) = codex_commands(&machine);
    assert_eq!(
        theirs,
        [(
            "Stop".to_owned(),
            "python3 ~/.codex/hooks/stop.py".to_owned()
        )]
    );
    let events: Vec<_> = ours.iter().map(|(e, _)| e.as_str()).collect();
    assert_eq!(
        events,
        [
            "Interrupt",
            "PermissionRequest",
            "PostToolUse",
            "PreToolUse",
            "SessionEnd",
            "SessionStart",
            "Stop",
            "UserPromptSubmit"
        ]
    );
    let command = format!("\"{}/plotkeeper-hook\" --agent codex", machine.bin());
    assert!(ours.iter().all(|(_, c)| *c == command), "{ours:?}");
    // Install edits the file itself: Codex has no command for its hooks.
    assert!(machine.calls("codex").is_empty());
}

#[test]
fn a_codex_hook_file_changed_after_install_loses_only_plotkeepers_hooks_at_uninstall() {
    let machine = Machine::new(false, false).with_codex();
    machine.run("install");
    let path = machine.home.join(".codex/hooks.json");
    let mut file: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    file["hooks"]["Stop"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({ "hooks": [{ "type": "command", "command": "say done" }] }));
    fs::write(&path, file.to_string()).unwrap();

    let out = machine.run("uninstall");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Codex"), "{text}");
    let (ours, theirs) = codex_commands(&machine);
    assert!(ours.is_empty(), "{ours:?}");
    assert_eq!(theirs, [("Stop".to_owned(), "say done".to_owned())]);
}

impl Machine {
    /// Puts a fake Cursor CLI, `agent`, on the PATH, which logs any call: install edits Cursor's hook file itself.
    fn with_cursor(self) -> Machine {
        script(
            &self.tools.join("agent"),
            r#"echo "$@" >> "$(dirname "$0")/agent-calls""#,
        );
        self
    }
}

/// A `~/.cursor/hooks.json` the user wrote: a hook of their own, in their own layout.
const CURSOR_OWN: &str = "{\n  \"version\": 1,\n  \"hooks\": {\n    \"stop\": [ { \"command\": \"./hooks/audit.sh\" } ]\n  }\n}\n";

/// The commands of `~/.cursor/hooks.json`, each with its event, plotkeeper's and the others apart.
fn cursor_commands(machine: &Machine) -> (Commands, Commands) {
    let text = fs::read_to_string(machine.home.join(".cursor/hooks.json")).unwrap();
    let file: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(file["version"], 1, "{text}");
    let mut all = Vec::new();
    for (event, entries) in file["hooks"].as_object().unwrap() {
        for entry in entries.as_array().unwrap() {
            all.push((event.clone(), entry["command"].as_str().unwrap().to_owned()));
        }
    }
    all.into_iter()
        .partition(|(_, c)| c.contains("plotkeeper-hook"))
}

/// Cursor's user-level hook file absent (with and without `~/.cursor`), present and customised (#362).
#[test]
fn cursor_install_then_uninstall_leaves_its_hook_file_byte_identical() {
    let setups: [(&str, Setup); 4] = [
        ("absent", |_| {}),
        ("no file", |m| {
            fs::create_dir_all(m.home.join(".cursor/projects")).unwrap()
        }),
        ("present", |m| {
            m.write(".cursor/hooks.json", "{ \"version\": 1, \"hooks\": {} }\n")
        }),
        ("customised", |m| m.write(".cursor/hooks.json", CURSOR_OWN)),
    ];
    for (name, setup) in setups {
        let machine = Machine::new(false, false).with_cursor();
        setup(&machine);
        let before = files(&machine.home);
        machine.run("install");
        let (ours, _) = cursor_commands(&machine);
        assert_eq!(ours.len(), 7, "{name}");
        machine.run("uninstall");
        assert_eq!(files(&machine.home), before, "{name}");
    }
}

#[test]
fn cursor_gets_one_recording_hook_per_event_beside_the_users_own() {
    let machine = Machine::new(false, false).with_cursor();
    machine.write(".cursor/hooks.json", CURSOR_OWN);
    let out = machine.run("install");
    machine.run("install");

    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Cursor"), "{text}");
    let (ours, theirs) = cursor_commands(&machine);
    assert_eq!(theirs, [("stop".to_owned(), "./hooks/audit.sh".to_owned())]);
    let events: Vec<_> = ours.iter().map(|(e, _)| e.as_str()).collect();
    assert_eq!(
        events,
        [
            "afterAgentResponse",
            "beforeSubmitPrompt",
            "postToolUse",
            "postToolUseFailure",
            "sessionEnd",
            "sessionStart",
            "stop"
        ]
    );
    let command = format!("\"{}/plotkeeper-hook\" --agent cursor", machine.bin());
    assert!(ours.iter().all(|(_, c)| *c == command), "{ours:?}");
    assert!(machine.calls("agent").is_empty());
    // Never a project's hook file.
    assert!(!machine.home.join(".cursor/.cursor").exists());
}

#[test]
fn a_cursor_hook_file_changed_after_install_loses_only_plotkeepers_hooks_at_uninstall() {
    let machine = Machine::new(false, false).with_cursor();
    machine.run("install");
    let path = machine.home.join(".cursor/hooks.json");
    let mut file: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    file["hooks"]["stop"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({ "command": "say done" }));
    fs::write(&path, file.to_string()).unwrap();

    let out = machine.run("uninstall");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Cursor"), "{text}");
    let (ours, theirs) = cursor_commands(&machine);
    assert!(ours.is_empty(), "{ours:?}");
    assert_eq!(theirs, [("stop".to_owned(), "say done".to_owned())]);
}

#[test]
fn cursor_not_found_is_skipped_with_one_line() {
    let machine = Machine::new(false, false);
    let out = machine.run("install");
    let text = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<_> = text.lines().filter(|l| l.contains("Cursor")).collect();
    assert_eq!(lines.len(), 1, "{text}");
    assert!(lines[0].contains("not found"), "{text}");
    assert!(!machine.home.join(".cursor").exists());
}

/// Copilot goes through its own plugin channel (#361; #320, "Install obeys the credentials rule"): install renders
/// `plugin-copilot/` into the data folder and runs `copilot plugin install` on it, uninstall runs `copilot plugin
/// uninstall`, and neither opens a file of Copilot's: each one that can hold its login or settings is a FIFO here.
#[test]
fn copilot_goes_through_copilot_plugin_and_never_opens_its_files() {
    let machine = Machine::new(false, false).with_copilot();
    let copilot = machine.home.join(".copilot");
    fs::create_dir_all(copilot.join("hooks")).unwrap();
    let files = [
        copilot.join("config.json"),
        copilot.join("settings.json"),
        copilot.join("hooks/own.json"),
    ];
    for file in &files {
        assert!(Command::new("mkfifo").arg(file).status().unwrap().success());
    }

    machine.run("install");
    let plugin = machine.data().join("plugins/copilot/plotkeeper");
    assert_eq!(
        machine.calls("copilot"),
        [format!("plugin install {}", plugin.display())]
    );
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(plugin.join("plugin.json")).unwrap()).unwrap();
    assert_eq!(manifest["name"], "plotkeeper");
    let hooks = commands(&plugin.join("hooks.json"));
    assert_eq!(hooks.len(), 7, "{hooks:?}");
    for (_, command) in hooks {
        let ours = format!(
            "\"{}/plotkeeper-hook\" --agent copilot --event ",
            machine.bin()
        );
        assert!(command.starts_with(&ours), "{command}");
    }

    machine.run("uninstall");
    assert_eq!(
        machine.calls("copilot")[1..],
        ["plugin uninstall plotkeeper".to_owned()]
    );
    assert!(!machine.data().join("plugins").exists());
    for file in &files {
        assert!(fs::symlink_metadata(file).unwrap().file_type().is_fifo());
    }
}

#[test]
fn a_second_copilot_install_replaces_its_plugin() {
    let machine = Machine::new(false, false).with_copilot();
    machine.run("install");
    machine.run("install");
    let plugin = machine.data().join("plugins/copilot/plotkeeper");
    let install = format!("plugin install {}", plugin.display());
    assert_eq!(
        machine.calls("copilot"),
        [
            install.clone(),
            "plugin uninstall plotkeeper".to_owned(),
            install
        ]
    );
}
