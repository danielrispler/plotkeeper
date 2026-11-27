//! `plotkeeper install` and `uninstall` (#357; #320, "Install and distribution") on a temporary home, with a fake
//! `claude`, a fake `agy` and a fake `codex` on the PATH. The real ones are never on it, so nothing here touches
//! Daniel's setup.

use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs,
    io::{Read, Write},
    os::unix::fs::{FileTypeExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// The data folder, relative to the home, where `plotkeeper` puts it with no `XDG_DATA_HOME`, which no start here has.
#[cfg(target_os = "macos")]
const DATA: &str = "Library/Application Support/plotkeeper";
#[cfg(target_os = "linux")]
const DATA: &str = ".local/share/plotkeeper";

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

    /// Puts a fake `qwen` on the PATH, which logs its arguments and writes nothing, as plotkeeper may not look at what
    /// the real one writes (#379).
    fn with_qwen(self) -> Machine {
        script(
            &self.tools.join("qwen"),
            r#"echo "$@" >> "$(dirname "$0")/qwen-calls""#,
        );
        self
    }

    /// Puts a fake `opencode` on the PATH, which logs any call: install writes OpenCode's plugin file itself, so it
    /// calls none (#370).
    fn with_opencode(self) -> Machine {
        script(
            &self.tools.join("opencode"),
            r#"echo "$@" >> "$(dirname "$0")/opencode-calls""#,
        );
        self
    }

    /// Puts a fake `cline` on the PATH, which logs its arguments and writes nothing, as plotkeeper may not look at what
    /// the real one writes (#371).
    fn with_cline(self) -> Machine {
        script(
            &self.tools.join("cline"),
            r#"echo "$@" >> "$(dirname "$0")/cline-calls""#,
        );
        self
    }

    fn run(&self, verb: &str) -> Output {
        let out = self.run_any(verb);
        assert!(
            out.status.success(),
            "plotkeeper {verb} failed:\n{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        out
    }

    fn run_args(&self, args: &[&str]) -> Output {
        let out = self.run_with(args);
        assert!(out.status.success(), "plotkeeper {args:?} failed: {out:?}");
        out
    }

    /// Runs `plotkeeper verb`, which may fail.
    fn run_any(&self, verb: &str) -> Output {
        self.run_with(&[verb])
    }

    fn run_with(&self, args: &[&str]) -> Output {
        let verb = args.join(" ");
        let mut child = Command::new(self.tools.join("plotkeeper"))
            .args(args)
            .env("HOME", &self.home)
            .env_remove("XDG_DATA_HOME")
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
        child.wait_with_output().unwrap()
    }

    fn starting(&self, terminal: bool) -> Command {
        let mut command = if terminal {
            in_terminal(&[self.tools.join("plotkeeper").as_os_str()])
        } else {
            Command::new(self.tools.join("plotkeeper"))
        };
        // In the home folder, in no git repository, so the start adds no project and its address carries none (#756).
        command
            .current_dir(&self.home)
            .env("HOME", &self.home)
            .env_remove("XDG_DATA_HOME")
            .env("PATH", format!("{}:/usr/bin:/bin", self.tools.display()))
            .env("PLOTKEEPER_NO_BROWSER", "1")
            .env("PLOTKEEPER_CARD_WRITER", "fake")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    /// A bare `plotkeeper` start in a terminal, given through `/usr/bin/script` (`in_terminal`): each `Wire these? [Y/n]` it
    /// asks is answered with the next of `answers`, and once it prints its address it is stopped with Ctrl+C. Returns
    /// all it printed, with the terminal's line endings made plain.
    fn start(&self, answers: &[&str]) -> String {
        let mut child = self.starting(true).stdin(Stdio::piped()).spawn().unwrap();
        let mut stdin = child.stdin.take().unwrap();
        let mut answered = 0;
        let text = served(&mut child, |text, up| {
            let asked = text.matches("Wire these? [Y/n]").count();
            while answered < asked.min(answers.len()) {
                writeln!(stdin, "{}", answers[answered]).unwrap();
                answered += 1;
            }
            if up {
                // Ctrl+C, through the terminal.
                // The server may have stopped already.
                stdin.write_all(b"\x03").ok();
            }
        });
        text.replace("\r\n", "\n")
    }

    /// A bare `plotkeeper` start with no terminal: its stdin is not one. Stopped with SIGINT once it prints its
    /// address; returns all it printed.
    fn start_without_terminal(&self) -> String {
        let mut child = self.starting(false).stdin(Stdio::null()).spawn().unwrap();
        let pid = child.id().to_string();
        served(&mut child, |_, up| {
            if up {
                // The server may have stopped already.
                Command::new("kill").args(["-INT", &pid]).status().ok();
            }
        })
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
        self.home.join(DATA)
    }

    fn bin(&self) -> String {
        self.data().join("bin").display().to_string()
    }
}

/// Reads what `child` prints, its stdout and stderr, until it exits, calling `each` with all of it so far and whether
/// the server's address is in it, so `each` can answer and, once the server serves, stop it. A start that does not
/// end within 30 seconds fails.
fn served(child: &mut Child, mut each: impl FnMut(&str, bool)) -> String {
    let text = Arc::new(Mutex::new(String::new()));
    let readers: Vec<_> = [
        child
            .stdout
            .take()
            .map(|o| Box::new(o) as Box<dyn Read + Send>),
        child
            .stderr
            .take()
            .map(|e| Box::new(e) as Box<dyn Read + Send>),
    ]
    .into_iter()
    .flatten()
    .map(|mut from| {
        let text = text.clone();
        std::thread::spawn(move || {
            let mut buf = [0; 4096];
            while let Ok(n @ 1..) = from.read(&mut buf) {
                text.lock()
                    .unwrap()
                    .push_str(&String::from_utf8_lossy(&buf[..n]));
            }
        })
    })
    .collect();
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut stopped = false;
    while child.try_wait().unwrap().is_none() {
        let now = text.lock().unwrap().clone();
        let up = now.contains("plotkeeper: http");
        if !stopped {
            each(&now, up);
            stopped = up;
        }
        if Instant::now() > deadline {
            child.kill().unwrap();
            panic!("the start did not end:\n{now}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    for reader in readers {
        reader.join().unwrap();
    }
    text.lock().unwrap().clone()
}

/// `command` run in a terminal of its own by `/usr/bin/script`, which logs to `/dev/null`. macOS's BSD `script`
/// takes the command's words after the log file; Linux's util-linux `script` takes one shell line after `-c`, so
/// there each word is single-quoted.
fn in_terminal(command: &[&OsStr]) -> Command {
    let mut script = Command::new("/usr/bin/script");
    #[cfg(target_os = "macos")]
    script.args(["-q", "/dev/null"]).args(command);
    #[cfg(target_os = "linux")]
    {
        let words: Vec<String> = command
            .iter()
            .map(|word| format!("'{}'", word.to_string_lossy().replace('\'', r"'\''")))
            .collect();
        script.args([
            "-q",
            "-e",
            "-c",
            &format!("exec {}", words.join(" ")),
            "/dev/null",
        ]);
    }
    script
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
        m.write(&format!("{DATA}/events.jsonl"), "{}\n");
    };
    let customised = |m: &Machine| {
        legacy(m, FALINKS_LINE);
        m.write(".claude/settings.json", "{ \"enabledPlugins\": {} }\n");
        m.write(".gemini/config/plugins/other/plugin.json", "{}");
        m.write(".codex/hooks.json", CODEX_OWN);
        m.write(
            ".qwen/settings.json",
            "{ \"env\": { \"DASHSCOPE_API_KEY\": \"sk-planted\" } }\n",
        );
        m.write(
            ".cline/data/settings/providers.json",
            "{ \"anthropic\": { \"apiKey\": \"sk-planted\" } }\n",
        );
    };
    let homes: [(&str, Setup); 3] = [
        ("absent", absent),
        ("present", present),
        ("customised", customised),
    ];
    for (name, setup) in homes {
        for (claude, agy) in [(true, true), (false, false)] {
            let machine = Machine::new(claude, agy);
            // Codex, Copilot, Qwen Code, OpenCode and Cline go with the others, so Codex's hook file and OpenCode's
            // plugin folder are changed and put back too.
            let machine = if claude {
                machine
                    .with_codex()
                    .with_copilot()
                    .with_qwen()
                    .with_opencode()
                    .with_cline()
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
    for label in [
        "Claude Code",
        "agy",
        "Codex",
        "Copilot",
        "Qwen Code",
        "OpenCode",
        "Cline",
    ] {
        let lines = text.lines().filter(|l| l.contains(label)).count();
        assert_eq!(lines, 1, "{label}: {text}");
    }
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
            p.starts_with(&format!("{DATA}/plugins/claude/"))
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

        // The archived app's `~/.zshrc` cleanup is macOS only (#701).
        if cfg!(target_os = "macos") {
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
            fs::create_dir_all(m.home.join(".cursor/projects")).unwrap();
        }),
        ("present", |m| {
            m.write(".cursor/hooks.json", "{ \"version\": 1, \"hooks\": {} }\n");
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

/// A second install whose `qwen extensions install` fails, after it took the first extension out, leaves nothing for
/// uninstall to take out of Qwen Code, so uninstall does not fail on an extension that is gone.
#[test]
fn a_second_qwen_install_that_fails_leaves_uninstall_nothing_to_undo_there() {
    let machine = Machine::new(false, false).with_qwen();
    machine.run("install");
    script(
        &machine.tools.join("qwen"),
        r#"echo "$@" >> "$(dirname "$0")/qwen-calls"; [ "$2" = install ] && exit 1; exit 0"#,
    );
    assert!(!machine.run_any("install").status.success());
    let calls = machine.calls("qwen").len();
    machine.run("uninstall");
    assert_eq!(machine.calls("qwen").len(), calls);
}

/// A second install whose `qwen extensions uninstall` fails for another reason than the extension being gone keeps
/// its record, so uninstall still takes the extension out.
#[test]
fn a_second_qwen_install_whose_uninstall_fails_keeps_the_extension_recorded() {
    let machine = Machine::new(false, false).with_qwen();
    machine.run("install");
    script(
        &machine.tools.join("qwen"),
        r#"echo "$@" >> "$(dirname "$0")/qwen-calls"; echo "Permission denied" >&2; exit 1"#,
    );
    assert!(!machine.run_any("install").status.success());
    assert!(!machine.run_any("uninstall").status.success());
    assert_eq!(
        machine.calls("qwen").last().unwrap(),
        "extensions uninstall plotkeeper"
    );
    assert!(machine.data().join("bin/plotkeeper-hook").exists());
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

/// Qwen Code goes through its own extension channel (#379): its hooks otherwise live in `~/.qwen/settings.json`,
/// which can hold API keys, so install renders `plugin-qwen/` into the data folder and runs `qwen extensions install`
/// on it, uninstall runs `qwen extensions uninstall`, and neither opens a file of Qwen Code's: each one that can hold
/// its keys or settings is a FIFO here.
// @law: install-never-opens-qwen-settings
#[test]
fn qwen_goes_through_qwen_extensions_and_never_opens_its_files() {
    let machine = Machine::new(false, false).with_qwen();
    let qwen = machine.home.join(".qwen");
    fs::create_dir_all(qwen.join("extensions")).unwrap();
    let files = [
        qwen.join("settings.json"),
        qwen.join(".env"),
        qwen.join("extensions/extension-enablement.json"),
    ];
    for file in &files {
        assert!(Command::new("mkfifo").arg(file).status().unwrap().success());
    }

    machine.run("install");
    let extension = machine.data().join("plugins/qwen/plotkeeper");
    assert_eq!(
        machine.calls("qwen"),
        [format!(
            "extensions install {} --consent",
            extension.display()
        )]
    );
    let manifest = extension.join("qwen-extension.json");
    let parsed: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest).unwrap()).unwrap();
    assert_eq!(parsed["name"], "plotkeeper");
    let hooks = commands(&manifest);
    let mut events: Vec<_> = hooks.iter().map(|(e, _)| e.as_str()).collect();
    events.sort();
    assert_eq!(
        events,
        [
            "Notification",
            "PermissionRequest",
            "PostToolUse",
            "PostToolUseFailure",
            "PreToolUse",
            "SessionEnd",
            "SessionStart",
            "Stop",
            "UserPromptSubmit"
        ]
    );
    for (_, command) in hooks {
        assert_eq!(
            command,
            format!("\"{}/plotkeeper-hook\" --agent qwen", machine.bin())
        );
    }

    machine.run("install");
    let install = format!("extensions install {} --consent", extension.display());
    assert_eq!(
        machine.calls("qwen"),
        [
            install.clone(),
            "extensions uninstall plotkeeper".to_owned(),
            install
        ]
    );

    machine.run("uninstall");
    assert_eq!(
        machine.calls("qwen")[3..],
        ["extensions uninstall plotkeeper".to_owned()]
    );
    assert!(!machine.data().join("plugins").exists());
    for file in &files {
        assert!(fs::symlink_metadata(file).unwrap().file_type().is_fifo());
    }
}

/// Cline keeps its API keys in its config folder (#371), so install renders `plugin-cline/` into the data folder and
/// runs `cline plugin install` on it, after `cline plugin uninstall` of an earlier one (Cline refuses to install over
/// one), uninstall runs `cline plugin uninstall`, and neither opens a file of
/// Cline's: its provider settings, which hold its keys, and its global settings are FIFOs here.
// @law: install-never-opens-cline-config
#[test]
fn cline_goes_through_cline_plugin_and_never_opens_its_files() {
    let machine = Machine::new(false, false).with_cline();
    let settings = machine.home.join(".cline/data/settings");
    fs::create_dir_all(&settings).unwrap();
    let files = [
        settings.join("providers.json"),
        settings.join("global-settings.json"),
    ];
    for file in &files {
        assert!(Command::new("mkfifo").arg(file).status().unwrap().success());
    }

    machine.run("install");
    let plugin = machine.data().join("plugins/cline/plotkeeper");
    let (uninstall, install) = (
        "plugin uninstall plotkeeper".to_owned(),
        format!("plugin install {}", plugin.display()),
    );
    assert_eq!(machine.calls("cline"), [uninstall.clone(), install.clone()]);
    let package: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(plugin.join("package.json")).unwrap()).unwrap();
    assert_eq!(package["name"], "plotkeeper");
    assert_eq!(
        package["cline"]["plugins"][0]["paths"][0],
        "./plotkeeper.js"
    );
    let entry = fs::read_to_string(plugin.join("plotkeeper.js")).unwrap();
    assert!(
        entry.contains(&format!(
            "const HOOK = \"{}/plotkeeper-hook\";",
            machine.bin()
        )),
        "{entry}"
    );
    assert!(entry.contains(r#"["--agent", "cline"]"#));

    // A second install replaces the first.
    machine.run("install");
    assert_eq!(
        machine.calls("cline")[2..],
        [uninstall.clone(), install.clone()]
    );

    machine.run("uninstall");
    assert_eq!(machine.calls("cline")[4..], [uninstall]);
    assert!(!machine.data().join("plugins").exists());
    for file in &files {
        assert!(fs::symlink_metadata(file).unwrap().file_type().is_fifo());
    }
}

/// Cline's "No plugin found matching", as a first install's uninstall gets, or an uninstall after the user took
/// plotkeeper's plugin out of Cline themselves, is the plugin gone already, not a failure.
#[test]
fn a_cline_plugin_already_gone_is_uninstalled() {
    let machine = Machine::new(false, false).with_cline();
    script(
        &machine.tools.join("cline"),
        r#"echo "$@" >> "$(dirname "$0")/cline-calls"
[ "$2" = uninstall ] && { echo 'error: No plugin found matching "plotkeeper"' >&2; exit 1; }
exit 0"#,
    );
    machine.run("install");

    let out = machine.run("uninstall");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("Cline: cline plugin uninstall plotkeeper"),
        "{out:?}"
    );
    assert!(!machine.data().join("plugins").exists());
}

/// OpenCode 2 loads every `.js` file in `~/.config/opencode/plugins/`, and has no command for a local plugin file, so
/// install writes plotkeeper's there itself and uninstall removes it, with the folders install made for it (#370).
/// OpenCode's config and login files, which can hold API keys, are FIFOs here: opening one would hang the test.
// @law: install-never-opens-opencode-config
#[test]
fn opencode_gets_its_plugin_file_and_its_other_files_are_never_opened() {
    let machine = Machine::new(false, false).with_opencode();
    let config = machine.home.join(".config/opencode");
    machine.write(".config/opencode/plugins/mine.js", "export default {}\n");
    let secrets = [
        config.join("opencode.json"),
        config.join("opencode.jsonc"),
        machine.home.join(".local/share/opencode/auth.json"),
    ];
    fs::create_dir_all(machine.home.join(".local/share/opencode")).unwrap();
    for file in &secrets {
        assert!(Command::new("mkfifo").arg(file).status().unwrap().success());
    }

    for _ in 0..2 {
        machine.run("install");
        let plugin = fs::read_to_string(config.join("plugins/plotkeeper.js")).unwrap();
        let hook = format!("const HOOK = \"{}/plotkeeper-hook\";", machine.bin());
        assert!(plugin.contains(&hook), "{plugin}");
        assert!(plugin.contains(r#"["--agent", "opencode"]"#), "{plugin}");
        assert!(!plugin.contains("__PLOTKEEPER_BIN__"), "{plugin}");
    }
    assert!(machine.calls("opencode").is_empty());

    machine.run("uninstall");
    assert!(!config.join("plugins/plotkeeper.js").exists());
    assert_eq!(
        fs::read_to_string(config.join("plugins/mine.js")).unwrap(),
        "export default {}\n"
    );
    for file in &secrets {
        assert!(fs::symlink_metadata(file).unwrap().file_type().is_fifo());
    }
}

/// On a home with no `~/.config`, uninstall removes the folders install made for OpenCode's plugin, which a list of
/// files cannot show (#370).
#[test]
fn opencode_install_then_uninstall_leaves_no_folder_it_made() {
    let machine = Machine::new(false, false).with_opencode();
    machine.run("install");
    assert!(
        machine
            .home
            .join(".config/opencode/plugins/plotkeeper.js")
            .is_file()
    );
    machine.run("uninstall");
    assert!(!machine.home.join(".config").exists());

    let machine = Machine::new(false, false).with_opencode();
    machine.write(".config/other/file", "x");
    machine.run("install");
    machine.run("uninstall");
    assert!(machine.home.join(".config/other/file").is_file());
    assert!(!machine.home.join(".config/opencode").exists());
}

/// A `plotkeeper.js` of the user's own in OpenCode's plugins folder is backed up at install and put back at uninstall,
/// and one the user wrote over plotkeeper's after install is left as it is (#370).
#[test]
fn opencode_keeps_a_plotkeeper_js_that_is_not_plotkeepers() {
    let machine = Machine::new(false, false).with_opencode();
    let own = "export default { id: \"mine\", setup() {} }\n";
    machine.write(".config/opencode/plugins/plotkeeper.js", own);
    let before = files(&machine.home);
    machine.run("install");
    machine.run("install");
    let plugin = machine.home.join(".config/opencode/plugins/plotkeeper.js");
    assert!(fs::read_to_string(&plugin).unwrap().contains("--agent"));
    machine.run("uninstall");
    assert_eq!(files(&machine.home), before);

    let machine = Machine::new(false, false).with_opencode();
    machine.run("install");
    machine.write(".config/opencode/plugins/plotkeeper.js", own);
    let out = machine.run("uninstall");
    let plugin = machine.home.join(".config/opencode/plugins/plotkeeper.js");
    assert_eq!(fs::read_to_string(&plugin).unwrap(), own);
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("left as it is"),
        "{out:?}"
    );

    // One written over plotkeeper's before a second install is backed up by it, and put back at uninstall.
    let machine = Machine::new(false, false).with_opencode();
    machine.run("install");
    machine.write(".config/opencode/plugins/plotkeeper.js", own);
    machine.run("install");
    machine.run("uninstall");
    let plugin = machine.home.join(".config/opencode/plugins/plotkeeper.js");
    assert_eq!(fs::read_to_string(&plugin).unwrap(), own);
}

/// Adds the user's own hook `command` under `event` to the hook file at `rel`, as an entry of `entry`'s shape.
fn add_own_hook(machine: &Machine, rel: &str, event: &str, entry: serde_json::Value) {
    let path = machine.home.join(rel);
    let mut file: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    file["hooks"][event].as_array_mut().unwrap().push(entry);
    fs::write(&path, serde_json::to_string_pretty(&file).unwrap()).unwrap();
}

#[test]
fn a_hook_the_user_added_between_two_installs_stays_at_uninstall() {
    let machine = Machine::new(false, false).with_codex().with_cursor();
    machine.write(".codex/hooks.json", CODEX_OWN);
    machine.write(".cursor/hooks.json", CURSOR_OWN);
    machine.run("install");
    add_own_hook(
        &machine,
        ".codex/hooks.json",
        "Stop",
        serde_json::json!({ "hooks": [{ "type": "command", "command": "say done" }] }),
    );
    add_own_hook(
        &machine,
        ".cursor/hooks.json",
        "stop",
        serde_json::json!({ "command": "say done" }),
    );
    machine.run("install");
    machine.run("uninstall");

    let (ours, theirs) = codex_commands(&machine);
    assert!(ours.is_empty(), "{ours:?}");
    assert_eq!(
        theirs,
        [
            (
                "Stop".to_owned(),
                "python3 ~/.codex/hooks/stop.py".to_owned()
            ),
            ("Stop".to_owned(), "say done".to_owned())
        ]
    );
    let (ours, theirs) = cursor_commands(&machine);
    assert!(ours.is_empty(), "{ours:?}");
    assert_eq!(
        theirs,
        [
            ("stop".to_owned(), "./hooks/audit.sh".to_owned()),
            ("stop".to_owned(), "say done".to_owned())
        ]
    );
}

#[test]
fn uninstall_runs_every_step_and_a_rerun_retries_only_the_failed_one() {
    let machine = Machine::new(true, true).with_codex();
    machine.write(".codex/hooks.json", CODEX_OWN);
    let before = files(&machine.home);
    machine.run("install");
    // agy is gone from the PATH at uninstall.
    let agy = machine.tools.join("agy");
    let kept = machine.tools.join("agy-kept");
    fs::rename(&agy, &kept).unwrap();

    let out = machine.run_any("uninstall");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!out.status.success(), "{text}");
    assert!(text.contains("`agy` not found"), "{text}");
    // The steps after agy's ran.
    assert!(
        machine
            .calls("claude")
            .iter()
            .any(|c| c.starts_with("plugin marketplace remove")),
        "{text}"
    );
    assert_eq!(
        fs::read_to_string(machine.home.join(".codex/hooks.json")).unwrap(),
        CODEX_OWN
    );

    fs::rename(&kept, &agy).unwrap();
    let claude_calls = machine.calls("claude").len();
    let out = machine.run("uninstall");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("agy plugin uninstall"), "{text}");
    assert!(!text.contains("Codex"), "{text}");
    assert_eq!(machine.calls("claude").len(), claude_calls, "{text}");
    assert_eq!(files(&machine.home), before);
}

#[test]
fn no_line_of_zshrc_but_the_removed_ones_is_kept_in_the_data_folder() {
    const TOKEN: &str = "export OPENAI_API_KEY=sk-planted-token-0123456789";
    let machine = Machine::new(true, true);
    legacy(&machine, FALINKS_LINE);
    let zshrc = machine.home.join(".zshrc");
    let text = fs::read_to_string(&zshrc).unwrap();
    fs::write(&zshrc, format!("{TOKEN}\n{text}")).unwrap();
    let before = files(&machine.home);
    machine.run("install");
    for (path, bytes) in files(&machine.data()) {
        let text = String::from_utf8_lossy(&bytes);
        for line in [TOKEN, "alias g=git", "export EDITOR=vim"] {
            assert!(!text.contains(line), "{path} holds `{line}`:\n{text}");
        }
    }
    machine.run("uninstall");
    assert_eq!(files(&machine.home), before);
}

/// Off macOS the archived app cannot have left a `~/.zshrc` line: install and uninstall neither read nor rewrite the
/// file, and keep no backup of it (#701).
#[test]
#[cfg(not(target_os = "macos"))]
fn off_macos_install_and_uninstall_leave_zshrc_alone_and_back_nothing_up() {
    let machine = Machine::new(true, true);
    legacy(&machine, FALINKS_LINE);
    let zshrc = machine.home.join(".zshrc");
    let before = fs::read(&zshrc).unwrap();
    machine.run("install");
    assert_eq!(fs::read(&zshrc).unwrap(), before);
    for (path, bytes) in files(&machine.data()) {
        let text = String::from_utf8_lossy(&bytes);
        assert!(!path.contains("zshrc"), "a backup of ~/.zshrc: {path}");
        assert!(
            !text.contains("zshrc_edits\":{"),
            "{path} records zshrc edits:\n{text}"
        );
    }
    machine.run("uninstall");
    assert_eq!(fs::read(&zshrc).unwrap(), before);
}

/// A dotfile manager links `~/.zshrc` to its own copy. Install replaces the copy whole, by a rename, so the link stays
/// a link, and a reader that opened the old file still reads all of it: nothing is ever cut short in place.
#[test]
#[cfg(target_os = "macos")]
fn a_symlinked_zshrc_stays_a_symlink_and_is_replaced_whole() {
    let machine = Machine::new(true, true);
    legacy(&machine, FALINKS_LINE);
    let link = machine.home.join(".zshrc");
    let real = machine.home.join("dotfiles/zshrc");
    fs::create_dir_all(real.parent().unwrap()).unwrap();
    fs::rename(&link, &real).unwrap();
    fs::set_permissions(&real, fs::Permissions::from_mode(0o600)).unwrap();
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let old = fs::read_to_string(&real).unwrap();
    let mut held = fs::File::open(&real).unwrap();

    machine.run("install");

    assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
    assert_eq!(fs::read_link(&link).unwrap(), real);
    let new = fs::read_to_string(&real).unwrap();
    assert!(!new.contains("checkout/plugin"), "{new}");
    assert!(new.contains("alias g=git\n") && new.contains("export EDITOR=vim\n"));
    let mode = fs::metadata(&real).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "the file keeps its permissions");
    let mut still = String::new();
    held.read_to_string(&mut still).unwrap();
    assert_eq!(still, old, "the old file was written over in place");
    let left: Vec<_> = fs::read_dir(real.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(left, ["zshrc"], "a temp file was left behind");

    machine.run("uninstall");
    assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
    assert_eq!(fs::read_to_string(&real).unwrap(), old);
}

/// A record written before install kept only the removed lines held the whole old `~/.zshrc` in `backups/zshrc` and
/// the text install wrote in `state.json`'s `zshrc`.
#[test]
#[cfg(target_os = "macos")]
fn uninstall_from_a_record_with_a_whole_zshrc_backup_puts_it_back_and_deletes_the_backup() {
    let machine = Machine::new(false, false);
    let old = "alias g=git\nexport CLAUDE_CODE_PLUGIN_DIRS=\"/x/plotkeeper/checkout/plugin\"\n";
    let cleaned = "alias g=git\n";
    machine.write(".zshrc", cleaned);
    let record = machine.data().join("install");
    machine.write(
        &format!("{DATA}/install/state.json"),
        &serde_json::json!({
            "claude": false, "agy": false, "zshrc": cleaned, "agy_backups": [], "codex": null, "cursor": null
        })
        .to_string(),
    );
    machine.write(&format!("{DATA}/install/backups/zshrc"), old);
    machine.run("uninstall");
    assert_eq!(
        fs::read_to_string(machine.home.join(".zshrc")).unwrap(),
        old
    );
    assert!(!record.join("backups/zshrc").exists());
}

/// Codex waits for a hook unless it runs in the background (its hook docs, https://learn.chatgpt.com/docs/hooks, read
/// 2026-11-23): the hook never makes the agent wait (the trusted law `hook-never-makes-agent-wait`).
#[test]
fn every_codex_recording_hook_runs_in_the_background() {
    let machine = Machine::new(false, false).with_codex();
    machine.run("install");
    let text = fs::read_to_string(machine.home.join(".codex/hooks.json")).unwrap();
    let file: serde_json::Value = serde_json::from_str(&text).unwrap();
    let handlers: Vec<_> = file["hooks"]
        .as_object()
        .unwrap()
        .values()
        .flat_map(|groups| groups.as_array().unwrap())
        .flat_map(|group| group["hooks"].as_array().unwrap())
        .filter(|h| h["command"].as_str().unwrap().contains("plotkeeper-hook"))
        .collect();
    assert_eq!(handlers.len(), 8, "{text}");
    for handler in handlers {
        assert_eq!(handler["async"], true, "{handler}");
    }
}

/// What a start says when it wires nothing and asks nothing, so a test can tell a start that asked.
const ASK: &str = "Wire these? [Y/n]";

#[test]
fn a_first_start_answered_yes_wires_every_harness_found_and_serves() {
    let machine = Machine::new(true, false).with_codex();
    let text = machine.start(&["y"]);
    assert!(text.contains(ASK), "{text}");
    for line in [
        "Claude Code: claude plugin install plotkeeper@plotkeeper",
        "Codex: add plotkeeper's hooks to",
        "agy: `agy` not found, skipped",
        &machine.bin(),
        "events stay on this machine",
        "plotkeeper uninstall",
    ] {
        assert!(text.contains(line), "{line:?} not in:\n{text}");
    }
    assert!(text.contains("plotkeeper: http"), "{text}");
    assert!(
        machine
            .calls("claude")
            .contains(&"plugin install plotkeeper@plotkeeper".to_owned()),
        "{:?}",
        machine.calls("claude")
    );
    assert!(!codex_commands(&machine).0.is_empty());

    // Both are answered, so the next start asks nothing and runs no harness CLI.
    let calls = machine.calls("claude").len();
    let text = machine.start(&[]);
    assert!(!text.contains(ASK), "{text}");
    assert!(text.contains("plotkeeper: http"), "{text}");
    assert_eq!(machine.calls("claude").len(), calls);
}

#[test]
fn a_yes_says_each_step_once_and_the_question_stands_after_a_blank_line() {
    let machine = Machine::new(true, false).with_codex();
    let text = machine.start(&["y"]);
    assert!(text.contains(&format!("\n\n{ASK}")), "{text}");
    for line in [
        "Claude Code: claude plugin install plotkeeper@plotkeeper",
        "hook binaries:",
        "run `/hooks` in Codex",
    ] {
        assert_eq!(text.matches(line).count(), 1, "{line:?} in:\n{text}");
    }
    assert!(
        text.contains("plotkeeper: wired Claude Code, Codex"),
        "{text}"
    );
}

#[test]
fn a_first_start_answered_no_wires_nothing_serves_and_never_asks_again() {
    let machine = Machine::new(true, false).with_codex();
    let text = machine.start(&["n"]);
    assert!(text.contains(ASK), "{text}");
    assert!(text.contains("plotkeeper: http"), "{text}");
    assert!(
        machine.calls("claude").is_empty(),
        "{:?}",
        machine.calls("claude")
    );
    assert!(!machine.home.join(".codex/hooks.json").exists());

    let text = machine.start(&[]);
    assert!(!text.contains(ASK), "{text}");
    assert!(text.contains("plotkeeper: http"), "{text}");
    assert!(machine.calls("claude").is_empty());
}

#[test]
fn a_harness_found_after_the_first_start_is_asked_about_alone() {
    let machine = Machine::new(true, false);
    machine.start(&["y"]);
    let calls = machine.calls("claude").len();
    let machine = machine.with_codex();

    let text = machine.start(&["y"]);
    assert_eq!(text.matches(ASK).count(), 1, "{text}");
    assert!(text.contains("Codex: add plotkeeper's hooks to"), "{text}");
    assert!(!text.contains("Claude Code: claude plugin"), "{text}");
    assert_eq!(
        machine.calls("claude").len(),
        calls,
        "Claude Code is not wired again"
    );
    assert!(!codex_commands(&machine).0.is_empty());
}

#[test]
fn a_start_with_no_terminal_never_asks_wires_or_stores_an_answer() {
    let machine = Machine::new(true, false).with_codex();
    let text = machine.start_without_terminal();
    assert!(!text.contains(ASK), "{text}");
    assert!(text.contains("plotkeeper install --yes"), "{text}");
    assert!(text.contains("plotkeeper: http"), "{text}");
    assert!(machine.calls("claude").is_empty());
    assert!(!machine.home.join(".codex/hooks.json").exists());

    // No answer was stored, so a start in a terminal asks.
    let text = machine.start(&["n"]);
    assert!(text.contains(ASK), "{text}");
}

#[test]
fn install_yes_wires_without_a_question_and_after_uninstall_a_start_asks_again() {
    let machine = Machine::new(true, false).with_codex();
    let out = machine.run_args(&["install", "--yes"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(!text.contains(ASK), "{text}");
    assert!(
        machine
            .calls("claude")
            .contains(&"plugin install plotkeeper@plotkeeper".to_owned())
    );
    assert!(!machine.start(&[]).contains(ASK));

    machine.run("uninstall");
    let text = machine.start(&["n"]);
    assert!(text.contains(ASK), "{text}");
    assert!(
        text.contains("Claude Code: claude plugin install"),
        "{text}"
    );
    assert!(text.contains("Codex: add plotkeeper's hooks to"), "{text}");
}

#[test]
fn uninstall_says_what_it_keeps_and_how_to_remove_the_rest() {
    let machine = Machine::new(true, false);
    machine.run("install");
    machine.write(&format!("{DATA}/events.jsonl"), "{}\n");
    let out = machine.run("uninstall");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("your recorded history stays in")
            && text.contains(DATA)
            && text.contains("npm uninstall -g plotkeeper"),
        "{text}"
    );

    // With nothing recorded, the data folder goes too, and nothing stays to be said.
    let machine = Machine::new(true, false);
    machine.run("install");
    let out = machine.run("uninstall");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(!text.contains("recorded history"), "{text}");
    assert!(text.contains("npm uninstall -g plotkeeper"), "{text}");
}

#[test]
fn a_declined_harness_is_wired_by_install_and_the_round_trip_stays_byte_identical() {
    let machine = Machine::new(true, false).with_codex();
    machine.write(".codex/hooks.json", CODEX_OWN);
    machine.start(&["n"]);
    let before = files(&machine.home);
    machine.run("install");
    assert!(
        !codex_commands(&machine).0.is_empty(),
        "install wires declined harnesses too"
    );
    machine.run("uninstall");
    let after = files(&machine.home);
    // Outside the data folder, which the start's server writes in, every file is as it was.
    let outside = |files: &BTreeMap<String, Vec<u8>>| {
        files
            .iter()
            .filter(|(p, _)| !p.starts_with(&format!("{DATA}/")))
            .map(|(p, b)| (p.clone(), b.clone()))
            .collect::<BTreeMap<_, _>>()
    };
    assert_eq!(outside(&after), outside(&before));
    // Uninstall forgot the answers too, so a start asks again.
    assert!(machine.start(&["n"]).contains(ASK));
}

#[test]
fn a_second_start_never_asks() {
    let machine = Machine::new(true, false);
    let mut first = machine
        .starting(false)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut line = String::new();
    let mut lines = std::io::BufReader::new(first.stdout.take().unwrap());
    while !line.starts_with("plotkeeper: http") {
        line.clear();
        assert_ne!(
            std::io::BufRead::read_line(&mut lines, &mut line).unwrap(),
            0,
            "the first start printed no address"
        );
    }

    let text = machine.start(&["y"]);
    // The child may have exited already.
    first.kill().ok();
    // Only reaps the child; its exit status is not used.
    first.wait().ok();
    assert!(text.contains(line.trim()), "{text}");
    assert!(!text.contains(ASK), "{text}");
    assert!(machine.calls("claude").is_empty());
}

/// Leaves `rel`, the hook file whose record is `key` in install's `state.json`, as an install from an older template
/// would have: each of plotkeeper's commands with other args, and one more event plotkeeper no longer hooks, `extra`,
/// with the record saying install wrote that text (#435).
fn as_an_older_template_left_it(machine: &Machine, rel: &str, key: &str, extra: &str) {
    fn older(v: &mut serde_json::Value) {
        match v {
            serde_json::Value::Object(o) => {
                if let Some(serde_json::Value::String(c)) = o.get_mut("command")
                    && c.contains("plotkeeper-hook")
                {
                    c.push_str(" --old");
                }
                o.values_mut().for_each(older);
            }
            serde_json::Value::Array(a) => a.iter_mut().for_each(older),
            _ => {}
        }
    }
    let path = machine.home.join(rel);
    let mut file: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    let events = file["hooks"].as_object_mut().unwrap();
    let mut gone = events
        .values()
        .flat_map(|entries| entries.as_array().unwrap())
        .find(|entry| entry.to_string().contains("plotkeeper-hook"))
        .unwrap()
        .clone();
    older(&mut gone);
    for entries in events.values_mut() {
        older(entries);
    }
    events.insert(extra.to_owned(), serde_json::json!([gone]));
    let text = serde_json::to_string_pretty(&file).unwrap() + "\n";
    fs::write(&path, &text).unwrap();
    let state = machine.data().join("install/state.json");
    let mut record: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&state).unwrap()).unwrap();
    record[key]["written"] = serde_json::Value::String(text);
    fs::write(&state, record.to_string()).unwrap();
}

#[test]
fn a_reinstall_after_a_template_change_leaves_only_the_new_entries_beside_the_users_own() {
    let machine = Machine::new(false, false).with_codex().with_cursor();
    machine.write(".codex/hooks.json", CODEX_OWN);
    machine.write(".cursor/hooks.json", CURSOR_OWN);
    machine.run("install");
    let codex_new = codex_commands(&machine);
    let cursor_new = cursor_commands(&machine);
    as_an_older_template_left_it(&machine, ".codex/hooks.json", "codex", "Notification");
    as_an_older_template_left_it(&machine, ".cursor/hooks.json", "cursor", "afterFileEdit");

    machine.run("install");

    assert_eq!(codex_commands(&machine), codex_new);
    assert_eq!(cursor_commands(&machine), cursor_new);
}

#[test]
fn install_then_a_reinstall_after_a_template_change_then_uninstall_leaves_the_hook_files_byte_identical()
 {
    let machine = Machine::new(false, false).with_codex().with_cursor();
    machine.write(".codex/hooks.json", CODEX_OWN);
    machine.write(".cursor/hooks.json", CURSOR_OWN);
    let before = files(&machine.home);
    machine.run("install");
    as_an_older_template_left_it(&machine, ".codex/hooks.json", "codex", "Notification");
    as_an_older_template_left_it(&machine, ".cursor/hooks.json", "cursor", "afterFileEdit");
    machine.run("install");
    machine.run("uninstall");
    assert_eq!(files(&machine.home), before);
}

#[test]
fn a_hook_the_user_added_after_an_older_template_stays_alone_at_uninstall() {
    let machine = Machine::new(false, false).with_codex().with_cursor();
    machine.write(".codex/hooks.json", CODEX_OWN);
    machine.write(".cursor/hooks.json", CURSOR_OWN);
    machine.run("install");
    as_an_older_template_left_it(&machine, ".codex/hooks.json", "codex", "Notification");
    as_an_older_template_left_it(&machine, ".cursor/hooks.json", "cursor", "afterFileEdit");
    add_own_hook(
        &machine,
        ".codex/hooks.json",
        "Stop",
        serde_json::json!({ "hooks": [{ "type": "command", "command": "say done" }] }),
    );
    add_own_hook(
        &machine,
        ".cursor/hooks.json",
        "stop",
        serde_json::json!({ "command": "say done" }),
    );
    machine.run("install");
    machine.run("uninstall");

    let (ours, theirs) = codex_commands(&machine);
    assert!(ours.is_empty(), "{ours:?}");
    assert_eq!(
        theirs,
        [
            (
                "Stop".to_owned(),
                "python3 ~/.codex/hooks/stop.py".to_owned()
            ),
            ("Stop".to_owned(), "say done".to_owned())
        ]
    );
    let (ours, theirs) = cursor_commands(&machine);
    assert!(ours.is_empty(), "{ours:?}");
    assert_eq!(
        theirs,
        [
            ("stop".to_owned(), "./hooks/audit.sh".to_owned()),
            ("stop".to_owned(), "say done".to_owned())
        ]
    );
}

/// Sets the fingerprint install's record holds for `harness` to `value`, or takes it out with `None`, as a record
/// from another build would have it.
fn set_fingerprint(machine: &Machine, harness: &str, value: Option<&str>) {
    let state = machine.data().join("install/state.json");
    let mut record: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&state).unwrap()).unwrap();
    let entry = record["harnesses"][harness].as_object_mut().unwrap();
    match value {
        Some(v) => entry.insert("fingerprint".into(), v.into()),
        None => entry.remove("fingerprint"),
    };
    fs::write(&state, record.to_string()).unwrap();
}

const RETRUST: &str = "run `/hooks` in Codex";

#[test]
fn a_start_rewires_only_the_wired_harnesses_whose_rendered_entries_changed() {
    let machine = Machine::new(true, false).with_codex().with_cursor();
    machine.write(".codex/hooks.json", CODEX_OWN);
    machine.write(".cursor/hooks.json", CURSOR_OWN);
    machine.start(&["y"]);
    let codex_new = codex_commands(&machine);
    let cursor_new = cursor_commands(&machine);
    let claude_calls = machine.calls("claude").len();
    // An older build rendered other entries for Codex and Cursor, and wired them.
    as_an_older_template_left_it(&machine, ".codex/hooks.json", "codex", "Notification");
    as_an_older_template_left_it(&machine, ".cursor/hooks.json", "cursor", "afterFileEdit");
    set_fingerprint(&machine, "codex", Some("older"));
    set_fingerprint(&machine, "cursor", Some("older"));

    let text = machine.start(&[]);

    assert!(!text.contains(ASK), "{text}");
    assert!(text.contains("plotkeeper: rewired Codex"), "{text}");
    assert!(text.contains("plotkeeper: rewired Cursor"), "{text}");
    assert!(text.contains(RETRUST), "{text}");
    assert!(!text.contains("rewired Claude Code"), "{text}");
    assert_eq!(machine.calls("claude").len(), claude_calls);
    assert_eq!(codex_commands(&machine), codex_new);
    assert_eq!(cursor_commands(&machine), cursor_new);

    // Rewired, they hold this build's fingerprints: the next start rewires nothing.
    let text = machine.start(&[]);
    assert!(!text.contains("rewired"), "{text}");
}

const TOOLS: [&str; 5] = ["claude", "agy", "codex", "copilot", "qwen"];

#[test]
fn a_start_with_nothing_changed_runs_no_harness_cli() {
    let machine = Machine::new(true, true)
        .with_codex()
        .with_cursor()
        .with_copilot()
        .with_qwen();
    machine.start(&["y"]);
    let calls: Vec<_> = TOOLS.iter().map(|t| machine.calls(t)).collect();
    assert!(!calls[0].is_empty(), "the first start wired Claude Code");

    let text = machine.start(&[]);

    assert!(!text.contains(ASK), "{text}");
    assert!(!text.contains("rewired"), "{text}");
    assert_eq!(
        TOOLS.iter().map(|t| machine.calls(t)).collect::<Vec<_>>(),
        calls
    );
}

#[test]
fn a_record_without_fingerprints_is_rewired_once_without_asking() {
    let machine = Machine::new(true, false).with_codex();
    machine.start(&["y"]);
    // As a build before #438 left it: answers, no fingerprints.
    set_fingerprint(&machine, "claude", None);
    set_fingerprint(&machine, "codex", None);
    let calls = machine.calls("claude").len();

    let text = machine.start(&[]);
    assert!(!text.contains(ASK), "{text}");
    assert!(text.contains("plotkeeper: rewired Claude Code"), "{text}");
    assert!(text.contains("plotkeeper: rewired Codex"), "{text}");
    assert!(machine.calls("claude").len() > calls);

    let calls = machine.calls("claude").len();
    let text = machine.start(&[]);
    assert!(!text.contains("rewired"), "{text}");
    assert_eq!(machine.calls("claude").len(), calls);
}

#[test]
fn a_declined_harness_whose_entries_changed_is_not_rewired() {
    let machine = Machine::new(true, false).with_codex();
    machine.start(&["n"]);
    set_fingerprint(&machine, "claude", Some("older"));
    set_fingerprint(&machine, "codex", Some("older"));

    let text = machine.start(&[]);

    assert!(!text.contains("rewired"), "{text}");
    assert!(machine.calls("claude").is_empty());
    assert!(!machine.home.join(".codex/hooks.json").exists());
}

#[test]
fn install_then_a_rewire_on_start_then_uninstall_leaves_every_file_outside_the_data_folder_byte_identical()
 {
    let machine = Machine::new(true, false).with_codex().with_cursor();
    machine.write(".codex/hooks.json", CODEX_OWN);
    machine.write(".cursor/hooks.json", CURSOR_OWN);
    let before = files(&machine.home);
    machine.run("install");
    as_an_older_template_left_it(&machine, ".codex/hooks.json", "codex", "Notification");
    as_an_older_template_left_it(&machine, ".cursor/hooks.json", "cursor", "afterFileEdit");
    set_fingerprint(&machine, "codex", Some("older"));
    set_fingerprint(&machine, "cursor", Some("older"));
    assert!(machine.start(&[]).contains("rewired Codex"));
    machine.run("uninstall");
    let outside = |files: BTreeMap<String, Vec<u8>>| {
        files
            .into_iter()
            .filter(|(p, _)| !p.starts_with(&format!("{DATA}/")))
            .collect::<BTreeMap<_, _>>()
    };
    assert_eq!(outside(files(&machine.home)), outside(before));
}

/// Asks the server at `url` (the address a start printed, with its token) whether any harness is wired (#437), as the
/// page does.
fn wired(url: &str) -> bool {
    let rest = url.strip_prefix("http://").unwrap();
    let (host, query) = rest.split_once("/?token=").unwrap();
    let mut stream = std::net::TcpStream::connect(host).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    write!(
        stream,
        "POST /api/wired HTTP/1.1\r\nHost: {host}\r\nCookie: plotkeeper={query}\r\nConnection: close\r\nContent-Length: 2\r\n\r\n{{}}"
    )
    .unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).unwrap();
    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((&raw, ""));
    assert!(head.starts_with("HTTP/1.1 200"), "{raw}");
    serde_json::from_str(body).unwrap_or_else(|e| panic!("{e}: {raw}"))
}

#[test]
fn the_server_says_nothing_is_wired_after_a_declined_start_and_wired_after_install() {
    let machine = Machine::new(true, false).with_codex();
    machine.start(&["n"]);
    let mut server = machine
        .starting(false)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut line = String::new();
    let mut lines = std::io::BufReader::new(server.stdout.take().unwrap());
    while !line.starts_with("plotkeeper: http") {
        line.clear();
        assert_ne!(
            std::io::BufRead::read_line(&mut lines, &mut line).unwrap(),
            0,
            "the start printed no address"
        );
    }
    let url = line.trim().strip_prefix("plotkeeper: ").unwrap().to_owned();
    let before = wired(&url);
    machine.run("install");
    let after = wired(&url);
    // The child may have exited already.
    server.kill().ok();
    // Only reaps the child; its exit status is not used.
    server.wait().ok();
    assert!(!before, "nothing is wired after a no");
    assert!(after, "install wired, and the running server says so");
}

#[test]
fn a_start_after_an_uninstall_that_failed_partway_never_rewires_and_asks_again() {
    let machine = Machine::new(true, false);
    machine.run_args(&["install", "--yes"]);
    // Claude Code's marketplace cannot be removed: the plugin goes, the record keeps the marketplace.
    script(
        &machine.tools.join("claude"),
        r#"echo "$@" >> "$(dirname "$0")/claude-calls"
[ "$2 $3" = "marketplace remove" ] && exit 1
exit 0"#,
    );
    assert!(!machine.run_any("uninstall").status.success());
    let calls = machine.calls("claude").len();

    let text = machine.start_without_terminal();

    assert!(!text.contains("rewired"), "{text}");
    assert_eq!(
        machine.calls("claude")[calls..],
        [] as [String; 0],
        "{text}"
    );
    assert!(text.contains("Claude Code not wired"), "{text}");
}

#[test]
fn a_rewire_that_fails_for_one_harness_still_rewires_the_others_and_asks_about_new_ones() {
    let machine = Machine::new(true, false).with_codex();
    machine.write(".codex/hooks.json", CODEX_OWN);
    machine.start(&["y"]);
    let codex_new = codex_commands(&machine);
    as_an_older_template_left_it(&machine, ".codex/hooks.json", "codex", "Notification");
    set_fingerprint(&machine, "claude", Some("older"));
    set_fingerprint(&machine, "codex", Some("older"));
    // Claude Code's plugin cannot be installed now, and Cursor is new.
    script(
        &machine.tools.join("claude"),
        r#"echo "$@" >> "$(dirname "$0")/claude-calls"
[ "$2" = "install" ] && { echo "no network" >&2; exit 1; }
exit 0"#,
    );
    let machine = machine.with_cursor();

    let text = machine.start_without_terminal();

    assert!(text.contains("Claude Code"), "{text}");
    assert!(text.contains("no network"), "{text}");
    assert!(text.contains("plotkeeper: rewired Codex"), "{text}");
    assert_eq!(codex_commands(&machine), codex_new);
    assert!(text.contains("Cursor not wired"), "{text}");
}

#[test]
fn with_stdout_sent_to_a_file_the_question_still_reaches_the_terminal_and_stdout_holds_the_address()
{
    let machine = Machine::new(true, false);
    let log = machine.home.join("log");
    // As `plotkeeper > log` in a terminal: stdin and stderr are the terminal, stdout the file.
    let plotkeeper = machine.tools.join("plotkeeper");
    let mut command = in_terminal(&[
        "/bin/sh".as_ref(),
        "-c".as_ref(),
        r#"exec "$0" > "$1""#.as_ref(),
        plotkeeper.as_os_str(),
        log.as_os_str(),
    ]);
    for (key, value) in machine.starting(true).get_envs() {
        match value {
            Some(value) => command.env(key, value),
            None => command.env_remove(key),
        };
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let (mut answered, mut stopped) = (false, false);
    let text = served(&mut child, |text, _| {
        if !answered && text.contains(ASK) {
            writeln!(stdin, "n").unwrap();
            answered = true;
        }
        let logged = fs::read_to_string(&log).unwrap_or_default();
        if !stopped && logged.contains("plotkeeper: http") {
            // The server may have stopped already.
            stdin.write_all(b"\x03").ok();
            stopped = true;
        }
    });
    let logged = fs::read_to_string(&log).unwrap();
    assert!(text.contains(ASK), "{text}");
    assert!(text.contains("plotkeeper: not wired"), "{text}");
    assert!(logged.starts_with("plotkeeper: http"), "{logged}");
    assert!(!logged.contains("Claude Code"), "{logged}");
}

#[test]
fn a_start_with_no_terminal_says_what_it_did_not_wire_on_stderr_and_prints_only_the_address_on_stdout()
 {
    let machine = Machine::new(true, false);
    let mut child = machine
        .starting(false)
        .stdin(Stdio::null())
        .spawn()
        .unwrap();
    let mut first = String::new();
    std::io::BufRead::read_line(
        &mut std::io::BufReader::new(child.stdout.take().unwrap()),
        &mut first,
    )
    .unwrap();
    // The child may have exited already.
    child.kill().ok();
    let mut errors = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut errors)
        .unwrap();
    // Only reaps the child; its exit status is not used.
    child.wait().ok();
    assert!(first.starts_with("plotkeeper: http"), "{first}");
    assert!(errors.contains("Claude Code not wired"), "{errors}");
}

/// Leaves install's record as a build before answers (#434) wrote it: its flags, no answers and no marker.
fn as_before_answers(machine: &Machine) {
    let state = machine.data().join("install/state.json");
    let mut record: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&state).unwrap()).unwrap();
    let record_map = record.as_object_mut().unwrap();
    record_map.remove("harnesses");
    record_map.remove("answers_kept");
    fs::write(&state, record.to_string()).unwrap();
}

