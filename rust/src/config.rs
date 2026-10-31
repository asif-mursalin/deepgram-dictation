//! Settings, the API key (both in `dictation_core::env`), and autostart.

use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::*;

use crate::overlay::wide;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "DeepgramDictation";
/// The Startup shortcut that setup.bat creates for the Python version.
const PYTHON_SHORTCUT: &str = r"Microsoft\Windows\Start Menu\Programs\Startup\Deepgram Dictation.lnk";

pub use dictation_core::env::{live_paste, streaming};

// -- API key ------------------------------------------------------------------

/// DEEPGRAM_API_KEY from the environment, or from the first .env file
/// next to the .exe or in a folder above it (so a build in
/// rust/target/release finds the repo's .env).
pub fn api_key() -> Option<String> {
    let exe = std::env::current_exe().ok();
    let dirs: Vec<PathBuf> = exe.iter().flat_map(|e| e.ancestors().skip(1).take(5)).map(Path::to_path_buf).collect();
    dictation_core::env::api_key(&dirs)
}

/// Save the key to .env next to the .exe. Other lines in the file stay.
pub fn save_api_key(key: &str) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    dictation_core::env::save_api_key(exe.parent().ok_or("no folder for .env")?, key)
}

// -- autostart ----------------------------------------------------------------

pub fn autostart_enabled() -> bool {
    unsafe {
        let mut size = 0u32;
        RegGetValueW(
            HKEY_CURRENT_USER,
            wide(RUN_KEY).as_ptr(),
            wide(RUN_VALUE).as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut size,
        ) == ERROR_SUCCESS
    }
}

/// Start (or not) at login, through the user's Run registry key. Returns
/// true if it removed the Python version's Startup shortcut, because only
/// one version can run at a time.
pub fn set_autostart(enabled: bool) -> Result<bool, String> {
    unsafe {
        if !enabled {
            RegDeleteKeyValueW(HKEY_CURRENT_USER, wide(RUN_KEY).as_ptr(), wide(RUN_VALUE).as_ptr());
            return Ok(false);
        }
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let command = wide(&format!("\"{}\"", exe.display()));
        let status = RegSetKeyValueW(
            HKEY_CURRENT_USER,
            wide(RUN_KEY).as_ptr(),
            wide(RUN_VALUE).as_ptr(),
            REG_SZ,
            command.as_ptr().cast(),
            (command.len() * 2) as u32,
        );
        if status != ERROR_SUCCESS {
            return Err(format!("registry error {status}"));
        }
    }
    let shortcut = std::env::var_os("APPDATA").map(|d| Path::new(&d).join(PYTHON_SHORTCUT));
    Ok(shortcut.is_some_and(|s| std::fs::remove_file(s).is_ok()))
}
