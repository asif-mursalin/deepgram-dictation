//! The code that the Windows app (`rust/`) and the Linux app (`linux/`)
//! share. Nothing here calls Windows or Linux (SPEC.md).

pub mod art;
pub mod audio;
pub mod deepgram;
pub mod env;
pub mod gfx;
pub mod live;
pub mod logging;
pub mod session;
pub mod words;

pub use logging::log;

/// A window that can have the keyboard focus, as the app sees it. Windows
/// uses the HWND. The Linux app on GNOME cannot see which window has focus
/// and uses 0 ("unknown").
pub type WindowId = isize;
