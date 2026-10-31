//! One copy of the app, and `dictation --toggle` / `--quit`.
//!
//! The running app listens on a Unix socket. A second start finds the
//! socket taken and exits; `--toggle` and `--quit` send one line to it.
//! The socket is in $XDG_RUNTIME_DIR, which only the user can open.

use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::mpsc::Sender;
use std::thread;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    Toggle,
    Quit,
}

impl Command {
    fn line(self) -> &'static str {
        match self {
            Command::Toggle => "toggle",
            Command::Quit => "quit",
        }
    }

    fn parse(line: &str) -> Option<Command> {
        match line.trim() {
            "toggle" => Some(Command::Toggle),
            "quit" => Some(Command::Quit),
            _ => None,
        }
    }
}

/// Take the socket. `None`: another copy is running. A socket file that
/// nobody answers on is left over from a crash, and is replaced.
pub fn listen(path: &Path) -> std::io::Result<Option<UnixListener>> {
    match UnixListener::bind(path) {
        Ok(listener) => Ok(Some(listener)),
        Err(e) if e.kind() == ErrorKind::AddrInUse => {
            if UnixStream::connect(path).is_ok() {
                return Ok(None);
            }
            std::fs::remove_file(path)?;
            UnixListener::bind(path).map(Some)
        }
        Err(e) => Err(e),
    }
}

/// Give each command to `on_command`, on a thread of its own.
pub fn serve(listener: UnixListener, on_command: impl Fn(Command) + Send + 'static) {
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut line = String::new();
            if BufReader::new(stream).read_line(&mut line).is_ok() {
                if let Some(command) = Command::parse(&line) {
                    on_command(command);
                }
            }
        }
    });
}

/// Send a command to the running copy.
pub fn send(path: &Path, command: Command) -> std::io::Result<()> {
    let mut stream = UnixStream::connect(path)?;
    writeln!(stream, "{}", command.line())
}

/// Forward each command as an event, for the main loop.
pub fn forward<E: Send + 'static>(listener: UnixListener, events: Sender<E>, event: fn(Command) -> E) {
    serve(listener, move |command| {
        let _ = events.send(event(command));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::mpsc;
    use std::time::Duration;

    /// A fresh socket path in the temp folder, per test.
    fn socket_path() -> PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("dictation-test-{}-{n}.sock", std::process::id()));
        let _ = std::fs::remove_file(&path);
        path
    }

    #[test]
    fn commands_round_trip() {
        for command in [Command::Toggle, Command::Quit] {
            assert_eq!(Command::parse(command.line()), Some(command));
        }
        assert_eq!(Command::parse("toggle\n"), Some(Command::Toggle));
        assert_eq!(Command::parse("dance"), None);
    }

    #[test]
    fn a_second_copy_finds_the_first() {
        let path = socket_path();
        let first = listen(&path).unwrap();
        assert!(first.is_some());
        assert!(listen(&path).unwrap().is_none());
        drop(first);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_stale_socket_is_replaced() {
        let path = socket_path();
        drop(UnixListener::bind(&path).unwrap()); // the file stays, nobody listens
        assert!(listen(&path).unwrap().is_some());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn send_reaches_the_running_copy() {
        let path = socket_path();
        let (tx, rx) = mpsc::channel();
        serve(listen(&path).unwrap().unwrap(), move |c| tx.send(c).unwrap());
        send(&path, Command::Toggle).unwrap();
        send(&path, Command::Quit).unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), Command::Toggle);
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), Command::Quit);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn send_without_a_running_copy_fails() {
        assert!(send(&socket_path(), Command::Toggle).is_err());
    }
}
