//! The app's `.desktop` file, and its app ID for the portals.
//!
//! GNOME keeps portal permissions (the shortcuts, typing, the clipboard) per
//! app ID. A process started from a terminal has none, and GNOME then files
//! the permissions under the app that started it (S3: under VS Code). So
//! the app registers its own ID before any portal call, and registering
//! needs a `.desktop` file with that ID (docs/linux-spikes.md, S3).

use std::path::Path;
use std::time::Duration;

use crate::paths::APP_ID;

/// The `.desktop` file for the binary at `exe`.
pub fn entry(exe: &Path) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Deepgram Dictation\n\
         Comment=Push-to-talk dictation: Alt+M, speak, Alt+M\n\
         Exec={}\n\
         Icon=audio-input-microphone\n\
         Categories=Utility;Accessibility;\n\
         Terminal=false\n",
        exec_quote(&exe.to_string_lossy())
    )
}

/// Quote a path for the Exec key, as the Desktop Entry specification says:
/// in double quotes, with `"`, `` ` ``, `$` and `\` escaped, if it has any
/// character that needs it.
fn exec_quote(path: &str) -> String {
    let plain = |c: char| c.is_ascii_alphanumeric() || "/._-+".contains(c);
    if path.chars().all(plain) {
        return path.to_string();
    }
    let mut out = String::from("\"");
    for c in path.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// Write the `.desktop` file if it is missing or names another binary.
/// True if it was written.
pub fn ensure(file: &Path, exe: &Path) -> std::io::Result<bool> {
    let want = entry(exe);
    if std::fs::read_to_string(file).is_ok_and(|have| have == want) {
        return Ok(false);
    }
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(file, want)?;
    Ok(true)
}

/// After a new `.desktop` file, the portal needs a moment to see it.
const RETRY: Duration = Duration::from_millis(300);

/// Register this process under the app ID, for the portals. It must run
/// before any other portal call, on the runtime that keeps the D-Bus
/// connection for the life of the app. `attempts` > 1 retries while the
/// portal does not know the new `.desktop` file yet.
pub fn register(runtime: &tokio::runtime::Runtime, attempts: u32) -> Result<(), String> {
    let mut last = String::new();
    for attempt in 1..=attempts.max(1) {
        let app_id = APP_ID.parse().map_err(|e| format!("app ID: {e}"))?;
        match runtime.block_on(ashpd::register_host_app(app_id)) {
            Ok(()) => return Ok(()),
            Err(e) => last = e.to_string(),
        }
        if attempt < attempts {
            std::thread::sleep(RETRY);
        }
    }
    Err(last)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn entry_names_the_binary() {
        let text = entry(Path::new("/usr/bin/dictation"));
        assert!(text.starts_with("[Desktop Entry]\n"));
        assert!(text.contains("\nExec=/usr/bin/dictation\n"));
        assert!(text.contains("\nType=Application\n"));
    }

    #[test]
    fn exec_quotes_paths_with_spaces_and_specials() {
        assert_eq!(exec_quote("/home/a/bin/dictation"), "/home/a/bin/dictation");
        assert_eq!(exec_quote("/home/a/My Apps/dictation"), "\"/home/a/My Apps/dictation\"");
        assert_eq!(exec_quote("/opt/$x/dictation"), "\"/opt/\\$x/dictation\"");
    }

    #[test]
    fn ensure_writes_once_and_again_when_the_binary_moves() {
        let dir = std::env::temp_dir().join(format!("dictation-desktop-{}", std::process::id()));
        let file = dir.join("applications").join("app.desktop");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(ensure(&file, Path::new("/a/dictation")).unwrap());
        assert!(!ensure(&file, Path::new("/a/dictation")).unwrap());
        assert!(ensure(&file, &PathBuf::from("/b/dictation")).unwrap());
        assert!(std::fs::read_to_string(&file).unwrap().contains("Exec=/b/dictation"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
