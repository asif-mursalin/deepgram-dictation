//! The log: `dictation.log` next to the .exe (there is no console in a
//! release build). DICTATION_LOG overrides the path, for the benchmark.
//! Writing is in `dictation_core::logging`.

use std::path::PathBuf;

use windows_sys::Win32::System::SystemInformation::GetLocalTime;

pub fn init() {
    dictation_core::logging::init(path(), now);
}

/// The log file: DICTATION_LOG, or dictation.log next to the .exe.
pub fn path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("DICTATION_LOG") {
        return Some(path.into());
    }
    Some(std::env::current_exe().ok()?.with_file_name("dictation.log"))
}

fn now() -> String {
    let t = unsafe {
        let mut t = std::mem::zeroed();
        GetLocalTime(&mut t);
        t
    };
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond)
}
