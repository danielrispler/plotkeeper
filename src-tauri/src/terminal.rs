//! The shell's terminal daemon client (danielrispler/Falinks#466). The window connects with `terminal_connect`, which
//! starts the daemon from beside the app's own executable when none is running (a Dock launch has
//! no useful `PATH`), then sends the daemon's requests with `terminal_send` and gets every daemon
//! event as a `terminal` event, `{"ev":"lost"}` once the daemon is gone. The protocol is in
//! `terminal-daemon/src/lib.rs`.
//!
//! danielrispler/Falinks#470: a daemon below this app's protocol is still connected to, so its terminals work while the
//! window waits for idle; `terminal_connect` says it is `outdated`, and `terminal_upgrade` stops it
//! once Daniel says yes. When the app starts the daemon itself, after a crash, a reboot or an
//! upgrade, the old daemon's record of its terminals is kept for `terminal_lost`.

use serde_json::{Value, json};
use std::{
    fs::File,
    io::{self, BufRead, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
    thread,
    time::Duration,
};
use studio_terminal_daemon::{Client, PROTOCOL, default_socket};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Default)]
pub struct Daemon {
    /// The connection's writing half, `None` before the first connect and after the daemon is lost.
    writer: Mutex<Option<UnixStream>>,
    /// Whether the daemon is below `PROTOCOL`.
    outdated: Mutex<bool>,
    /// The terminals a dead daemon left in its record.
    lost: Mutex<Vec<Value>>,
}

/// Connects to the daemon, once per app run until it is lost, and returns `{repo, outdated}`: the
/// configured repository, where the pane's shell starts, and whether the daemon is below this
/// app's protocol.
#[tauri::command(async)]
pub fn terminal_connect(app: AppHandle, daemon: State<Daemon>) -> Result<Value, String> {
    let mut writer = daemon.writer.lock().unwrap();
    if writer.is_none() {
        let (mut client, lost) = connect()?;
        // So the daemon deletes the presence file if the app crashes (danielrispler/Falinks#555); an older daemon ignores it.
        client
            .send(&json!({ "op": "app" }))
            .map_err(|e| format!("the terminal daemon: {e}"))?;
        *daemon.outdated.lock().unwrap() = client.protocol < PROTOCOL;
        daemon.lost.lock().unwrap().extend(lost);
        let (reader, stream) = client.split();
        *writer = Some(stream);
        if !POLLING.swap(true, std::sync::atomic::Ordering::AcqRel) {
            let app = app.clone();
            thread::spawn(move || poll(app));
        }
        thread::spawn(move || {
            for line in reader.lines().map_while(Result::ok) {
                if let Ok(event) = serde_json::from_str::<Value>(&line) {
                    // Each `list`'s OSC 7501 status goes to the core (danielrispler/Falinks#560).
                    if event["ev"] == "list"
                        && let Ok(terms) = serde_json::from_value(event["terms"].clone())
                    {
                        crate::listed(&app, terms);
                    }
                    let _ = app.emit("terminal", event);
                }
            }
            app.state::<Daemon>().writer.lock().unwrap().take();
            let _ = app.emit("terminal", json!({ "ev": "lost" }));
        });
    }
    let outdated = *daemon.outdated.lock().unwrap();
    Ok(json!({ "repo": repo(), "outdated": outdated }))
}

/// Whether `poll` runs: one thread per app run, across reconnects.
static POLLING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Asks the daemon for its `list` every second while connected, so the core follows each terminal's
/// OSC 7501 status (danielrispler/Falinks#560); the read loop hands the reply to the core.
// shortcut: polls; have the daemon push status changes if the delay or the traffic matters.
fn poll(app: AppHandle) {
    loop {
        thread::sleep(Duration::from_secs(1));
        if let Some(stream) = app.state::<Daemon>().writer.lock().unwrap().as_mut() {
            let _ = writeln!(stream, "{}", json!({ "op": "list" }));
        }
    }
}

/// The terminals the last daemon ran when it died, each once.
#[tauri::command]
pub fn terminal_lost(daemon: State<Daemon>) -> Vec<Value> {
    std::mem::take(&mut daemon.lost.lock().unwrap())
}

/// Stops the daemon for an upgrade; the read loop then reports it `lost`, and the next connect
/// starts this build's daemon.
#[tauri::command(async)]
pub fn terminal_upgrade() -> Result<(), String> {
    studio_terminal_daemon::stop(&default_socket()).map_err(|e| format!("stop the daemon: {e}"))
}

