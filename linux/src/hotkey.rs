//! Alt+M, Ctrl+Alt+Z and Ctrl+Alt+Q, through the GlobalShortcuts portal (S3).
//!
//! GNOME asks once ("allow ... to set shortcuts globally") and keeps the
//! permission under the app ID, so a restart binds without the dialog.
//! The user can change the keys in GNOME Settings. Each press becomes the
//! same command that `dictation --toggle` sends.

use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
use ashpd::zbus::export::futures_core::Stream;
use tokio::runtime::Runtime;

use dictation_core::log;

use crate::control::Command;

/// (id, description, preferred trigger). The triggers follow the XDG
/// shortcuts specification.
const SHORTCUTS: [(&str, &str, &str); 3] = [
    ("toggle", "Start or stop dictation", "ALT+m"),
    ("undo", "Delete the last dictation", "CTRL+ALT+z"),
    ("quit", "Quit dictation", "CTRL+ALT+q"),
];

/// A bind that takes this long showed GNOME's dialog. Without the dialog
/// it takes about 0.3 s (S3).
const DIALOG_SHOWN: Duration = Duration::from_secs(2);

/// The command for a shortcut ID from the portal.
fn command(id: &str) -> Option<Command> {
    match id {
        "toggle" => Some(Command::Toggle),
        "undo" => Some(Command::Undo),
        "quit" => Some(Command::Quit),
        _ => None,
    }
}

/// Right after the first dialog, no press arrived in S3, so bind again
/// once: the second bind needs no dialog.
fn rebind(took: Duration) -> bool {
    took >= DIALOG_SHOWN
}

/// Bind the shortcuts and send each press as an event, on the portal
/// runtime, for the life of the app. If the portal is missing or the user
/// says no, log it: `dictation --toggle` still works.
pub fn listen<E: Send + 'static>(runtime: &Runtime, events: Sender<E>, event: fn(Command) -> E) {
    runtime.spawn(async move {
        if let Err(e) = run(events, event).await {
            log(&format!("shortcuts not available: {e} (dictation --toggle still works)"));
        }
    });
}

async fn run<E>(events: Sender<E>, event: fn(Command) -> E) -> Result<(), String> {
    let portal = GlobalShortcuts::new().await.map_err(|e| format!("GlobalShortcuts portal: {e}"))?;
    // the shortcuts stay bound while the session is open
    let _session = bind(&portal).await?;
    let mut presses = Box::pin(portal.receive_activated().await.map_err(|e| format!("Activated: {e}"))?);
    while let Some(press) = std::future::poll_fn(|cx| presses.as_mut().poll_next(cx)).await {
        match command(press.shortcut_id()) {
            Some(command) => {
                if events.send(event(command)).is_err() {
                    break; // the app is quitting
                }
            }
            None => log(&format!("unknown shortcut {}", press.shortcut_id())),
        }
    }
    Ok(())
}

/// One session with the three shortcuts. Shows GNOME's dialog the first
/// time.
async fn bind(portal: &GlobalShortcuts<'static>) -> Result<ashpd::desktop::Session<'static, GlobalShortcuts<'static>>, String> {
    let wanted: Vec<NewShortcut> = SHORTCUTS
        .iter()
        .map(|(id, description, trigger)| NewShortcut::new(*id, *description).preferred_trigger(*trigger))
        .collect();
    let mut first = true;
    loop {
        let session = portal.create_session().await.map_err(|e| format!("CreateSession: {e}"))?;
        let started = Instant::now();
        let bound = portal
            .bind_shortcuts(&session, &wanted, None)
            .await
            .map_err(|e| format!("BindShortcuts: {e}"))?
            .response()
            .map_err(|e| format!("the shortcuts dialog was cancelled ({e})"))?;
        let took = started.elapsed();
        if first && rebind(took) {
            log("shortcuts allowed in the dialog; binding again");
            let _ = session.close().await;
            first = false;
            continue;
        }
        let keys: Vec<String> =
            bound.shortcuts().iter().map(|s| format!("{} {}", s.id(), s.trigger_description())).collect();
        log(&format!("shortcuts ready in {} ms: {}", took.as_millis(), keys.join(", ")));
        return Ok(session);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_shortcut_is_a_command() {
        assert_eq!(command("toggle"), Some(Command::Toggle));
        assert_eq!(command("undo"), Some(Command::Undo));
        assert_eq!(command("quit"), Some(Command::Quit));
        assert_eq!(command("dance"), None);
    }

    #[test]
    fn every_bound_shortcut_has_a_command() {
        assert!(SHORTCUTS.iter().all(|(id, _, _)| command(id).is_some()));
    }

    #[test]
    fn rebind_only_after_the_dialog() {
        assert!(!rebind(Duration::from_millis(300)));
        assert!(rebind(Duration::from_secs(12)));
    }
}
