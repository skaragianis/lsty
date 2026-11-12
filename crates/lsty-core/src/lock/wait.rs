//! Waiting for a lock until it is free, the deadline passes or the wait
//! is cancelled.

use std::{
    fs::File,
    io,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError},
    },
    thread,
    time::{Duration, Instant},
};

use super::{Flock, Mode};
use crate::errs;

/// How often a wait looks at its deadline and its token.
const PAUSE: Duration = Duration::from_millis(2);

/// How many helper threads one process has at a time. A wait that gives up
/// leaves its helper behind until the holder lets go, so a holder that
/// hangs cannot make them pile up: past this many, a wait keeps trying
/// every [`PAUSE`] instead.
pub(super) const MOST_HELPERS: usize = 8;

/// What a helper sends when it has the lock, or has failed.
type Answer = Receiver<io::Result<()>>;

/// The helper threads alive in this process; a wait may count its own
/// instead (tests do).
pub(super) static HELPERS: AtomicUsize = AtomicUsize::new(0);

/// Stops a wait for a lock. Clones share one state: cancel any of them
/// and every wait that holds one ends.
#[derive(Clone, Debug, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    /// A token that is not cancelled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Cancels every wait that holds this token.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// Whether [`cancel`](Self::cancel) was called.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
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
    /// The count of helper threads this wait adds to.
    helpers: &'static AtomicUsize,
}

impl<'a> Waiter<'a> {
    pub(super) fn new(
        cancel: &'a Cancel,
        deadline: Option<Instant>,
        helpers: &'static AtomicUsize,
    ) -> Self {
        Self {
            cancel,
            deadline,
            helpers,
        }
    }

    /// Takes one lock. A lock that is busy is waited for in the system's
    /// queue, on a thread of its own, so that a waiter is not overtaken
    /// again and again by writers that keep trying. The descriptor is
    /// shared with the thread, so a wait that gives up leaves the thread
    /// to take the lock when it is free and let go of it by closing.
    pub(super) fn lock<F: Flock + ?Sized>(
        &self,
        flock: &F,
        file: &File,
        mode: Mode,
    ) -> Result<(), Failure> {
        if self.tried(flock, file, mode)? {
            return Ok(());
        }
        self.nap()?;
        match helper(self.helpers, file, mode) {
            Some(done) => self.wait_for(&done),
            None => self.poll(flock, file, mode),
        }
    }

    /// One try at the lock: whether it was taken.
    fn tried<F: Flock + ?Sized>(
        &self,
        flock: &F,
        file: &File,
        mode: Mode,
    ) -> Result<bool, Failure> {
        if self.cancel.is_cancelled() {
            return Err(Failure::Cancelled);
        }
        match flock.try_lock(file, mode) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(false),
            Err(e) => Err(Failure::Io(e)),
        }
    }

    /// Tries again every [`PAUSE`], for when no helper can be had.
    fn poll<F: Flock + ?Sized>(
        &self,
        flock: &F,
        file: &File,
        mode: Mode,
    ) -> Result<(), Failure> {
        loop {
            thread::sleep(self.nap()?);
            if self.tried(flock, file, mode)? {
                return Ok(());
            }
        }
    }

    /// Waits for the helper to take the lock.
    fn wait_for(&self, done: &Answer) -> Result<(), Failure> {
        loop {
            match done.recv_timeout(self.nap()?) {
                Ok(taken) => return taken.map_err(Failure::Io),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(Failure::Io(io::Error::other("lock thread")));
                }
            }
            if self.cancel.is_cancelled() {
                return Err(Failure::Cancelled);
            }
        }
    }

    /// How long to wait before the next look, or `Busy` when the deadline
    /// has passed.
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

/// Starts a helper that waits for the lock on `file`, unless the process
/// has [`MOST_HELPERS`] already or cannot start one (the wait then polls).
/// The answer comes on the channel.
fn helper(
    helpers: &'static AtomicUsize,
    file: &File,
    mode: Mode,
) -> Option<Answer> {
    let queued = file.try_clone().ok()?;
    let (taken, answer) = mpsc::channel();
    if helpers.fetch_add(1, Ordering::SeqCst) < MOST_HELPERS
        && thread::Builder::new()
            .spawn(move || {
                let result = block(&queued, mode);
                helpers.fetch_sub(1, Ordering::SeqCst);
                drop(taken.send(result));
            })
            .is_ok()
    {
        return Some(answer);
    }
    helpers.fetch_sub(1, Ordering::SeqCst);
    None
}

/// Waits for the lock on `file` as long as it takes.
fn block(file: &File, mode: Mode) -> io::Result<()> {
    loop {
        let locked = match mode {
            Mode::Shared => file.lock_shared(),
            Mode::Exclusive => file.lock(),
        };
        match locked {
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            done => return done,
        }
    }
}