/// Sends one request, such as `{"op":"input","id":…,"data":…}`, to the daemon.
#[tauri::command]
pub fn terminal_send(msg: Value, daemon: State<Daemon>) -> Result<(), String> {
    let mut writer = daemon.writer.lock().unwrap();
    let stream = writer
        .as_mut()
        .ok_or("not connected to the terminal daemon")?;
    writeln!(stream, "{msg}").map_err(|e| e.to_string())
}

/// The picker's places: the configured repository's main checkout and its tasks, read live from git
/// (danielrispler/Falinks#461, decision 8).
#[tauri::command(async)]
pub fn places() -> Result<studio_workbench::Places, String> {
    studio_workbench::places(Path::new(&repo()))
}

/// The git state of the place at `path` (#26): branch, upstream distance, changed files, distance
/// from origin's default branch, and the operation in progress.
#[tauri::command(async)]
pub fn place_git_state(path: String) -> Result<studio_workbench::GitState, String> {
    studio_workbench::git_state(Path::new(&repo()), &path)
}

/// Fetches the configured repository once (#33); the window reads every place's state again after.
#[tauri::command(async)]
pub fn fetch_repository() -> Result<(), String> {
    studio_workbench::fetch(Path::new(&repo()))
}

/// Creates task `name` in the configured repository (danielrispler/Falinks#468) and returns its place.
#[tauri::command(async)]
pub fn create_task(name: String) -> Result<studio_workbench::Place, String> {
    studio_workbench::create_task(Path::new(&repo()), &name)
}

/// Creates a task named after `request`, an issue, a spec or a prompt, and returns its place.
#[tauri::command(async)]
pub fn create_named_task(request: String) -> Result<studio_workbench::Place, String> {
    studio_workbench::create_named_task(Path::new(&repo()), &request)
}

/// Removes the task's worktree at `path`, or, unless `confirmed`, returns what that would lose.
#[tauri::command(async)]
pub fn remove_task(
    path: String,
    confirmed: bool,
) -> Result<Option<studio_workbench::Losses>, String> {
    studio_workbench::remove_task(Path::new(&repo()), &path, confirmed)
}

/// Deletes branch `branch` of the configured repository when its main checkout holds it.
#[tauri::command(async)]
pub fn delete_merged_branch(branch: String) -> Result<(), String> {
    studio_workbench::delete_merged_branch(Path::new(&repo()), &branch)
}

/// The command a session's terminal runs: `claude` with the studio plugin of the studio's own
/// checkout, whose hooks run its `studio-hook`, or `claude --resume <resume>` for an ended session.
/// From the checkout, not the build folder, so a moved bundle still finds it.
/// `auto` starts it in auto mode (danielrispler/Falinks#541).
#[tauri::command]
pub fn claude_argv(resume: Option<String>, auto: bool) -> Vec<String> {
    let plugin = checkout().unwrap_or_default().join("plugin");
    let presence = crate::presence::path().unwrap_or_default();
    studio_workbench::claude(&plugin, &presence, resume.as_deref(), auto)
}

/// `CLAUDE_CODE_PLUGIN_DIRS`'s entry for the plugin through the data folder's `checkout` link (#21),
/// as the profile writes it.
fn plugin_entry() -> String {
    let data = studio_hook::data_dir(Path::new(""));
    format!("$HOME/{}", data.join("checkout/plugin").display())
}

