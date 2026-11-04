// studio-terminal-daemon serve [--socket PATH]
// studio-terminal-daemon attach <terminal> [--socket PATH]
use std::path::PathBuf;

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let socket = match args.iter().position(|a| a == "--socket") {
        Some(at) if at + 1 < args.len() => PathBuf::from(args.drain(at..at + 2).nth(1).unwrap()),
        _ => studio_terminal_daemon::default_socket(),
    };
    let done = match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["serve"] => studio_terminal_daemon::daemon::serve(&socket),
        ["attach", id] => studio_terminal_daemon::attach::run(&socket, id),
        _ => Err("usage: studio-terminal-daemon serve | attach <terminal>  [--socket PATH]".into()),
    };
    if let Err(e) = done {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
