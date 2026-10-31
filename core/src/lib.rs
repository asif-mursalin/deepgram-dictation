//! The code that the Windows app (`rust/`) and the Linux app (`linux/`)
//! share. Nothing here calls Windows or Linux (SPEC.md).

pub mod art;
pub mod audio;
pub mod deepgram;
pub mod env;
pub mod gfx;
pub mod logging;
pub mod words;

pub use logging::log;
