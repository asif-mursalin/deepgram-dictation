//! The clipboard, through the Clipboard portal on the typing session (S2).
//!
//! Each dictation goes on the clipboard once, at the end, so a clipboard
//! history gets one entry for it. While the app owns the clipboard, each
//! app that pastes asks the portal for the text (`SelectionTransfer`), and
//! `serve` writes it.

use std::fs::File;
use std::io::Write;
use std::os::fd::OwnedFd;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use ashpd::desktop::clipboard::Clipboard;
use ashpd::desktop::remote_desktop::RemoteDesktop;
use ashpd::desktop::Session;
use ashpd::zbus::export::futures_core::Stream;

use dictation_core::log;

/// How long the target app gets to read a paste before the clipboard
/// changes again, as on Windows (CLIPBOARD_DELAY_MS).
pub const PASTE_READ: Duration = Duration::from_millis(500);

/// The formats the app offers.
pub const TYPES: [&str; 4] = ["text/plain;charset=utf-8", "text/plain", "UTF8_STRING", HISTORY_HINT];

/// Asks clipboard history tools not to keep the entry. KDE's Klipper
/// honors it; GNOME history extensions ignore it (S2).
const HISTORY_HINT: &str = "x-kde-passwordManagerHint";

/// What to send an app that reads the clipboard as `mime`. None for a
/// format the app did not offer.
pub fn data(mime: &str, text: &str) -> Option<Vec<u8>> {
    match mime {
        HISTORY_HINT => Some(b"secret".to_vec()),
        _ if TYPES.contains(&mime) => Some(text.as_bytes().to_vec()),
        _ => None,
    }
}

/// Answer each app that reads the clipboard with `text`, for the life of
/// the session. Runs on the portal runtime, so an app can read while the
/// main thread presses Ctrl+V.
pub async fn serve(clipboard: &Clipboard<'_>, session: &Session<'_, RemoteDesktop<'_>>, text: &Mutex<String>) {
    let mut transfers = match clipboard.receive_selection_transfer().await {
        Ok(transfers) => Box::pin(transfers),
        Err(e) => {
            log(&format!("clipboard: cannot listen for pastes: {e}"));
            return;
        }
    };
    while let Some((_, mime, serial)) = std::future::poll_fn(|cx| transfers.as_mut().poll_next(cx)).await {
        let bytes = data(&mime, &text.lock().unwrap_or_else(|e| e.into_inner()));
        let ok = match bytes {
            Some(bytes) => match clipboard.selection_write(session, serial).await {
                // the file closes before SelectionWriteDone
                Ok(fd) => File::from(OwnedFd::from(fd)).write_all(&bytes).is_ok(),
                Err(e) => {
                    log(&format!("clipboard: SelectionWrite failed: {e}"));
                    false
                }
            },
            None => false,
        };
        let _ = clipboard.selection_write_done(session, serial, ok).await;
    }
}

/// The text for the clipboard at the end of a dictation, so it is set
/// once: a paste that went through the clipboard already put it there.
#[derive(Default)]
pub struct Pending {
    wanted: Option<String>,
    on_clipboard: Option<String>,
    /// When the last paste pressed Ctrl+V.
    pasted_at: Option<Instant>,
    /// The dictation has ended.
    done: bool,
}

impl Pending {
    /// The dictation's text so far, for the clipboard at the end.
    pub fn want(&mut self, text: &str) {
        self.wanted = Some(text.to_string());
        self.done = false;
    }

    /// The text is on the clipboard now, and Ctrl+V was pressed.
    pub fn copied(&mut self, text: &str, now: Instant) {
        self.on_clipboard = Some(text.to_string());
        self.pasted_at = Some(now);
    }

    /// The dictation has ended: `take` may give its text.
    pub fn done(&mut self) {
        self.done = true;
    }

    /// The text to put on the clipboard now: once the dictation has ended,
    /// the target app has had time to read the last paste, and only if it
    /// is not on the clipboard already.
    pub fn take(&mut self, now: Instant) -> Option<String> {
        if !self.done || self.pasted_at.is_some_and(|t| now < t + PASTE_READ) {
            return None;
        }
        self.done = false;
        self.pasted_at = None;
        // the user may copy something else before the next dictation
        let on_clipboard = self.on_clipboard.take();
        self.wanted.take().filter(|text| on_clipboard.as_ref() != Some(text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_formats_get_the_text() {
        for mime in ["text/plain;charset=utf-8", "text/plain", "UTF8_STRING"] {
            assert_eq!(data(mime, "café").as_deref(), Some("café".as_bytes()));
        }
    }

    #[test]
    fn the_history_hint_says_secret() {
        assert_eq!(data(HISTORY_HINT, "hello").as_deref(), Some(&b"secret"[..]));
    }

    #[test]
    fn other_formats_get_nothing() {
        assert_eq!(data("image/png", "hello"), None);
    }

    #[test]
    fn every_offered_type_has_data() {
        assert!(TYPES.iter().all(|mime| data(mime, "x").is_some()));
    }

    fn at(ms: u64) -> Instant {
        start() + Duration::from_millis(ms)
    }

    fn start() -> Instant {
        static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
        *START.get_or_init(Instant::now)
    }

    #[test]
    fn a_typed_dictation_is_copied_when_it_is_done() {
        let mut copy = Pending::default();
        assert_eq!(copy.take(at(0)), None);
        copy.want("hello there");
        // still speaking: phrases are not copied
        assert_eq!(copy.take(at(0)), None);
        copy.done();
        assert_eq!(copy.take(at(0)).as_deref(), Some("hello there"));
        assert_eq!(copy.take(at(0)), None);
    }

    #[test]
    fn a_pasted_dictation_is_not_copied_twice() {
        let mut copy = Pending::default();
        copy.want("café");
        copy.copied("café", at(0));
        copy.done();
        assert_eq!(copy.take(at(1000)), None);
    }

    #[test]
    fn live_paste_copies_the_whole_text_after_the_app_read_the_paste() {
        // "In" typed, " ed €15" pasted, then the full text for the clipboard
        let mut copy = Pending::default();
        copy.want("In");
        copy.want(" ed €15");
        copy.copied(" ed €15", at(0));
        copy.want("In ed €15");
        copy.done();
        // the target app may not have read " ed €15" yet
        assert_eq!(copy.take(at(100)), None);
        assert_eq!(copy.take(at(PASTE_READ.as_millis() as u64)).as_deref(), Some("In ed €15"));
    }

    #[test]
    fn the_same_text_in_the_next_dictation_is_copied_again() {
        let mut copy = Pending::default();
        copy.want("yes");
        copy.copied("yes", at(0));
        copy.done();
        assert_eq!(copy.take(at(1000)), None);
        // the user copied something else in between
        copy.want("yes");
        copy.done();
        assert_eq!(copy.take(at(2000)).as_deref(), Some("yes"));
    }

    #[test]
    fn a_new_dictation_before_the_copy_waits_for_its_own_end() {
        let mut copy = Pending::default();
        copy.want("first");
        copy.done();
        copy.want("second phrase");
        assert_eq!(copy.take(at(0)), None);
    }
}
