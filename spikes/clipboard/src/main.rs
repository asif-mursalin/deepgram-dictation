//! Spike S2: put text on the clipboard from the background on GNOME
//! Wayland, and paste it with Ctrl+V.
//!
//!   clipboard-spike portal [--paste] [--delay S] [--hold S] [TEXT]
//!   clipboard-spike x11    [--paste] [--delay S] [--hold S] [TEXT]
//!   clipboard-spike keysym           [--delay S]            [TEXT]
//!
//! - `portal`: the Clipboard portal, tied to a RemoteDesktop session.
//! - `x11`: the X11 clipboard through XWayland (arboard), which Mutter
//!   shares with Wayland apps.
//! - `keysym`: no clipboard at all. Types each character as a keysym through
//!   the RemoteDesktop portal, to see whether GNOME types characters that
//!   are not on the active layout (English text with Arabic active).
//!
//! `--paste` presses Ctrl+V through the RemoteDesktop portal after `--delay`
//! seconds. Without it, press Ctrl+V yourself while the program holds the
//! clipboard (`--hold` seconds). The portal token is kept in
//! ~/.config/dictation-spike/portal-token-s2 (mode 0600).

use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::time::Duration;

use ashpd::desktop::remote_desktop::{DeviceType, KeyState, RemoteDesktop};
use ashpd::desktop::{PersistMode, Session};
use futures_util::StreamExt;

const DEFAULT_TEXT: &str = "Hello from the clipboard.";
const DEFAULT_DELAY: u64 = 10;
const DEFAULT_HOLD: u64 = 20;

/// evdev key codes, the same on every layout.
const KEY_LEFTCTRL: i32 = 29;
const KEY_V: i32 = 47;

/// The clipboard formats offered. The KDE hint asks clipboard history tools
/// not to keep the entry; GNOME history extensions may ignore it.
const TEXT_TYPES: [&str; 3] = ["text/plain;charset=utf-8", "text/plain", "UTF8_STRING"];
const HISTORY_HINT: &str = "x-kde-passwordManagerHint";

#[derive(Debug, PartialEq)]
enum Mode {
    Portal,
    X11,
    Keysym,
}

#[derive(Debug)]
struct Args {
    mode: Mode,
    paste: bool,
    delay: u64,
    hold: u64,
    text: String,
}

#[tokio::main]
async fn main() {
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(msg) => {
            eprintln!("{msg}");
            eprintln!("usage: clipboard-spike portal|x11|keysym [--paste] [--delay S] [--hold S] [TEXT]");
            std::process::exit(2);
        }
    };
    let result = match args.mode {
        Mode::Portal => portal(&args).await,
        Mode::X11 => x11(&args).await,
        Mode::Keysym => keysym(&args).await,
    };
    if let Err(msg) = result {
        eprintln!("FAILED: {msg}");
        std::process::exit(1);
    }
}

async fn portal(args: &Args) -> Result<(), String> {
    let (remote, session) = open_session(true).await?;
    let clipboard = ashpd::desktop::clipboard::Clipboard::new().await.map_err(err("clipboard portal"))?;
    let mut types: Vec<&str> = TEXT_TYPES.to_vec();
    types.push(HISTORY_HINT);
    clipboard.set_selection(&session, &types).await.map_err(err("SetSelection"))?;
    println!("Clipboard set through the portal.");

    // Answer each app that reads the clipboard, while we hold it.
    let mut transfers = Box::pin(clipboard.receive_selection_transfer().await.map_err(err("SelectionTransfer"))?);
    let text = args.text.clone();
    let serve = async {
        while let Some((_, mime, serial)) = transfers.next().await {
            let data = if mime == HISTORY_HINT { "secret".to_string() } else { text.clone() };
            let ok = match clipboard.selection_write(&session, serial).await {
                Ok(fd) => fs::File::from(std::os::fd::OwnedFd::from(fd)).write_all(data.as_bytes()).is_ok(),
                Err(e) => {
                    println!("SelectionWrite failed: {e}");
                    false
                }
            };
            let _ = clipboard.selection_write_done(&session, serial, ok).await;
            println!("An app read the clipboard as {mime}: {}", if ok { "sent" } else { "failed" });
        }
    };
    let act = async {
        if args.paste {
            wait_for_target(args.delay).await;
            ctrl_v(&remote, &session).await?;
            println!("Pressed Ctrl+V.");
        }
        println!("Holding the clipboard for {} s. Paste somewhere to test.", args.hold);
        tokio::time::sleep(Duration::from_secs(args.hold)).await;
        Ok::<(), String>(())
    };
    tokio::select! {
        _ = serve => Err("the transfer stream ended".to_string()),
        result = act => result,
    }
}

async fn x11(args: &Args) -> Result<(), String> {
    println!("DISPLAY={}", std::env::var("DISPLAY").unwrap_or_default());
    // arboard keeps serving the clipboard while `clipboard` is alive.
    let mut clipboard = arboard::Clipboard::new().map_err(err("X11 clipboard"))?;
    clipboard.set_text(args.text.clone()).map_err(err("set_text"))?;
    println!("Clipboard set through X11 (XWayland).");
    if args.paste {
        let (remote, session) = open_session(false).await?;
        wait_for_target(args.delay).await;
        ctrl_v(&remote, &session).await?;
        println!("Pressed Ctrl+V.");
    }
    println!("Holding the clipboard for {} s. Paste somewhere to test.", args.hold);
    tokio::time::sleep(Duration::from_secs(args.hold)).await;
    Ok(())
}

