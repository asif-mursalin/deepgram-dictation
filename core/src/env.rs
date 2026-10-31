//! Settings from the environment, and the API key from a `.env` file. The
//! app chooses the folders: the Windows app looks next to the .exe, the
//! Linux app in its XDG config folder.

use std::path::{Path, PathBuf};

const KEY_NAME: &str = "DEEPGRAM_API_KEY";

/// "streaming" (default) or "batch". The benchmark can override it with
/// DICTATION_MODE, so both versions run with the same settings.
pub fn streaming() -> bool {
    std::env::var("DICTATION_MODE").map_or(true, |m| m != "batch")
}

/// Streaming only: paste each phrase while you speak (default).
/// DICTATION_LIVE_PASTE=0 pastes all the text when you stop.
pub fn live_paste() -> bool {
    std::env::var("DICTATION_LIVE_PASTE").map_or(true, |v| !matches!(v.as_str(), "0" | "false"))
}

/// DEEPGRAM_API_KEY from the environment, or from the `.env` file in the
/// first of `dirs` that has one.
pub fn api_key(dirs: &[PathBuf]) -> Option<String> {
    if let Ok(key) = std::env::var(KEY_NAME) {
        if !key.trim().is_empty() {
            return Some(key.trim().to_string());
        }
    }
    dirs.iter().map(|dir| dir.join(".env")).find_map(|p| read_env_file(&p))
}

fn read_env_file(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    parse_env(&text)
}

fn parse_env(text: &str) -> Option<String> {
    text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).find_map(|line| {
        let (name, value) = line.split_once('=')?;
        (name.trim() == KEY_NAME).then(|| value.trim().trim_matches(['"', '\'']).to_string())
    })
}

/// Save the key to `.env` in `dir`. Other lines in the file stay.
pub fn save_api_key(dir: &Path, key: &str) -> Result<PathBuf, String> {
    let path = dir.join(".env");
    let old = std::fs::read_to_string(&path).unwrap_or_default();
    std::fs::write(&path, with_key(&old, key)).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// `text` (a .env file) with the key line replaced, or added.
fn with_key(text: &str, key: &str) -> String {
    let line = format!("{KEY_NAME}={key}");
    let mut found = false;
    let mut lines: Vec<String> = text
        .lines()
        .map(|l| match l.split_once('=') {
            Some((name, _)) if name.trim() == KEY_NAME && !l.trim_start().starts_with('#') => {
                found = true;
                line.clone()
            }
            _ => l.to_string(),
        })
        .collect();
    if !found {
        lines.push(line);
    }
    lines.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_quoted_key() {
        assert_eq!(parse_env("DEEPGRAM_API_KEY=\"abc\"\n"), Some("abc".into()));
    }

    #[test]
    fn ignores_comments_blanks_and_other_names() {
        let text = "# comment\n\nOTHER=1\n DEEPGRAM_API_KEY = 'xyz' \n";
        assert_eq!(parse_env(text), Some("xyz".into()));
    }

    #[test]
    fn missing_key_is_none() {
        assert_eq!(parse_env("OTHER=1\n"), None);
    }

    #[test]
    fn saving_replaces_the_key_and_keeps_other_lines() {
        let old = "# my keys\nOTHER=1\nDEEPGRAM_API_KEY=old\n";
        assert_eq!(with_key(old, "new"), "# my keys\nOTHER=1\nDEEPGRAM_API_KEY=new\n");
    }

    #[test]
    fn saving_adds_the_key_when_missing() {
        assert_eq!(with_key("", "new"), "DEEPGRAM_API_KEY=new\n");
        assert_eq!(with_key("# DEEPGRAM_API_KEY=x\n", "new"), "# DEEPGRAM_API_KEY=x\nDEEPGRAM_API_KEY=new\n");
    }
}
