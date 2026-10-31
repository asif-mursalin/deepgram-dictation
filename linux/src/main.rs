//! Deepgram dictation for Linux (GNOME and KDE on Wayland).
//!
//!   dictation            start the app (only one copy runs)
//!   dictation --toggle   start or stop a dictation in the running app
//!   dictation --quit     quit the running app
//!
//! The dictation itself (`dictation_core::session`) is the same as on
//! Windows. This file has the start, the event loop, and the `Host` that
//! connects the session to Linux: the text is typed at the cursor through
//! the RemoteDesktop portal (`typing.rs`).

mod control;
mod desktop;
mod paths;
mod typing;

use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use dictation_core::session::{self, Host, Session, State, Transcript};
use dictation_core::words::{self, Words};
use dictation_core::{audio, deepgram, env, log, WindowId};

use control::Command;
use paths::Paths;
use typing::{Delivery, Typer};

/// Check live paste this often while recording, as the Windows app does
/// (8 frames of 33 ms).
const LIVE_CHECK: Duration = Duration::from_millis(250);

/// GNOME does not tell an app which window has focus, so every dictation
/// targets "the focused window" (SPEC.md).
const FOCUSED_WINDOW: WindowId = 0;

enum Event {
    Command(Command),
    /// Deepgram sent a new phrase.
    LiveText,
    Transcript(Transcript),
}

fn main() {
    let Some(paths) = Paths::from_env() else {
        eprintln!("dictation: HOME is not set");
        std::process::exit(1);
    };
    let command = match std::env::args().nth(1).as_deref() {
        None => None,
        Some("--toggle") => Some(Command::Toggle),
        Some("--quit") => Some(Command::Quit),
        Some(other) => {
            eprintln!("dictation: unknown argument {other}\nusage: dictation [--toggle | --quit]");
            std::process::exit(2);
        }
    };
    if let Some(command) = command {
        if let Err(e) = control::send(&paths.socket(), command) {
            eprintln!("dictation: the app is not running ({e})");
            std::process::exit(1);
        }
        return;
    }
    if let Err(msg) = run(&paths) {
        log(&format!("dictation: {msg}"));
        std::process::exit(1);
    }
}

