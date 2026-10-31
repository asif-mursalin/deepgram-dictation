//! The log. The app chooses the file and the clock at start: the Windows
//! app writes dictation.log next to the .exe, the Linux app writes it to
//! its XDG state folder.

use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

const MAX_BYTES: u64 = 1_000_000; // start a new log above this size

static FILE: OnceLock<Option<Mutex<std::fs::File>>> = OnceLock::new();

/// Open the log file (`None`: only print) and write the start line, with
/// the local time from `now`.
pub fn init(path: Option<PathBuf>, now: fn() -> String) {
    FILE.get_or_init(|| {
        let path = path?;
        if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES) {
            let _ = std::fs::remove_file(&path);
        }
        let file = std::fs::OpenOptions::new().create(true).append(true).open(path).ok()?;
        Some(Mutex::new(file))
    });
    log(&format!("\n--- started {} ---", now()));
}

pub fn log(msg: &str) {
    println!("{msg}");
    if let Some(Some(file)) = FILE.get() {
        if let Ok(mut f) = file.lock() {
            let _ = writeln!(f, "{msg}");
        }
    }
}
