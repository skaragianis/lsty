//! Being asked to stop: SIGHUP and SIGTERM.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::{self, JoinHandle};

use lsty_core::lock::Cancel;
use signal_hook::consts::{SIGHUP, SIGTERM};
use signal_hook::flag;
use signal_hook::iterator::{Handle, Signals};

use crate::run::{Feed, Input};

/// The signals that end the program like a quit does.
const STOPPING: [i32; 2] = [SIGHUP, SIGTERM];

/// Whether the signals end the process as they do by default: no watch
/// is alive. The handlers of a dropped watch stay installed and would
/// swallow them for good.
pub(crate) fn default_action() -> &'static AtomicBool {
    static DEFAULT: OnceLock<Arc<AtomicBool>> = OnceLock::new();
    DEFAULT.get_or_init(|| {
        let flag = Arc::new(AtomicBool::new(true));
        for signal in STOPPING {
            // Without it a signal after the watch is ignored; nothing else
            // can be done about a failure.
            drop(flag::register_conditional_default(
                signal,
                Arc::clone(&flag),
            ));
        }
        flag
    })
}

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
        let default = default_action();
        let signals = Signals::new(STOPPING)?;
        default.store(false, Ordering::SeqCst);
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
        // The default first: a signal in between reaches both.
        default_action().store(true, Ordering::SeqCst);
        self.handle.close();
        if let Some(thread) = self.thread.take() {
            // The thread has nothing to report.
            drop(thread.join());
        }
    }
}