fn run(paths: &Paths) -> Result<(), String> {
    for dir in [&paths.config, &paths.state] {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    dictation_core::logging::init(Some(paths.log()), now);

    let socket = paths.socket();
    let listener = control::listen(&socket)
        .map_err(|e| format!("{}: {e}", socket.display()))?
        .ok_or("already running")?;

    // The portals need the app ID, and the ID needs the .desktop file (S3).
    // The runtime keeps the D-Bus connection for the life of the app.
    let portals = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .map_err(|e| format!("runtime: {e}"))?;
    let exe = std::env::current_exe().map_err(|e| format!("exe: {e}"))?;
    let wrote = match desktop::ensure(&paths.desktop_file(), &exe) {
        Ok(wrote) => wrote,
        Err(e) => {
            log(&format!("{}: {e}", paths.desktop_file().display()));
            false
        }
    };
    if wrote {
        log(&format!("wrote {}", paths.desktop_file().display()));
    }
    match desktop::register(&portals, if wrote { 10 } else { 1 }) {
        Ok(()) => log(&format!("registered as {}", paths::APP_ID)),
        Err(e) => log(&format!("could not register the app ID: {e}")),
    }

    // Open the typing session now: the first start shows GNOME's dialog.
    let mut typer = Typer::new(paths.config.join("portal-token"));
    if let Err(e) = typer.connect(&portals) {
        log(&format!("typing not ready: {e}"));
    }

    words::set_dir(paths.config.clone());
    let key = env::api_key(&[paths.config.clone()])
        .ok_or_else(|| format!("no API key: put DEEPGRAM_API_KEY=... in {}", paths.config.join(".env").display()))?;
    log(&format!(
        "ready: {} mode, live paste {}",
        if env::streaming() { "streaming" } else { "batch" },
        if env::streaming() && env::live_paste() { "on" } else { "off" },
    ));

    let (events, rx) = mpsc::channel();
    control::forward(listener, events.clone(), Event::Command);
    let mut session = Session::new(Linux { key, events, portals, typer }, Words::new());
    loop {
        match rx.recv_timeout(LIVE_CHECK) {
            Ok(Event::Command(Command::Toggle)) => session.toggle(),
            Ok(Event::Command(Command::Quit)) => break,
            Ok(Event::LiveText) => session.live_step(),
            Ok(Event::Transcript(result)) => session.on_transcript(result),
            Err(RecvTimeoutError::Timeout) => {
                if session.recording() {
                    session.live_step();
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    let _ = std::fs::remove_file(&socket);
    log("quit");
    Ok(())
}

/// The local time for the log, like the Windows app's GetLocalTime.
fn now() -> String {
    // SAFETY: time and localtime_r only write into the struct we pass.
    let tm = unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        tm
    };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec
    )
}

/// The session's link to Linux: the microphone, Deepgram and typing for
/// real; the meter and the tray only in the log until T7.
struct Linux {
    key: String,
    events: Sender<Event>,
    /// Runs the portal calls, and keeps their D-Bus connection.
    portals: tokio::runtime::Runtime,
    typer: Typer,
}

impl Host for Linux {
    type Stream = deepgram::Stream;

    fn record(&mut self, sink: Option<Sender<Vec<i16>>>) -> Result<Box<dyn session::Recording>, String> {
        audio::start(sink).map(|r| Box::new(r) as Box<dyn session::Recording>)
    }

    fn stream(&mut self, terms: Vec<String>, words: Words) -> Option<(deepgram::Stream, Sender<Vec<i16>>)> {
        if !env::streaming() {
            return None;
        }
        let events = self.events.clone();
        Some(deepgram::Stream::start(self.key.clone(), terms, words, move || {
            let _ = events.send(Event::LiveText);
        }))
    }

    fn live_paste(&self) -> bool {
        env::live_paste()
    }

    /// Wait for Deepgram off the main thread.
    fn finish(&mut self, stream: Option<deepgram::Stream>, samples: Vec<i16>, terms: Vec<String>, words: Words) {
        let key = self.key.clone();
        let events = self.events.clone();
        thread::spawn(move || {
            let result = deepgram::finish(&key, stream, &samples, &terms, &words);
            let _ = events.send(Event::Transcript(result));
        });
    }

    fn foreground(&self) -> WindowId {
        FOCUSED_WINDOW
    }

    /// Type the text. False if it could not reach the cursor.
    fn paste(&mut self, text: &str) -> bool {
        let started = Instant::now();
        match typing::delivery(text) {
            Delivery::Type => match self.typer.type_text(&self.portals, text) {
                Ok(()) => {
                    log(&format!("typed {} characters in {} ms", text.chars().count(), started.elapsed().as_millis()));
                    true
                }
                Err(e) => {
                    log(&format!("typing failed: {e}"));
                    false
                }
            },
            // T9 pastes such a text from the clipboard.
            Delivery::Paste => {
                log("this text needs the clipboard paste, which is not there yet (T9)");
                false
            }
        }
    }

    fn backspaces(&mut self, count: usize) {
        if let Err(e) = self.typer.backspaces(&self.portals, count) {
            log(&format!("undo failed: {e}"));
        }
    }

    /// GNOME does not tell an app that the user typed (SPEC.md).
    fn typed_since_reset(&self) -> bool {
        false
    }

    fn reset_typing(&mut self) {}

    fn clipboard_later(&mut self, _text: String) {}

    fn show(&mut self, state: State) {
        let name = match &state {
            State::Hidden => "idle".to_string(),
            State::Recording => "recording".to_string(),
            State::Working => "transcribing".to_string(),
            State::Notice { text, error } => format!("{}: {text}", if *error { "problem" } else { "notice" }),
        };
        log(&format!("state: {name}"));
    }

    fn set_paused(&mut self, _paused: bool) {}

    fn balloon(&mut self, title: &str, text: &str) {
        log(&format!("{title}: {text}"));
    }
}