#[test]
fn a_harness_wired_before_answers_stays_wired_when_a_start_saves_without_it_on_the_path() {
    let machine = Machine::new(true, false).with_codex();
    machine.run("install");
    as_before_answers(&machine);
    let codex = machine.tools.join("codex");
    let kept = machine.tools.join("codex-kept");
    fs::rename(&codex, &kept).unwrap();
    // Claude Code's rewire saves the record while Codex is off the PATH.
    assert!(
        machine
            .start_without_terminal()
            .contains("rewired Claude Code")
    );
    fs::rename(&kept, &codex).unwrap();

    let text = machine.start_without_terminal();

    assert!(!text.contains("not wired"), "{text}");
    assert!(text.contains("plotkeeper: rewired Codex"), "{text}");
}

#[test]
fn a_decline_keeps_a_harness_wired_before_answers_that_is_off_the_path() {
    let machine = Machine::new(false, false).with_codex();
    machine.run("install");
    as_before_answers(&machine);
    let codex = machine.tools.join("codex");
    let kept = machine.tools.join("codex-kept");
    fs::rename(&codex, &kept).unwrap();
    script(
        &machine.tools.join("claude"),
        r#"echo "$@" >> "$(dirname "$0")/claude-calls""#,
    );
    assert!(machine.start(&["n"]).contains(ASK));
    fs::rename(&kept, &codex).unwrap();

    let text = machine.start_without_terminal();

    assert!(!text.contains("not wired"), "{text}");
    assert!(text.contains("plotkeeper: rewired Codex"), "{text}");
}