async fn keysym(args: &Args) -> Result<(), String> {
    let (remote, session) = open_session(false).await?;
    wait_for_target(args.delay).await;
    for c in args.text.chars() {
        let sym = keysym_for(c);
        for state in [KeyState::Pressed, KeyState::Released] {
            remote.notify_keyboard_keysym(&session, sym, state).await.map_err(err("keysym"))?;
        }
    }
    println!("Sent {} keysyms.", args.text.chars().count());
    // Let GNOME handle the last keys before the session closes (S1).
    tokio::time::sleep(Duration::from_secs(1)).await;
    Ok(())
}

/// A RemoteDesktop session with the keyboard, and the clipboard if asked.
async fn open_session(with_clipboard: bool) -> Result<(RemoteDesktop<'static>, Session<'static, RemoteDesktop<'static>>), String> {
    let path = token_path().ok_or("no home folder")?;
    let saved = fs::read_to_string(&path).ok().map(|t| t.trim().to_string());
    println!("Restore token: {}", if saved.is_some() { "found" } else { "none, expect the dialog" });

    let remote = RemoteDesktop::new().await.map_err(err("RemoteDesktop portal"))?;
    let session = remote.create_session().await.map_err(err("CreateSession"))?;
    if with_clipboard {
        let clipboard = ashpd::desktop::clipboard::Clipboard::new().await.map_err(err("clipboard portal"))?;
        clipboard.request(&session).await.map_err(err("RequestClipboard"))?;
    }
    remote
        .select_devices(&session, DeviceType::Keyboard.into(), saved.as_deref(), PersistMode::ExplicitlyRevoked)
        .await
        .map_err(err("SelectDevices"))?;
    let selected = remote
        .start(&session, None)
        .await
        .map_err(err("Start"))?
        .response()
        .map_err(err("Start response (dialog cancelled?)"))?;
    if let Some(token) = selected.restore_token() {
        save_token(&path, token).map_err(err("save token"))?;
    }
    println!("Session started (clipboard {}).", if with_clipboard { "requested" } else { "not requested" });
    Ok((remote, session))
}

async fn ctrl_v(remote: &RemoteDesktop<'_>, session: &Session<'_, RemoteDesktop<'_>>) -> Result<(), String> {
    let keys = [
        (KEY_LEFTCTRL, KeyState::Pressed),
        (KEY_V, KeyState::Pressed),
        (KEY_V, KeyState::Released),
        (KEY_LEFTCTRL, KeyState::Released),
    ];
    for (key, state) in keys {
        remote.notify_keyboard_keycode(session, key, state).await.map_err(err("keycode"))?;
    }
    Ok(())
}

async fn wait_for_target(delay: u64) {
    println!("Click into the target window. Acting in {delay} s...");
    tokio::time::sleep(Duration::from_secs(delay)).await;
}

/// The X keysym of a character: Latin-1 keeps its code, anything else is
/// the Unicode keysym 0x01000000 + code point.
fn keysym_for(c: char) -> i32 {
    let code = c as u32;
    let sym = match code {
        0x20..=0x7e | 0xa0..=0xff => code,
        _ => 0x0100_0000 | code,
    };
    sym as i32
}

fn parse_args(mut it: impl Iterator<Item = String>) -> Result<Args, String> {
    let mode = match it.next().as_deref() {
        Some("portal") => Mode::Portal,
        Some("x11") => Mode::X11,
        Some("keysym") => Mode::Keysym,
        _ => return Err("the first argument must be portal, x11 or keysym".into()),
    };
    let mut args = Args { mode, paste: false, delay: DEFAULT_DELAY, hold: DEFAULT_HOLD, text: String::new() };
    let mut words = Vec::new();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--paste" => args.paste = true,
            "--delay" => args.delay = number(it.next(), "--delay")?,
            "--hold" => args.hold = number(it.next(), "--hold")?,
            _ => words.push(arg),
        }
    }
    args.text = if words.is_empty() { DEFAULT_TEXT.to_string() } else { words.join(" ") };
    Ok(args)
}

fn number(value: Option<String>, flag: &str) -> Result<u64, String> {
    value.and_then(|v| v.parse().ok()).ok_or(format!("{flag} needs a number"))
}

fn err<E: std::fmt::Display>(what: &'static str) -> impl Fn(E) -> String {
    move |e| format!("{what}: {e}")
}

fn token_path() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(config.join("dictation-spike").join("portal-token-s2"))
}

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
    fn mode_is_required() {
        assert!(parse(&[]).is_err());
        assert!(parse(&["wayland"]).is_err());
    }

    #[test]
    fn defaults() {
        let args = parse(&["portal"]).unwrap();
        assert_eq!(args.mode, Mode::Portal);
        assert!(!args.paste);
        assert_eq!((args.delay, args.hold), (DEFAULT_DELAY, DEFAULT_HOLD));
        assert_eq!(args.text, DEFAULT_TEXT);
    }

    #[test]
    fn flags_and_text() {
        let args = parse(&["x11", "--paste", "--delay", "3", "--hold", "5", "Dr.", "Smith"]).unwrap();
        assert_eq!(args.mode, Mode::X11);
        assert!(args.paste);
        assert_eq!((args.delay, args.hold), (3, 5));
        assert_eq!(args.text, "Dr. Smith");
    }

    #[test]
    fn keysyms_latin1_and_unicode() {
        assert_eq!(keysym_for('H'), 0x48);
        assert_eq!(keysym_for(' '), 0x20);
        assert_eq!(keysym_for('é'), 0xe9);
        assert_eq!(keysym_for('ع'), 0x0100_0639);
        assert_eq!(keysym_for('—'), 0x0100_2014);
    }
}