/// At the window's start (#21): points the data folder's `checkout` at this checkout, then reads
/// `CLAUDE_CODE_PLUGIN_DIRS` as a new interactive login shell sets it. `None` when it loads this
/// checkout's plugin, or when the shell gives no answer within 10 s; else what is wrong, for the
/// bar's "Fix it". An error when the link cannot be made, which "Fix it" cannot mend.
// shortcut: a profile that `exec`s another program, such as tmux, gives no answer, so it counts as
// loaded; ask a non-interactive shell too if that hides a stale path.
#[tauri::command(async)]
pub fn plugin_setup() -> Result<Option<String>, String> {
    let (Some(home), Some(checkout)) = (std::env::home_dir(), checkout()) else {
        return Ok(None);
    };
    studio_workbench::link_checkout(&studio_hook::data_dir(&home), &checkout)
        .map_err(|e| format!("The studio could not link its checkout: {e}."))?;
    let shell = std::env::var("SHELL").unwrap_or("/bin/zsh".into());
    // Markers, as a profile may print its own lines; the app's own value must not count.
    let Ok(mut child) = Command::new(&shell)
        .args([
            "-lic",
            r#"printf '\n<studio>%s</studio>' "$CLAUDE_CODE_PLUGIN_DIRS""#,
        ])
        .env_remove("CLAUDE_CODE_PLUGIN_DIRS")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return Ok(None);
    };
    // A profile that waits on a prompt or the network must not hold the check forever.
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while child.try_wait().is_ok_and(|done| done.is_none()) {
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(None);
        }
        thread::sleep(Duration::from_millis(50));
    }
    let Ok(out) = child.wait_with_output() else {
        return Ok(None);
    };
    let out = String::from_utf8_lossy(&out.stdout);
    let Some(dirs) = out
        .rsplit_once("<studio>")
        .and_then(|(_, rest)| rest.strip_suffix("</studio>"))
    else {
        return Ok(None);
    };
    if studio_workbench::loads_plugin(dirs, &checkout.join("plugin")) {
        return Ok(None);
    }
    Ok(Some(if dirs.is_empty() {
        "Sessions you start yourself get no row: your profile does not load the studio plugin."
            .into()
    } else {
        format!(
            "Sessions you start yourself get no row: CLAUDE_CODE_PLUGIN_DIRS is {dirs}, not the studio plugin."
        )
    }))
}

/// "Fix it" (#21): `~/.zshrc` with its `CLAUDE_CODE_PLUGIN_DIRS` pointed at the `checkout` link. The
/// first fix copies it to `~/.zshrc.studio-backup`, and a later one keeps that copy. Written in
/// place, so a profile that is a link stays one.
// shortcut: always `~/.zshrc`, never `$ZDOTDIR`'s; read the shell's `ZDOTDIR` when a profile moves.
#[tauri::command(async)]
pub fn fix_plugin_setup() -> Result<(), String> {
    let by_hand = || {
        format!(
            "Add this line to your profile: export CLAUDE_CODE_PLUGIN_DIRS=\"{}\"",
            plugin_entry()
        )
    };
    if !std::env::var("SHELL").unwrap_or_default().ends_with("/zsh") {
        return Err(format!("Only zsh's profile is fixed here. {}", by_hand()));
    }
    let home = std::env::home_dir().ok_or("no home folder")?;
    let profile = home.join(".zshrc");
    let old = match std::fs::read_to_string(&profile) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        read => read.map_err(|e| format!("could not read {}: {e}", profile.display()))?,
    };
    let new = studio_workbench::fixed_profile(&old, &plugin_entry(), &home).ok_or_else(|| {
        format!(
            "~/.zshrc builds CLAUDE_CODE_PLUGIN_DIRS with an expansion, so it is left alone. {}",
            by_hand()
        )
    })?;
    let backup = profile.with_file_name(".zshrc.studio-backup");
    if !backup.exists() {
        std::fs::write(&backup, &old).map_err(|e| format!("could not back up the profile: {e}"))?;
    }
    std::fs::write(&profile, new).map_err(|e| format!("could not write {}: {e}", profile.display()))
}

/// Opens an `http(s)` URL Daniel clicked, in a terminal or a rendered reply, in the browser. A file
/// goes through `open_file`.
#[tauri::command]
pub fn open_link(target: String) -> Result<(), String> {
    if !(target.starts_with("https://") || target.starts_with("http://")) {
        return Err(format!("not a web link: {target}"));
    }
    // Never an option: a URL starts with its scheme.
    run(&["/usr/bin/open".into(), target])
}

/// Zed's command-line tool, the default editor when installed.
const ZED: &str = "/Applications/Zed.app/Contents/MacOS/cli";

/// The one opener for files (danielrispler/Falinks#557): opens `path` in Daniel's editor at `line` and `column`. A
/// relative path resolves in `cwd`, the folder of the session that printed it, else the configured
/// repository. `editor` is `settings.json`'s argv template, never a shell string (`editor_argv`). A
/// file that could run when opened is revealed in Finder instead (`reveals`).
#[tauri::command(async)]
pub fn open_file(
    path: String,
    line: u32,
    column: u32,
    cwd: Option<String>,
    editor: Option<Vec<String>>,
) -> Result<(), String> {
    let path = resolve(&path, &cwd.unwrap_or_else(repo));
    if !path.exists() {
        return Err(format!("no such file: {}", path.display()));
    }
    let shown = path.display().to_string();
    let argv = if reveals(&path) {
        vec!["/usr/bin/open".into(), "-R".into(), shown]
    } else {
        editor_argv(
            editor.as_deref(),
            &shown,
            line,
            column,
            Path::new(ZED).exists(),
        )
    };
    run(&argv)
}

