//! Polling for a lock until it is free, the deadline passes or the wait
//! is cancelled.

use std::{
    fs::File,
    io,
    path::Path,
    sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

use super::{Flock, Mode};
use crate::errs;

const PAUSE: Duration = Duration::from_millis(2);

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

    /// Takes one lock, trying again every 2 ms. The pause is short so that
    /// a waiter is rarely asleep when the holder lets go: a longer one lets
    /// a writer that arrives in between take the lock again and again.
    pub(super) fn lock<F: Flock + ?Sized>(
        &self,
        flock: &F,
        file: &File,
        mode: Mode,
    ) -> Result<(), Failure> {
        loop {
            if self.cancel.is_cancelled() {
                return Err(Failure::Cancelled);
            }
            match flock.try_lock(file, mode) {
                Ok(()) => return Ok(()),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(Failure::Io(e)),
            }
            let nap = self.nap()?;
            if self.cancel.sleep(nap) {
                return Err(Failure::Cancelled);
            }
        }
    }

    /// How long to sleep before the next attempt, or `Busy` when the
    /// deadline has passed.
    fn nap(&self) -> Result<Duration, Failure> {
        let Some(at) = self.deadline else {
            return Ok(PAUSE);
        };
        let left = at.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(Failure::Busy);
        }
        Ok(PAUSE.min(left))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    /// A lock that is busy for six tries, free on the seventh, and notes
    /// when it was tried.
    #[derive(Default)]
    struct Busy(RefCell<Vec<Instant>>);

    impl Flock for Busy {
        fn try_lock(&self, _: &File, _: Mode) -> io::Result<()> {
            let mut tries = self.0.borrow_mut();
            tries.push(Instant::now());
            if tries.len() == 7 {
                return Ok(());
            }
            Err(io::ErrorKind::WouldBlock.into())
        }
    }

    #[test]
    fn a_sleep_lasts_until_the_time_or_the_cancel() {
        let token = Cancel::new();
        let start = Instant::now();
        assert!(!token.sleep(Duration::from_millis(40)));
        assert!(start.elapsed() >= Duration::from_millis(40));
        token.cancel();
        let start = Instant::now();
        assert!(token.sleep(Duration::from_secs(5)));
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn the_tries_are_a_pause_apart() {
        let file = tempfile::tempfile().expect("a file");
        let token = Cancel::new();
        let waiter = Waiter::new(&token, None);
        let busy = Busy::default();
        let result = waiter.lock(&busy, &file, Mode::Exclusive);
        assert!(result.is_ok());
        let tries = busy.0.borrow();
        let gaps: Vec<Duration> =
            tries.windows(2).map(|w| w[1] - w[0]).collect();
        assert_eq!(gaps.len(), 6);
        assert!(gaps.iter().all(|g| *g >= PAUSE), "{gaps:?}");
    }
}
