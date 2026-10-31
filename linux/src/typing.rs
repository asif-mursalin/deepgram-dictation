//! Typing the text at the cursor, through the RemoteDesktop portal.
//!
//! One portal session for the life of the app, with the keyboard and the
//! clipboard. GNOME asks once ("Allow Remote Interaction", "Allow
//! Clipboard Access"); the restore token in ~/.config/dictation/
//! portal-token skips the dialog after that, also after a reboot (S1).
//!
//! The text is typed as keysyms (S2): GNOME finds each one on any of the
//! user's layouts, so English comes out right with Arabic active. Keysyms
//! of characters that are on no layout (é, —, emoji) are dropped, so such
//! a text is pasted from the clipboard instead (T9).

use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

use ashpd::desktop::remote_desktop::{DeviceType, KeyState, RemoteDesktop};
use ashpd::desktop::{PersistMode, Session};
use tokio::runtime::Runtime;

use dictation_core::log;

/// X keysym for Return: a new line in the text ("new paragraph").
const KEYSYM_RETURN: i32 = 0xff0d;
/// X keysym for BackSpace.
const KEYSYM_BACKSPACE: i32 = 0xff08;

/// How a text reaches the cursor.
#[derive(Debug, PartialEq)]
pub enum Delivery {
    /// Type it, key by key.
    Type,
    /// Paste it from the clipboard: it has a character that may be on no
    /// keyboard layout.
    Paste,
}

/// Type only printable ASCII, spaces and new lines: those are on the US
/// layout, which every English dictation needs anyway (SPEC.md).
pub fn delivery(text: &str) -> Delivery {
    if text.chars().all(|c| keysym(c).is_some()) {
        Delivery::Type
    } else {
        Delivery::Paste
    }
}

/// The keysym of a character that is safe to type. ASCII keysyms are the
/// character codes.
fn keysym(c: char) -> Option<i32> {
    match c {
        '\n' => Some(KEYSYM_RETURN),
        ' '..='~' => Some(c as i32),
        _ => None,
    }
}

struct Portal {
    remote: RemoteDesktop<'static>,
    session: Session<'static, RemoteDesktop<'static>>,
}

pub struct Typer {
    token: PathBuf,
    portal: Option<Portal>,
}

impl Typer {
    pub fn new(token: PathBuf) -> Typer {
        Typer { token, portal: None }
    }

    /// Open the portal session now, so the first dialog shows at start,
    /// not in the middle of the first dictation.
    pub fn connect(&mut self, runtime: &Runtime) -> Result<(), String> {
        if self.portal.is_none() {
            self.portal = Some(runtime.block_on(open(&self.token))?);
        }
        Ok(())
    }

    /// Type `text` at the cursor. The caller has checked `delivery`.
    pub fn type_text(&mut self, runtime: &Runtime, text: &str) -> Result<(), String> {
        let keys: Vec<i32> = text.chars().filter_map(keysym).collect();
        self.send(runtime, &keys)
    }

    /// Delete `count` characters before the cursor.
    pub fn backspaces(&mut self, runtime: &Runtime, count: usize) -> Result<(), String> {
        self.send(runtime, &vec![KEYSYM_BACKSPACE; count])
    }

    /// Press and release each key. If the session broke (GNOME closes it
    /// when the user revokes the permission), open a new one and try once
    /// more.
    fn send(&mut self, runtime: &Runtime, keys: &[i32]) -> Result<(), String> {
        for attempt in 0..2 {
            self.connect(runtime)?;
            let portal = self.portal.as_ref().expect("connected");
            match runtime.block_on(press(portal, keys)) {
                Ok(()) => return Ok(()),
                Err(e) if attempt == 0 => {
                    log(&format!("typing failed ({e}); opening a new portal session"));
                    self.portal = None;
                }
                Err(e) => return Err(e),
            }
        }
        unreachable!()
    }
}

async fn open(token_path: &PathBuf) -> Result<Portal, String> {
    let saved = load_token(token_path);
    let remote = RemoteDesktop::new().await.map_err(|e| format!("RemoteDesktop portal: {e}"))?;
    let session = remote.create_session().await.map_err(|e| format!("CreateSession: {e}"))?;
    // T9 uses the clipboard; ask for it now, so there is only one dialog.
    match ashpd::desktop::clipboard::Clipboard::new().await {
        Ok(clipboard) => {
            if let Err(e) = clipboard.request(&session).await {
                log(&format!("clipboard not available: {e}"));
            }
        }
        Err(e) => log(&format!("clipboard portal not available: {e}")),
    }
    remote
        .select_devices(&session, DeviceType::Keyboard.into(), saved.as_deref(), PersistMode::ExplicitlyRevoked)
        .await
        .map_err(|e| format!("SelectDevices: {e}"))?;
    let selected = remote
        .start(&session, None)
        .await
        .map_err(|e| format!("Start: {e}"))?
        .response()
        .map_err(|e| format!("the permission dialog was cancelled ({e})"))?;
    // GNOME gives a new token each time; the old one no longer works (S1).
    if let Some(token) = selected.restore_token() {
        if let Err(e) = save_token(token_path, token) {
            log(&format!("could not save the portal token: {e}"));
        }
    }
    log(&format!("typing ready ({})", if saved.is_some() { "permission remembered" } else { "permission granted" }));
    Ok(Portal { remote, session })
}

async fn press(portal: &Portal, keys: &[i32]) -> Result<(), String> {
    for &key in keys {
        for state in [KeyState::Pressed, KeyState::Released] {
            portal
                .remote
                .notify_keyboard_keysym(&portal.session, key, state)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn load_token(path: &PathBuf) -> Option<String> {
    fs::read_to_string(path).ok().map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
}

/// The token lets a program skip the permission dialog, so only the user
/// may read it.
fn save_token(path: &PathBuf, token: &str) -> std::io::Result<()> {
    let mut file = fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)?;
    file.write_all(token.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn plain_english_is_typed() {
        assert_eq!(delivery("Hello, world. It's 5:30 (approx.) - ok?"), Delivery::Type);
        assert_eq!(delivery("First line.\nSecond line."), Delivery::Type);
        assert_eq!(delivery(""), Delivery::Type);
    }

    #[test]
    fn characters_on_no_layout_are_pasted() {
        assert_eq!(delivery("café"), Delivery::Paste);
        assert_eq!(delivery("a — b"), Delivery::Paste);
        assert_eq!(delivery("“quoted”"), Delivery::Paste);
        assert_eq!(delivery("ok 👍"), Delivery::Paste);
        assert_eq!(delivery("tab\there"), Delivery::Paste);
    }

    #[test]
    fn keysyms_are_the_ascii_codes() {
        assert_eq!(keysym('A'), Some(0x41));
        assert_eq!(keysym(' '), Some(0x20));
        assert_eq!(keysym('~'), Some(0x7e));
        assert_eq!(keysym('\n'), Some(KEYSYM_RETURN));
        assert_eq!(keysym('é'), None);
    }

    #[test]
    fn the_token_is_saved_private_and_read_back() {
        let path = std::env::temp_dir().join(format!("dictation-token-{}", std::process::id()));
        let _ = fs::remove_file(&path);
        assert_eq!(load_token(&path), None);
        save_token(&path, "abc123").unwrap();
        assert_eq!(load_token(&path).as_deref(), Some("abc123"));
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        save_token(&path, "new").unwrap();
        assert_eq!(load_token(&path).as_deref(), Some("new"));
        let _ = fs::remove_file(&path);
    }
}