/// `path` from the folder `cwd`: `~/` from the home folder, a relative path inside `cwd`.
fn resolve(path: &str, cwd: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => std::env::home_dir().unwrap_or_default().join(rest),
        None => Path::new(cwd).join(path),
    }
}

/// The editor's argv for `path`, absolute, at `line` and `column`: `template`'s arguments with
/// `{path}`, `{line}` and `{column}` filled in, so the path stays one argument whatever it holds.
/// With no template, Zed's CLI when `zed` is installed, else `open`, which loses the line.
fn editor_argv(
    template: Option<&[String]>,
    path: &str,
    line: u32,
    column: u32,
    zed: bool,
) -> Vec<String> {
    let default = if zed {
        [ZED, "{path}:{line}:{column}"]
    } else {
        ["/usr/bin/open", "{path}"]
    };
    let template = template.map_or_else(|| default.map(String::from).to_vec(), <[_]>::to_vec);
    // The path last, so a `{line}` in it stays as written.
    template
        .iter()
        .map(|arg| {
            arg.replace("{line}", &line.to_string())
                .replace("{column}", &column.to_string())
                .replace("{path}", path)
        })
        .collect()
}

/// The argv that runs `argv`: as is when its program is a path, else under a login shell, so a bare
/// name (`code`) is found on the profile's `PATH` as the daemon finds `claude`; a Finder launch has
/// a bare one. The arguments reach the program as `"$@"`, never parsed by the shell.
fn launcher(argv: &[String]) -> Vec<String> {
    if argv.first().is_none_or(|program| program.contains('/')) {
        return argv.to_vec();
    }
    let shell = std::env::var("SHELL").unwrap_or("/bin/zsh".into());
    [
        shell,
        "-l".into(),
        "-c".into(),
        "exec \"$@\"".into(),
        "studio".into(),
    ]
    .into_iter()
    .chain(argv.iter().cloned())
    .collect()
}

/// Runs `argv` through `launcher` and waits for it.
fn run(argv: &[String]) -> Result<(), String> {
    let argv = launcher(argv);
    let (program, args) = argv.split_first().ok_or("the editor command is empty")?;
    match Command::new(program).args(args).status() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!("{} exited with {status}", argv.join(" "))),
        Err(e) => Err(format!("could not run {program}: {e}")),
    }
}

/// Whether opening `path` could run it, so a click must only reveal it: an executable file, a
/// script or app kind that `open` runs, or a directory bundle (a directory with an extension, such
/// as `Tool.app`).
fn reveals(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    const RUNS: &[&str] = &[
        "app", "command", "sh", "tool", "terminal", "fileloc", "jar", "pkg", "mpkg",
    ];
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase());
    let Ok(meta) = path.metadata() else {
        return true;
    };
    meta.is_dir() && ext.is_some()
        || meta.is_file() && meta.permissions().mode() & 0o111 != 0
        || ext.is_some_and(|e| RUNS.contains(&e.as_str()))
}

/// Connects to a daemon of any protocol, starting one first when nothing listens on its socket;
/// then also returns the record of the terminals the dead one ran.
fn connect() -> Result<(Client, Vec<Value>), String> {
    let socket = default_socket();
    match Client::connect(&socket, 0) {
        Ok(client) => return Ok((client, Vec::new())),
        Err(e)
            if !matches!(
                e.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
            ) =>
        {
            return Err(format!("the terminal daemon: {e}"));
        }
        Err(_) => {}
    }
    // Read before the new daemon's first spawn rewrites it.
    let record = std::fs::read_to_string(studio_terminal_daemon::record(&socket));
    let lost = record
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok());
    let log = socket.with_extension("log");
    start(&log).map_err(|e| format!("start the terminal daemon: {e}"))?;
    for _ in 0..50 {
        thread::sleep(Duration::from_millis(100));
        if let Ok(client) = Client::connect(&socket, 0) {
            return Ok((client, lost.unwrap_or_default()));
        }
    }
    Err(format!(
        "the terminal daemon did not start; see {}",
        log.display()
    ))
}

