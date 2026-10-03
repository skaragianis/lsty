//! Polling for a lock until it is free, the deadline passes or the wait
//! is cancelled.

use std::fs::File;
use std::io;
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use super::{Flock, Mode};
use crate::errs;

const FIRST_PAUSE: Duration = Duration::from_millis(5);
const MAX_PAUSE: Duration = Duration::from_millis(50);

/// Stops a wait for a lock. Clones share one state: cancel any of them
/// and every wait that holds one ends at once.
#[derive(Clone, Debug, Default)]
pub struct Cancel(Arc<(Mutex<bool>, Condvar)>);

impl Cancel {
    /// A token that is not cancelled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Cancels every wait that holds this token.
    pub fn cancel(&self) {
        *self.state() = true;
        self.0.1.notify_all();
    }

    /// Whether [`cancel`](Self::cancel) was called.
    pub fn is_cancelled(&self) -> bool {
        *self.state()
    }

    /// Sleeps for `d` or until cancelled; true when cancelled.
    fn sleep(&self, d: Duration) -> bool {
        let guard = self.state();
        let (guard, _) = self
            .0
            .1
            .wait_timeout_while(guard, d, |cancelled| !*cancelled)
            .unwrap_or_else(PoisonError::into_inner);
        *guard
    }

    fn state(&self) -> MutexGuard<'_, bool> {
        self.0.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Why a lock was not taken.
pub(super) enum Failure {
    /// The deadline passed.
    Busy,
    /// The token was cancelled.
    Cancelled,
    /// The system call failed.
    Io(io::Error),
}

impl Failure {
    /// The error for a failure to lock `path`; `name` is the file whose
    /// folder could not be locked.
    pub(super) fn into_error(self, path: &Path, name: &str) -> errs::Error {
        match self {
            Self::Busy => errs::Error::new(
                errs::Code::LockTimeout,
                format!("{name} is busy (locked by another writer)"),
            ),
            Self::Cancelled => {
                errs::Error::new(errs::Code::Io, "context canceled")
            }
            Self::Io(e) => errs::io("lock", path, e),
        }
    }
}

/// The deadline and token shared by every lock of one list.
pub(super) struct Waiter<'a> {
    cancel: &'a Cancel,
    /// None when the timeout is too long to be an instant: wait forever.
    deadline: Option<Instant>,
}

impl<'a> Waiter<'a> {
    pub(super) fn new(cancel: &'a Cancel, deadline: Option<Instant>) -> Self {
        Self { cancel, deadline }
    }

    /// Takes one lock, trying again after a pause that starts at 5 ms and
    /// doubles up to 50 ms.
    pub(super) fn lock<F: Flock + ?Sized>(
        &self,
        flock: &F,
        file: &File,
        mode: Mode,
    ) -> Result<(), Failure> {
        let mut pause = FIRST_PAUSE;
        loop {
            if self.cancel.is_cancelled() {
                return Err(Failure::Cancelled);
            }
            match flock.try_lock(file, mode) {
                Ok(()) => return Ok(()),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(Failure::Io(e)),
            }
            let nap = self.nap(pause)?;
            if self.cancel.sleep(nap) {
                return Err(Failure::Cancelled);
            }
            pause = (pause * 2).min(MAX_PAUSE);
        }
    }

    /// How long to sleep before the next attempt, or `Busy` when the
    /// deadline has passed.
    fn nap(&self, pause: Duration) -> Result<Duration, Failure> {
        let Some(at) = self.deadline else {
            return Ok(pause);
        };
        let left = at.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(Failure::Busy);
        }
        Ok(pause.min(left))
    }
}
