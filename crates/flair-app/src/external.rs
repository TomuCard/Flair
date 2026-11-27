//! Events coming from outside the window: global shortcut, tray menu, other
//! `flair` processes (IPC). They are produced on background threads and
//! delivered to Iced through a single channel; the subscription sleeps until
//! something arrives (no polling, no timer).

use std::sync::{Mutex, OnceLock};

use flair_platform::tray::TrayAction;
use iced::Subscription;
use iced::futures::channel::mpsc::{self, UnboundedReceiver, UnboundedSender};
use iced::futures::stream::{self, BoxStream, StreamExt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum External {
    Show,
    Toggle,
    Quit,
    Uninstall,
    Reload,
    Open(std::path::PathBuf),
    Tray(TrayAction),
    /// The system switched to dark (`true`) or light.
    SystemTheme(bool),
    /// Time to give memory back, if Flair is still hidden since hide `n`.
    Release(u64),
}

struct Channel {
    sender: UnboundedSender<External>,
    receiver: Mutex<Option<UnboundedReceiver<External>>>,
}

static CHANNEL: OnceLock<Channel> = OnceLock::new();

fn channel() -> &'static Channel {
    CHANNEL.get_or_init(|| {
        let (sender, receiver) = mpsc::unbounded();
        Channel {
            sender,
            receiver: Mutex::new(Some(receiver)),
        }
    })
}

/// Sends an event to the UI, from any thread. Counts as a wake-up.
pub fn send(event: External) {
    crate::stats::count_wakeup();
    let _ = channel().sender.unbounded_send(event);
}

fn stream() -> BoxStream<'static, External> {
    let receiver = channel().receiver.lock().ok().and_then(|mut r| r.take());
    match receiver {
        Some(receiver) => receiver.boxed(),
        // Already taken (the subscription is created once): nothing more.
        None => stream::pending().boxed(),
    }
}

/// Subscription delivering external events to `update`.
pub fn subscription() -> Subscription<External> {
    Subscription::run(stream)
}
