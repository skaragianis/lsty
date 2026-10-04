//! Being asked to stop: SIGHUP and SIGTERM.

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use signal_hook::SigId;
use signal_hook::consts::{SIGHUP, SIGTERM};
use signal_hook::{flag, low_level};

/// The signals that end the program like a quit does.
const STOPPING: [i32; 2] = [SIGHUP, SIGTERM];

/// A watch for the signals of [`STOPPING`], until it is dropped.
pub(crate) struct Stop {
    asked: Arc<AtomicBool>,
    ids: Vec<SigId>,
}

impl Stop {
    /// Starts to watch.
    pub(crate) fn new() -> io::Result<Self> {
        let mut stop = Self {
            asked: Arc::new(AtomicBool::new(false)),
            ids: Vec::new(),
        };
        for signal in STOPPING {
            let id = flag::register(signal, Arc::clone(&stop.asked))?;
            stop.ids.push(id);
        }
        Ok(stop)
    }

    /// Whether one of the signals has arrived.
    pub(crate) fn asked(&self) -> bool {
        self.asked.load(Ordering::Relaxed)
    }
}

impl Drop for Stop {
    fn drop(&mut self) {
        for id in self.ids.drain(..) {
            low_level::unregister(id);
        }
    }
}