/// Starts `studio-terminal-daemon serve` from beside this executable, logging to `log`. The daemon
/// calls `setsid` itself, so it outlives the app.
fn start(log: &Path) -> io::Result<()> {
    let daemon = std::env::current_exe()?.with_file_name("studio-terminal-daemon");
    if let Some(dir) = log.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let log = File::options().create(true).append(true).open(log)?;
    let mut child = Command::new(&daemon)
        .arg("serve")
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log)
        .spawn()
        .map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", daemon.display())))?;
    // Reaps it if it ever exits while the app runs.
    thread::spawn(move || child.wait());
    Ok(())
}

/// The configured repository: `settings.json`'s `repository` (danielrispler/Falinks#605), else the studio's own
/// checkout (danielrispler/Falinks#461, decision 3), else the home folder.
pub(crate) fn repo() -> String {
    crate::settings::repository()
        .map(PathBuf::from)
        .or_else(checkout)
        .or_else(std::env::home_dir)
        .unwrap_or_default()
        .display()
        .to_string()
}

/// The main checkout of the repository the app was built in, so a build in a worktree still finds
/// the main checkout's plugin and release hooks.
fn checkout() -> Option<PathBuf> {
    let common = Command::new("git")
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output();
    match common {
        Ok(out) if out.status.success() => {
            let dir = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
            dir.parent().map(Path::to_path_buf)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::PermissionsExt};

    #[test]
    fn a_file_that_could_run_is_revealed_and_any_other_opened() {
        let dir = std::env::temp_dir().join(format!("studio-reveal-{}", std::process::id()));
        fs::create_dir_all(dir.join("Tool.app")).unwrap();
        fs::create_dir_all(dir.join("plain")).unwrap();
        for name in ["notes.md", "run", "go.command", "setup.SH"] {
            fs::write(dir.join(name), "").unwrap();
        }
        fs::set_permissions(dir.join("run"), fs::Permissions::from_mode(0o755)).unwrap();
        let reveals = |name: &str| super::reveals(&dir.join(name));
        let revealed: Vec<&str> = [
            "Tool.app",
            "plain",
            "notes.md",
            "run",
            "go.command",
            "setup.SH",
        ]
        .into_iter()
        .filter(|name| reveals(name))
        .collect();
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(revealed, ["Tool.app", "run", "go.command", "setup.SH"]);
    }

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_editor_argv_keeps_the_path_one_argument() {
        let path = "/r/my dir/a;rm -rf ~.rs";
        let zed = super::editor_argv(None, path, 12, 3, true);
        assert_eq!(
            zed,
            [super::ZED, "/r/my dir/a;rm -rf ~.rs:12:3"].map(String::from)
        );
        let code = strings(&["code", "-g", "{path}:{line}:{column}"]);
        assert_eq!(
            super::editor_argv(Some(&code), path, 5, 1, true),
            strings(&["code", "-g", "/r/my dir/a;rm -rf ~.rs:5:1"])
        );
        // A bare program is found through the login shell's `PATH`, with each argument its own.
        let launched = super::launcher(&strings(&["code", "-g", path]));
        assert_eq!(
            &launched[1..],
            strings(&["-l", "-c", "exec \"$@\"", "studio", "code", "-g", path])
        );
    }

    #[test]
    fn with_no_zed_and_no_editor_set_the_fallback_is_open() {
        assert_eq!(
            super::editor_argv(None, "/r/a.rs", 4, 1, false),
            strings(&["/usr/bin/open", "/r/a.rs"])
        );
        assert_eq!(
            super::launcher(&strings(&["/usr/bin/open", "/r/a.rs"])),
            strings(&["/usr/bin/open", "/r/a.rs"])
        );
    }

    #[test]
    fn a_relative_path_resolves_in_the_folder_of_the_session_that_printed_it() {
        let worktree = "/r/Falinks/.claude/worktrees/fix-x";
        assert_eq!(
            super::resolve("src/foo.rs", worktree),
            std::path::Path::new("/r/Falinks/.claude/worktrees/fix-x/src/foo.rs")
        );
        assert_eq!(
            super::resolve("/abs/a.rs", worktree),
            std::path::Path::new("/abs/a.rs")
        );
        assert!(super::resolve("~/notes.md", worktree).ends_with("notes.md"));
        assert!(!super::resolve("~/notes.md", worktree).starts_with(worktree));
    }
}
