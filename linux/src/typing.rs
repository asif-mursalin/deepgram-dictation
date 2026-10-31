//! Typing the text at the cursor, through the RemoteDesktop portal.
//!
//! One portal session for the life of the app, with the keyboard and the
//! clipboard. GNOME asks once ("Allow Remote Interaction", "Allow
//! Clipboard Access"); the restore token in ~/.config/dictation/
//! portal-token skips the dialog after that, also after a reboot (S1).
//!
//! The text is typed as keysyms (S2), with English (US) active (SPEC.md).
//! Keysyms of characters that are on no layout (é, —, emoji) are dropped,
//! so such a text is pasted instead: put on the clipboard, then Ctrl+V.

use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use ashpd::desktop::clipboard::Clipboard;
use ashpd::desktop::remote_desktop::{DeviceType, KeyState, RemoteDesktop};
use ashpd::desktop::{PersistMode, Session};
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

use crate::clipboard;

use dictation_core::log;

/// X keysym for Return: a new line in the text ("new paragraph").
const KEYSYM_RETURN: i32 = 0xff0d;
/// X keysym for BackSpace.
const KEYSYM_BACKSPACE: i32 = 0xff08;
/// evdev key codes for Ctrl+V: the same keys on every layout (S2).
const KEY_LEFTCTRL: i32 = 29;
const KEY_V: i32 = 47;

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
    /// None if the clipboard portal is missing or was refused.
    clipboard: Option<Clipboard<'static>>,
}

/// What to do through the portal.
enum Action<'a> {
    /// Press and release each key, as keysyms.
    Keys(&'a [i32]),
    /// Put the text on the clipboard.
    Copy(&'a str),
    /// Put the text on the clipboard and press Ctrl+V.
    Paste(&'a str),
}

pub struct Typer {
    token: PathBuf,
    portal: Option<Arc<Portal>>,
    /// The text the app offers on the clipboard.
    offer: Arc<Mutex<String>>,
    /// Answers the apps that paste it, for the current session.
    serving: Option<JoinHandle<()>>,
}

impl Typer {
    pub fn new(token: PathBuf) -> Typer {
        Typer { token, portal: None, offer: Arc::default(), serving: None }
    }

    /// Open the portal session now, so the first dialog shows at start,
    /// not in the middle of the first dictation.
    pub fn connect(&mut self, runtime: &Runtime) -> Result<(), String> {
        if self.portal.is_some() {
            return Ok(());
        }
        let portal = Arc::new(runtime.block_on(open(&self.token))?);
        if let Some(old) = self.serving.take() {
            old.abort();
        }
        if portal.clipboard.is_some() {
            let (portal, offer) = (portal.clone(), self.offer.clone());
            self.serving = Some(runtime.spawn(async move {
                if let Some(clipboard) = &portal.clipboard {
                    clipboard::serve(clipboard, &portal.session, &offer).await;
                }
            }));
        }
        self.portal = Some(portal);
        Ok(())
    }

    /// Type `text` at the cursor. The caller has checked `delivery`.
    pub fn type_text(&mut self, runtime: &Runtime, text: &str) -> Result<(), String> {
        let keys: Vec<i32> = text.chars().filter_map(keysym).collect();
        self.run(runtime, Action::Keys(&keys))
    }

    /// Delete `count` characters before the cursor.
    pub fn backspaces(&mut self, runtime: &Runtime, count: usize) -> Result<(), String> {
        self.run(runtime, Action::Keys(&vec![KEYSYM_BACKSPACE; count]))
    }

    /// Put `text` on the clipboard.
    pub fn copy(&mut self, runtime: &Runtime, text: &str) -> Result<(), String> {
        self.run(runtime, Action::Copy(text))
    }

    /// Paste `text` at the cursor through the clipboard. It stays there.
    pub fn paste(&mut self, runtime: &Runtime, text: &str) -> Result<(), String> {
        self.run(runtime, Action::Paste(text))
    }

    /// If the session broke (GNOME closes it when the user revokes the
    /// permission), open a new one and try once more.
    fn run(&mut self, runtime: &Runtime, action: Action) -> Result<(), String> {
        for attempt in 0..2 {
            self.connect(runtime)?;
            let portal = self.portal.as_ref().expect("connected");
            match runtime.block_on(act(portal, &self.offer, &action)) {
                Ok(()) => return Ok(()),
                Err(e) if attempt == 0 => {
                    log(&format!("the portal call failed ({e}); opening a new portal session"));
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
    // Ask for the clipboard before Start, so it is in the same dialog.
    let clipboard = match Clipboard::new().await {
        Ok(clipboard) => match clipboard.request(&session).await {
            Ok(()) => Some(clipboard),
            Err(e) => {
                log(&format!("clipboard not available: {e}"));
                None
            }
        },
        Err(e) => {
            log(&format!("clipboard portal not available: {e}"));
            None
        }
    };
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
    Ok(Portal { remote, session, clipboard })
}

async fn act(portal: &Portal, offer: &Mutex<String>, action: &Action<'_>) -> Result<(), String> {
    let text = match action {
        Action::Keys(keys) => {
            for &key in *keys {
                for state in [KeyState::Pressed, KeyState::Released] {
                    portal.remote.notify_keyboard_keysym(&portal.session, key, state).await.map_err(|e| e.to_string())?;
                }
            }
            return Ok(());
        }
        Action::Copy(text) | Action::Paste(text) => text,
    };
    let clipboard = portal.clipboard.as_ref().ok_or("no clipboard access (see the Remote Desktop dialog)")?;
    *offer.lock().unwrap_or_else(|e| e.into_inner()) = text.to_string();
    clipboard.set_selection(&portal.session, &clipboard::TYPES).await.map_err(|e| format!("SetSelection: {e}"))?;
    if let Action::Paste(_) = action {
        let keys = [
            (KEY_LEFTCTRL, KeyState::Pressed),
            (KEY_V, KeyState::Pressed),
            (KEY_V, KeyState::Released),
            (KEY_LEFTCTRL, KeyState::Released),
        ];
        for (key, state) in keys {
            portal.remote.notify_keyboard_keycode(&portal.session, key, state).await.map_err(|e| e.to_string())?;
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
