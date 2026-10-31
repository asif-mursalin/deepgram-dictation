//! Where the app keeps its files, from the XDG base directories:
//!
//!   ~/.config/dictation/        .env (the API key), words.txt
//!   ~/.local/state/dictation/   dictation.log
//!   $XDG_RUNTIME_DIR/           dictation.sock (one copy, --toggle)
//!   ~/.local/share/applications/<APP_ID>.desktop

use std::ffi::OsString;
use std::path::PathBuf;

/// The app ID, for the `.desktop` file and the portals (SPEC.md).
pub const APP_ID: &str = "io.github.rawasmhd.DeepgramDictation";

pub struct Paths {
    pub config: PathBuf,
    pub state: PathBuf,
    runtime: PathBuf,
    data: PathBuf,
    /// DICTATION_LOG overrides the log file, for the benchmark.
    log: Option<PathBuf>,
}

impl Paths {
    pub fn from_env() -> Option<Paths> {
        Paths::from_vars(|name| std::env::var_os(name))
    }

    /// `var` reads an environment variable. None without a home folder.
    fn from_vars(var: impl Fn(&str) -> Option<OsString>) -> Option<Paths> {
        let home = var("HOME").map(PathBuf::from).filter(|h| h.is_absolute());
        // the XDG variable if it is an absolute path, else the default under ~
        let xdg = |name: &str, default: &str| {
            var(name).map(PathBuf::from).filter(|p| p.is_absolute()).or_else(|| home.as_ref().map(|h| h.join(default)))
        };
        let state = xdg("XDG_STATE_HOME", ".local/state")?.join("dictation");
        Some(Paths {
            config: xdg("XDG_CONFIG_HOME", ".config")?.join("dictation"),
            runtime: var("XDG_RUNTIME_DIR").map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| state.clone()),
            data: xdg("XDG_DATA_HOME", ".local/share")?,
            log: var("DICTATION_LOG").map(PathBuf::from),
            state,
        })
    }

    pub fn log(&self) -> PathBuf {
        self.log.clone().unwrap_or_else(|| self.state.join("dictation.log"))
    }

    pub fn socket(&self) -> PathBuf {
        self.runtime.join("dictation.sock")
    }

    pub fn desktop_file(&self) -> PathBuf {
        self.data.join("applications").join(format!("{APP_ID}.desktop"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(vars: &[(&str, &str)]) -> Option<Paths> {
        Paths::from_vars(|name| vars.iter().find(|(n, _)| *n == name).map(|(_, v)| OsString::from(v)))
    }

    #[test]
    fn defaults_under_home() {
        let p = paths(&[("HOME", "/home/a"), ("XDG_RUNTIME_DIR", "/run/user/1000")]).unwrap();
        assert_eq!(p.config, PathBuf::from("/home/a/.config/dictation"));
        assert_eq!(p.log(), PathBuf::from("/home/a/.local/state/dictation/dictation.log"));
        assert_eq!(p.socket(), PathBuf::from("/run/user/1000/dictation.sock"));
        assert_eq!(
            p.desktop_file(),
            PathBuf::from("/home/a/.local/share/applications/io.github.rawasmhd.DeepgramDictation.desktop")
        );
    }

    #[test]
    fn xdg_variables_win() {
        let p = paths(&[("HOME", "/home/a"), ("XDG_CONFIG_HOME", "/cfg"), ("XDG_STATE_HOME", "/st"), ("XDG_DATA_HOME", "/data")])
            .unwrap();
        assert_eq!(p.config, PathBuf::from("/cfg/dictation"));
        assert_eq!(p.log(), PathBuf::from("/st/dictation/dictation.log"));
        assert_eq!(p.desktop_file(), PathBuf::from("/data/applications/io.github.rawasmhd.DeepgramDictation.desktop"));
    }

    #[test]
    fn relative_xdg_variables_are_ignored() {
        let p = paths(&[("HOME", "/home/a"), ("XDG_CONFIG_HOME", "cfg")]).unwrap();
        assert_eq!(p.config, PathBuf::from("/home/a/.config/dictation"));
    }

    #[test]
    fn without_a_runtime_dir_the_socket_goes_to_the_state_folder() {
        let p = paths(&[("HOME", "/home/a")]).unwrap();
        assert_eq!(p.socket(), PathBuf::from("/home/a/.local/state/dictation/dictation.sock"));
    }

    #[test]
    fn dictation_log_overrides_the_log() {
        let p = paths(&[("HOME", "/home/a"), ("DICTATION_LOG", "/tmp/bench.log")]).unwrap();
        assert_eq!(p.log(), PathBuf::from("/tmp/bench.log"));
    }

    #[test]
    fn no_home_no_paths() {
        assert!(paths(&[]).is_none());
        assert!(paths(&[("HOME", "relative")]).is_none());
    }
}
