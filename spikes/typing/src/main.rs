//! Spike S1: type text into the focused window through the RemoteDesktop
//! portal and libei, on GNOME Wayland.
//!
//!   typing-spike [--delay SECONDS] [--idle SECONDS] [--layout N] [--forget] [TEXT]
//!
//! It waits `--delay` seconds (time to click into the target window), then
//! connects, then types. `--idle` keeps the connection open that long
//! before typing, to see whether GNOME drops an idle connection.
//!
//! It answers four questions (tasks/todo.md, S1):
//!
//! 1. Does typing work in gedit, Firefox and GNOME Terminal?
//! 2. Is the permission dialog skipped on the second run (restore token)?
//! 3. Is it still skipped after a reboot?
//! 4. Does the app learn the active layout (English or Arabic)?
//!
//! The restore token is kept in ~/.config/dictation-spike/portal-token
//! (mode 0600). `--forget` deletes it, to see the dialog again.
//! Run with RUST_LOG=info to see the detected layout group.

use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use eitype::{EiType, EiTypeConfig};

const DEFAULT_TEXT: &str = "Hello, world.";
const DEFAULT_DELAY: u64 = 5;
/// Time to keep the connection open after typing, so GNOME handles the
/// last keys before the session closes.
const HOLD: Duration = Duration::from_secs(1);

struct Args {
    delay: u64,
    idle: u64,
    layout: Option<u32>,
    forget: bool,
    text: String,
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,zbus=warn,tracing=warn")).init();
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(msg) => {
            eprintln!("{msg}");
            eprintln!("usage: typing-spike [--delay SECONDS] [--idle SECONDS] [--layout N] [--forget] [TEXT]");
            std::process::exit(2);
        }
    };
    if let Err(msg) = run(args) {
        eprintln!("FAILED: {msg}");
        std::process::exit(1);
    }
}

fn run(args: Args) -> Result<(), String> {
    let path = token_path().ok_or("no home folder")?;
    if args.forget {
        let _ = fs::remove_file(&path);
        println!("Restore token deleted: the dialog should appear.");
    }
    let saved = fs::read_to_string(&path).ok().map(|t| t.trim().to_string());
    println!(
        "Restore token: {}",
        if saved.is_some() { "found, the dialog should NOT appear" } else { "none, expect the dialog" }
    );

    println!("Click into the target window. Connecting in {} s...", args.delay);
    std::thread::sleep(Duration::from_secs(args.delay));

    let config = EiTypeConfig { layout_index: args.layout, ..EiTypeConfig::from_env() };
    let started = Instant::now();
    let (mut typer, token) = EiType::connect_portal_with_token(config, saved.as_deref())
        .map_err(|e| format!("portal or libei: {e}"))?;
    println!("Connected in {:.1} s.", started.elapsed().as_secs_f32());

    match token {
        Some(token) => {
            save_token(&path, &token).map_err(|e| format!("save token: {e}"))?;
            println!("New restore token saved to {}.", path.display());
        }
        None => println!("The portal returned no restore token: the dialog will come back."),
    }

    if args.idle > 0 {
        println!("Idle for {} s before typing...", args.idle);
        std::thread::sleep(Duration::from_secs(args.idle));
    }
    let started = Instant::now();
    typer.type_text(&args.text).map_err(|e| format!("type: {e}"))?;
    println!(
        "Typed {} characters in {} ms.",
        args.text.chars().count(),
        started.elapsed().as_millis()
    );
    std::thread::sleep(HOLD);
    typer.close();
    Ok(())
}

fn parse_args(mut it: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut args = Args { delay: DEFAULT_DELAY, idle: 0, layout: None, forget: false, text: String::new() };
    let mut words = Vec::new();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--delay" => args.delay = number(it.next(), "--delay")?,
            "--idle" => args.idle = number(it.next(), "--idle")?,
            "--layout" => args.layout = Some(number(it.next(), "--layout")?),
            "--forget" => args.forget = true,
            _ => words.push(arg),
        }
    }
    args.text = if words.is_empty() { DEFAULT_TEXT.to_string() } else { words.join(" ") };
    Ok(args)
}

fn number<T: std::str::FromStr>(value: Option<String>, flag: &str) -> Result<T, String> {
    value.and_then(|v| v.parse().ok()).ok_or(format!("{flag} needs a number"))
}

fn token_path() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(config.join("dictation-spike").join("portal-token"))
}

/// The token lets any program that reads it skip the dialog, so only the
/// user may read it.
fn save_token(path: &PathBuf, token: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut file = fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)?;
    file.write_all(token.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Args, String> {
        parse_args(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn defaults_without_arguments() {
        let args = parse(&[]).unwrap();
        assert_eq!(args.delay, DEFAULT_DELAY);
        assert_eq!(args.idle, 0);
        assert_eq!(args.layout, None);
        assert!(!args.forget);
        assert_eq!(args.text, DEFAULT_TEXT);
    }

    #[test]
    fn flags_and_text() {
        let args = parse(&["--delay", "2", "--idle", "30", "--layout", "1", "--forget", "Dr.", "Smith"]).unwrap();
        assert_eq!(args.delay, 2);
        assert_eq!(args.idle, 30);
        assert_eq!(args.layout, Some(1));
        assert!(args.forget);
        assert_eq!(args.text, "Dr. Smith");
    }

    #[test]
    fn a_flag_without_its_number_is_an_error() {
        assert!(parse(&["--delay"]).is_err());
        assert!(parse(&["--layout", "x"]).is_err());
    }
}
