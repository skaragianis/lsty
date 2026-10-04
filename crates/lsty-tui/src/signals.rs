//! Being asked to stop: SIGHUP and SIGTERM.

use std::io;
use std::thread::{self, JoinHandle};

use lsty_core::lock::Cancel;
use signal_hook::consts::{SIGHUP, SIGTERM};
use signal_hook::iterator::{Handle, Signals};

use crate::run::{Feed, Input};

/// The signals that end the program like a quit does.
const STOPPING: [i32; 2] = [SIGHUP, SIGTERM];

/// A watch for the signals of [`STOPPING`], until it is dropped.
///
/// A signal also cancels [`Stop::cancel`], so that a wait for a lock
/// ends at once, which is what a second signal would be needed for.
pub(crate) struct Stop {
    signals: Option<Signals>,
    handle: Handle,
    cancel: Cancel,
    thread: Option<JoinHandle<()>>,
}

impl Stop {
    /// Starts to catch the signals; they are told once [`Stop::start`]
    /// has been called.
    pub(crate) fn new() -> io::Result<Self> {
        let signals = Signals::new(STOPPING)?;
        Ok(Self {
            handle: signals.handle(),
            signals: Some(signals),
            cancel: Cancel::new(),
            thread: None,
        })
    }

    /// Tells `feed` when a signal comes, from a thread that waits for
    /// them without polling.
    pub(crate) fn start(&mut self, feed: Feed) -> io::Result<()> {
        let Some(mut signals) = self.signals.take() else {
            return Ok(());
        };
        let cancel = self.cancel.clone();
        let thread = thread::Builder::new().name("signals".to_owned()).spawn(
            move || {
                for _ in signals.forever() {
                    cancel.cancel();
                    feed.send(Input::Stop);
                }
            },
        )?;
        self.thread = Some(thread);
        Ok(())
    }

    /// A token that a signal cancels.
    pub(crate) fn cancel(&self) -> Cancel {
        self.cancel.clone()
    }
}

impl Drop for Stop {
    fn drop(&mut self) {
        self.handle.close();
        if let Some(thread) = self.thread.take() {
            // The thread has nothing to report.
            drop(thread.join());
        }
    }
}
