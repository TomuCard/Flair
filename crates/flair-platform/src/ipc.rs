//! Single instance and inter-process messages (`flair --toggle`, `--stats`).
//!
//! A local socket (named pipe on Windows, abstract Unix socket on Linux) is
//! held by the running instance. A new `flair` process first tries to connect:
//! if it succeeds, it sends its command and exits; otherwise it becomes the
//! instance. The server thread blocks on `accept`: no polling.
//!
//! Protocol: one command line from the client, one reply line from the server.

use std::io::{self, BufRead, BufReader, Write};

use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{GenericNamespaced, ListenerOptions, Stream};

/// Commands understood by the running instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Show,
    Toggle,
    Stats,
    Quit,
    /// Show the uninstallation screen (Windows "Installed apps").
    Uninstall,
    /// Re-read the configuration (after `flair --import`).
    Reload,
    /// Open a `.flair` file (double-click): show its import summary.
    Open(String),
}

impl Command {
    fn to_line(&self) -> String {
        match self {
            Self::Show => "show".into(),
            Self::Toggle => "toggle".into(),
            Self::Stats => "stats".into(),
            Self::Quit => "quit".into(),
            Self::Uninstall => "uninstall".into(),
            Self::Reload => "reload".into(),
            Self::Open(path) => format!("open {path}"),
        }
    }

    fn parse(line: &str) -> Option<Self> {
        let line = line.trim_end_matches(['\r', '\n']);
        Some(match line {
            "show" => Self::Show,
            "toggle" => Self::Toggle,
            "stats" => Self::Stats,
            "quit" => Self::Quit,
            "uninstall" => Self::Uninstall,
            "reload" => Self::Reload,
            other => Self::Open(other.strip_prefix("open ")?.to_owned()),
        })
    }
}

/// Socket name, per user (the namespace is shared by all users of the machine).
fn socket_name() -> String {
    let user = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_default();
    let user: String = user
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    format!("flair-{user}.sock")
}

/// Sends a command to the running instance and returns its reply.
/// Fails (`NotFound` / `ConnectionRefused`) when no instance is running.
pub fn send(command: &Command) -> io::Result<String> {
    let name = socket_name().to_ns_name::<GenericNamespaced>()?;
    let stream = Stream::connect(name)?;
    // Let the running instance bring its window to the foreground (Windows
    // only lets the process that received the user input do it).
    crate::window::allow_foreground();
    let mut stream = BufReader::new(stream);
    stream
        .get_mut()
        .write_all(format!("{}\n", command.to_line()).as_bytes())?;
    let mut reply = String::new();
    stream.read_line(&mut reply)?;
    Ok(reply.trim_end().to_owned())
}

/// The listening side, held by the running instance.
pub struct Server {
    listener: interprocess::local_socket::Listener,
}

impl Server {
    /// Takes the instance lock. Fails with `AddrInUse` if another instance holds it.
    pub fn bind() -> io::Result<Self> {
        let name = socket_name().to_ns_name::<GenericNamespaced>()?;
        let listener = ListenerOptions::new().name(name).create_sync()?;
        Ok(Self { listener })
    }

    /// Serves commands on a dedicated thread. `handler` returns the reply line.
    pub fn spawn<F>(self, handler: F) -> io::Result<()>
    where
        F: Fn(Command) -> String + Send + 'static,
    {
        std::thread::Builder::new()
            .name("flair-ipc".into())
            .spawn(move || {
                for connection in self.listener.incoming() {
                    let connection = match connection {
                        Ok(c) => c,
                        Err(e) => {
                            tracing::warn!("IPC connection failed: {e}");
                            continue;
                        }
                    };
                    let mut reader = BufReader::new(connection);
                    let mut line = String::new();
                    if reader.read_line(&mut line).is_err() {
                        continue;
                    }
                    let reply = match Command::parse(&line) {
                        Some(command) => handler(command),
                        None => format!("error: unknown command {:?}", line.trim()),
                    };
                    let _ = reader.get_mut().write_all(format!("{reply}\n").as_bytes());
                }
            })
            .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_round_trip() {
        for c in [
            Command::Show,
            Command::Toggle,
            Command::Stats,
            Command::Quit,
            Command::Uninstall,
            Command::Reload,
            Command::Open(r"C:\Users\me\Mon export.flair".into()),
            Command::Open("/home/me/a b.flair".into()),
        ] {
            assert_eq!(Command::parse(&c.to_line()), Some(c));
        }
        assert_eq!(Command::parse("toggle\n"), Some(Command::Toggle));
        assert_eq!(Command::parse("rm -rf"), None);
    }

    #[test]
    fn socket_name_is_safe() {
        let name = socket_name();
        assert!(name.starts_with("flair-") && name.ends_with(".sock"));
        assert!(!name.contains('/') && !name.contains('\\'));
    }
}
