//! Spike S3: register the app's three shortcuts through the GlobalShortcuts
//! portal on GNOME Wayland, and print each press.
//!
//!   hotkeys-spike [--app-id ID] [--seconds N]
//!
//! It asks for Alt+M (start/stop), Ctrl+Alt+Z (undo) and Ctrl+Alt+Q (quit),
//! prints what GNOME granted, then prints every press and release for
//! `--seconds` (default 60), or until the quit shortcut.
//!
//! `--app-id` registers the process under that app ID first
//! (org.freedesktop.host.portal.Registry). Portals keep permissions per app
//! ID, and an unsandboxed process started from a terminal has none.
//!
//! Questions (tasks/todo.md, S3): what GNOME 51 shows the first time;
//! whether the second run asks again; whether Alt+M is accepted.

use std::time::{Duration, Instant};

use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
use futures_util::StreamExt;

const DEFAULT_SECONDS: u64 = 60;

/// (id, description, preferred trigger). Trigger strings follow the XDG
/// shortcuts specification.
const SHORTCUTS: [(&str, &str, &str); 3] = [
    ("toggle", "Start or stop dictation", "ALT+m"),
    ("undo", "Delete the last dictation", "CTRL+ALT+z"),
    ("quit", "Quit dictation", "CTRL+ALT+q"),
];

#[derive(Debug, PartialEq)]
struct Args {
    app_id: Option<String>,
    seconds: u64,
}

#[tokio::main]
async fn main() {
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(msg) => {
            eprintln!("{msg}");
            eprintln!("usage: hotkeys-spike [--app-id ID] [--seconds N]");
            std::process::exit(2);
        }
    };
    if let Err(msg) = run(args).await {
        eprintln!("FAILED: {msg}");
        std::process::exit(1);
    }
}

async fn run(args: Args) -> Result<(), String> {
    if let Some(id) = &args.app_id {
        let app_id = id.parse().map_err(|e| format!("app ID {id}: {e}"))?;
        ashpd::register_host_app(app_id).await.map_err(err("register app ID"))?;
        println!("Registered as {id}.");
    }

    let portal = GlobalShortcuts::new().await.map_err(err("GlobalShortcuts portal"))?;
    let session = portal.create_session().await.map_err(err("CreateSession"))?;

    let wanted: Vec<NewShortcut> = SHORTCUTS
        .iter()
        .map(|(id, description, trigger)| NewShortcut::new(*id, *description).preferred_trigger(*trigger))
        .collect();
    let started = Instant::now();
    let bound = portal
        .bind_shortcuts(&session, &wanted, None)
        .await
        .map_err(err("BindShortcuts"))?
        .response()
        .map_err(err("BindShortcuts response (dialog cancelled?)"))?;
    println!("Bound in {:.1} s:", started.elapsed().as_secs_f32());
    for shortcut in bound.shortcuts() {
        println!("  {:<7} {:<30} -> {:?}", shortcut.id(), shortcut.description(), shortcut.trigger_description());
    }
    if bound.shortcuts().is_empty() {
        println!("  (none granted)");
    }

    let mut activated = Box::pin(portal.receive_activated().await.map_err(err("Activated"))?);
    let mut deactivated = Box::pin(portal.receive_deactivated().await.map_err(err("Deactivated"))?);
    println!("Press the shortcuts. Listening for {} s (or until quit)...", args.seconds);
    let deadline = tokio::time::sleep(Duration::from_secs(args.seconds));
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            Some(event) = activated.next() => {
                println!("{:>6.1} s  pressed   {}", started.elapsed().as_secs_f32(), event.shortcut_id());
                if event.shortcut_id() == "quit" {
                    println!("Quit shortcut: stopping.");
                    return Ok(());
                }
            }
            Some(event) = deactivated.next() => {
                println!("{:>6.1} s  released  {}", started.elapsed().as_secs_f32(), event.shortcut_id());
            }
            _ = &mut deadline => {
                println!("Time is up.");
                return Ok(());
            }
        }
    }
}

fn parse_args(mut it: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut args = Args { app_id: None, seconds: DEFAULT_SECONDS };
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--app-id" => args.app_id = Some(it.next().ok_or("--app-id needs an ID")?),
            "--seconds" => {
                args.seconds = it.next().and_then(|v| v.parse().ok()).ok_or("--seconds needs a number")?
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(args)
}

fn err<E: std::fmt::Display>(what: &'static str) -> impl Fn(E) -> String {
    move |e| format!("{what}: {e}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Args, String> {
        parse_args(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn defaults() {
        assert_eq!(parse(&[]).unwrap(), Args { app_id: None, seconds: DEFAULT_SECONDS });
    }

    #[test]
    fn app_id_and_seconds() {
        let args = parse(&["--app-id", "io.github.rawasmhd.DeepgramDictation", "--seconds", "30"]).unwrap();
        assert_eq!(args.app_id.as_deref(), Some("io.github.rawasmhd.DeepgramDictation"));
        assert_eq!(args.seconds, 30);
    }

    #[test]
    fn bad_arguments() {
        assert!(parse(&["--app-id"]).is_err());
        assert!(parse(&["--seconds", "x"]).is_err());
        assert!(parse(&["--verbose"]).is_err());
    }

    #[test]
    fn the_three_shortcuts_have_unique_ids() {
        let mut ids: Vec<_> = SHORTCUTS.iter().map(|s| s.0).collect();
        ids.dedup();
        assert_eq!(ids, ["toggle", "undo", "quit"]);
    }
}
