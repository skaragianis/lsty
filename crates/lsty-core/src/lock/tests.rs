use std::cell::RefCell;
use std::fs::{self, File};
use std::io;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use rustix::io::Errno;

use super::*;
use crate::errs::Code;

const ONE_SECOND: Duration = Duration::from_secs(1);

fn root(dirs: &[&str]) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("temp dir");
    let real = fs::canonicalize(tmp.path()).expect("canonicalize");
    for dir in dirs {
        fs::create_dir(real.join(dir)).expect("mkdir");
    }
    (tmp, real)
}

fn want(file: &Path, mode: Mode) -> Want {
    Want {
        file: file.to_path_buf(),
        mode,
    }
}

fn take(file: &Path, mode: Mode, timeout: Duration) -> errs::Result<Held> {
    acquire(&[want(file, mode)], timeout, &Cancel::new())
}

fn names(dir: &Path) -> Vec<String> {
    let mut found: Vec<String> = fs::read_dir(dir)
        .expect("read dir")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    found.sort();
    found
}

fn ino(path: &Path) -> u64 {
    fs::metadata(path).expect("stat").ino()
}

/// Refuses to lock a folder with `refusal` and locks anything else, and
/// records the inode of every file it was asked to lock.
struct Refuse {
    refusal: Errno,
    asked: RefCell<Vec<u64>>,
}

impl Refuse {
    fn new(refusal: Errno) -> Self {
        Self {
            refusal,
            asked: RefCell::new(Vec::new()),
        }
    }
}

impl Flock for Refuse {
    fn try_lock(&self, file: &File, mode: Mode) -> io::Result<()> {
        let meta = file.metadata()?;
        self.asked.borrow_mut().push(meta.ino());
        if meta.is_dir() {
            return Err(io::Error::from_raw_os_error(
                self.refusal.raw_os_error(),
            ));
        }
        OsFlock.try_lock(file, mode)
    }
}

/// Records the inode of every file it was asked to lock.
#[derive(Default)]
struct Record(RefCell<Vec<(u64, Mode)>>);

impl Flock for Record {
    fn try_lock(&self, file: &File, mode: Mode) -> io::Result<()> {
        self.0.borrow_mut().push((file.metadata()?.ino(), mode));
        OsFlock.try_lock(file, mode)
    }
}

#[test]
fn locks_in_path_order_and_a_folder_once() {
    // 05.10
    let (_tmp, root) = root(&["a", "b"]);
    let record = Record::default();
    let wants = [
        want(&root.join("b/todo.txt"), Mode::Shared),
        want(&root.join("a/todo.txt"), Mode::Shared),
        want(&root.join("a/done.txt"), Mode::Exclusive),
    ];
    let held = acquire_with(&record, &wants, ONE_SECOND, &Cancel::new());
    assert!(held.is_ok());
    let want_order = [
        (ino(&root.join("a")), Mode::Exclusive),
        (ino(&root.join("b")), Mode::Shared),
    ];
    assert_eq!(*record.0.borrow(), want_order);
}

#[test]
fn a_folder_with_any_exclusive_want_is_exclusive() {
    // 05.11
    let (_tmp, root) = root(&["a", "b"]);
    let record = Record::default();
    let wants = [
        want(&root.join("a/todo.txt"), Mode::Shared),
        want(&root.join("a/done.txt"), Mode::Exclusive),
        want(&root.join("b/todo.txt"), Mode::Shared),
    ];
    let held = acquire_with(&record, &wants, ONE_SECOND, &Cancel::new())
        .expect("held");
    let modes: Vec<Mode> = record.0.borrow().iter().map(|r| r.1).collect();
    assert_eq!(modes, [Mode::Exclusive, Mode::Shared]);
    assert!(held.exclusive(&root.join("a/todo.txt")));
    assert!(held.exclusive(&root.join("a/done.txt")));
    assert!(!held.exclusive(&root.join("b/todo.txt")));
    assert!(!held.exclusive(&root.join("c/todo.txt")));
}

#[test]
fn a_busy_folder_is_a_lock_timeout() {
    // 05.13
    let (_tmp, root) = root(&["d"]);
    let file = root.join("d/todo.txt");
    let _held = take(&file, Mode::Exclusive, ONE_SECOND).expect("held");
    let err = take(&file, Mode::Exclusive, Duration::from_millis(50))
        .expect_err("busy");
    assert_eq!(err.code(), Code::LockTimeout);
    assert_eq!(err.exit(), 4);
    assert_eq!(err.message(), "todo.txt is busy (locked by another writer)");
}

#[test]
fn the_timeout_is_for_the_whole_list() {
    let (_tmp, root) = root(&["a", "b"]);
    let _a = take(&root.join("a/x"), Mode::Exclusive, ONE_SECOND);
    let _b = take(&root.join("b/x"), Mode::Exclusive, ONE_SECOND);
    let held = [root.join("a/x"), root.join("b/x")];
    let wants: Vec<Want> =
        held.iter().map(|f| want(f, Mode::Shared)).collect();
    let err = acquire(&wants, Duration::from_millis(100), &Cancel::new())
        .expect_err("busy");
    assert_eq!(err.code(), Code::LockTimeout);
}

#[test]
fn cancelling_ends_the_wait_at_once() {
    // 05.14
    let (_tmp, root) = root(&["d"]);
    let file = root.join("d/todo.txt");
    let _held = take(&file, Mode::Exclusive, ONE_SECOND).expect("held");
    let cancel = Cancel::new();
    let trigger = cancel.clone();
    let handle = thread::spawn(move || {
        thread::sleep(Duration::from_millis(20));
        trigger.cancel();
    });
    let err = acquire(
        &[want(&file, Mode::Exclusive)],
        Duration::from_secs(10),
        &cancel,
    )
    .expect_err("cancelled");
    assert!(handle.join().is_ok());
    assert_eq!(err.code(), Code::Io);
    assert_eq!(err.message(), "context canceled");
    assert!(cancel.is_cancelled());
}

#[test]
fn a_cancelled_token_fails_before_locking() {
    let (_tmp, root) = root(&["d"]);
    let cancel = Cancel::new();
    cancel.cancel();
    let err = acquire(
        &[want(&root.join("d/todo.txt"), Mode::Shared)],
        ONE_SECOND,
        &cancel,
    )
    .expect_err("cancelled");
    assert_eq!(err.message(), "context canceled");
}

#[test]
fn no_file_is_created() {
    // 05.15
    let (_tmp, root) = root(&["d"]);
    let mut held = take(&root.join("d/todo.txt"), Mode::Exclusive, ONE_SECOND)
        .expect("held");
    assert!(held.release().is_ok());
    assert_eq!(names(&root.join("d")), Vec::<String>::new());
}

#[test]
fn an_unsupported_folder_lock_uses_a_sidecar() {
    // 05.16
    let (_tmp, root) = root(&["d"]);
    let file = root.join("d/todo.txt");
    let refuse = Refuse::new(Errno::NOTSUP);
    let held = acquire_with(
        &refuse,
        &[want(&file, Mode::Exclusive)],
        ONE_SECOND,
        &Cancel::new(),
    )
    .expect("held");
    let sidecar = root.join("d/.todo.txt.lock");
    assert!(sidecar.exists());
    assert!(held.exclusive(&file));
    let other = acquire_with(
        &refuse,
        &[want(&file, Mode::Shared)],
        Duration::from_millis(30),
        &Cancel::new(),
    )
    .expect_err("busy");
    assert_eq!(other.code(), Code::LockTimeout);
}

const REFUSALS: [Errno; 4] =
    [Errno::INVAL, Errno::NOTSUP, Errno::OPNOTSUPP, Errno::NOLCK];

#[test]
fn every_unsupported_error_falls_back() {
    for errno in REFUSALS {
        let (_tmp, root) = root(&["d"]);
        let refuse = Refuse::new(errno);
        let held = acquire_with(
            &refuse,
            &[want(&root.join("d/todo.txt"), Mode::Shared)],
            ONE_SECOND,
            &Cancel::new(),
        );
        assert!(held.is_ok(), "errno {errno:?}");
        assert!(root.join("d/.todo.txt.lock").exists());
    }
}

#[test]
fn another_flock_error_is_an_io_error_without_a_sidecar() {
    // 05.18
    let (_tmp, root) = root(&["d"]);
    let refuse = Refuse::new(Errno::ACCESS);
    let err = acquire_with(
        &refuse,
        &[want(&root.join("d/todo.txt"), Mode::Exclusive)],
        ONE_SECOND,
        &Cancel::new(),
    )
    .expect_err("refused");
    assert_eq!(err.code(), Code::Io);
    let dir = root.join("d");
    let expected = format!("lock {}: permission denied", dir.display());
    assert_eq!(err.message(), expected);
    assert_eq!(names(&root.join("d")), Vec::<String>::new());
}

#[test]
fn release_twice_is_harmless() {
    // 05.19
    let (_tmp, root) = root(&["d"]);
    let mut held = take(&root.join("d/todo.txt"), Mode::Exclusive, ONE_SECOND)
        .expect("held");
    assert!(held.release().is_ok());
    assert!(held.release().is_ok());
}

#[test]
fn dropping_releases() {
    let (_tmp, root) = root(&["d"]);
    let file = root.join("d/todo.txt");
    drop(take(&file, Mode::Exclusive, ONE_SECOND).expect("held"));
    assert!(take(&file, Mode::Exclusive, Duration::ZERO).is_ok());
}

#[test]
fn a_missing_folder_is_an_io_error() {
    // 05.21
    let (_tmp, root) = root(&[]);
    let file = root.join("nope/todo.txt");
    let err = take(&file, Mode::Shared, ONE_SECOND).expect_err("missing");
    let dir = root.join("nope");
    assert_eq!(err.code(), Code::Io);
    assert_eq!(
        err.message(),
        format!(
            "lock {0}: open {0}: no such file or directory",
            dir.display()
        )
    );
}

#[test]
fn sidecars_lock_each_file_in_sidecar_order() {
    // Two files in one refusing folder have separate sidecars; "a-b" sorts
    // before "a" once the dot and the suffix are added.
    let rows = [
        ("done.txt", "todo.txt", [".done.txt.lock", ".todo.txt.lock"]),
        ("a", "a-b", [".a-b.lock", ".a.lock"]),
    ];
    for (shared, exclusive, order) in rows {
        let (_tmp, root) = root(&["d"]);
        let shared = root.join("d").join(shared);
        let exclusive = root.join("d").join(exclusive);
        let refuse = Refuse::new(Errno::NOTSUP);
        let wants = [
            want(&exclusive, Mode::Exclusive),
            want(&shared, Mode::Shared),
        ];
        let held = acquire_with(&refuse, &wants, ONE_SECOND, &Cancel::new())
            .expect("held");
        let dir = ino(&root.join("d"));
        let asked: Vec<u64> = refuse
            .asked
            .borrow()
            .iter()
            .copied()
            .filter(|i| *i != dir)
            .collect();
        let expected: Vec<u64> =
            order.iter().map(|n| ino(&root.join("d").join(n))).collect();
        assert_eq!(asked, expected);
        assert!(!held.exclusive(&shared));
        assert!(held.exclusive(&exclusive));
    }
}

/// Whether the user running the tests ignores file modes.
fn ignores_modes(dir: &Path) -> bool {
    let probe = dir.join("probe");
    fs::create_dir(&probe).expect("mkdir");
    fs::set_permissions(&probe, fs::Permissions::from_mode(0o555))
        .expect("chmod");
    let writes = File::create(probe.join("x")).is_ok();
    fs::set_permissions(&probe, fs::Permissions::from_mode(0o755))
        .expect("chmod");
    writes
}

#[test]
fn a_read_only_sidecar_still_locks() {
    let (_tmp, root) = root(&["d"]);
    if ignores_modes(&root) {
        return;
    }
    let file = root.join("d/todo.txt");
    let side = root.join("d/.todo.txt.lock");
    fs::write(&side, "").expect("write");
    fs::set_permissions(&side, fs::Permissions::from_mode(0o444))
        .expect("chmod");
    let refuse = Refuse::new(Errno::NOTSUP);
    let held = acquire_with(
        &refuse,
        &[want(&file, Mode::Exclusive)],
        ONE_SECOND,
        &Cancel::new(),
    );
    assert!(held.is_ok());
}

#[test]
fn a_read_only_folder_cannot_hold_a_sidecar() {
    let (_tmp, root) = root(&["d"]);
    if ignores_modes(&root) {
        return;
    }
    let dir = root.join("d");
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o555))
        .expect("chmod");
    let refuse = Refuse::new(Errno::NOTSUP);
    let err = acquire_with(
        &refuse,
        &[want(&dir.join("todo.txt"), Mode::Exclusive)],
        ONE_SECOND,
        &Cancel::new(),
    )
    .expect_err("read-only");
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755))
        .expect("chmod");
    assert_eq!(err.code(), Code::Io);
}

#[test]
fn a_new_sidecar_follows_the_umask() {
    let (_tmp, root) = root(&["d"]);
    let refuse = Refuse::new(Errno::NOTSUP);
    let file = root.join("d/todo.txt");
    let _held = acquire_with(
        &refuse,
        &[want(&file, Mode::Shared)],
        ONE_SECOND,
        &Cancel::new(),
    )
    .expect("held");
    let mode = fs::metadata(root.join("d/.todo.txt.lock"))
        .expect("stat")
        .permissions()
        .mode();
    assert_eq!(mode & 0o111, 0);
    assert_eq!(mode & 0o600, 0o600);
}

#[test]
fn a_timeout_too_long_for_an_instant_does_not_panic() {
    let (_tmp, root) = root(&["d"]);
    let held = take(&root.join("d/todo.txt"), Mode::Shared, Duration::MAX);
    assert!(held.is_ok());
}

#[test]
fn a_sidecar_that_is_a_folder_is_an_io_error() {
    let (_tmp, root) = root(&["d"]);
    let side = root.join("d/.todo.txt.lock");
    fs::create_dir(&side).expect("mkdir");
    let refuse = Refuse::new(Errno::NOTSUP);
    let err = acquire_with(
        &refuse,
        &[want(&root.join("d/todo.txt"), Mode::Exclusive)],
        ONE_SECOND,
        &Cancel::new(),
    )
    .expect_err("folder");
    let expected =
        format!("lock {0}: open {0}: is a directory", side.display());
    assert_eq!(err.message(), expected);
}

#[test]
fn dropping_a_guard_unlocks_even_when_the_descriptor_lives_on() {
    let (_tmp, root) = root(&["d"]);
    let file = root.join("d/todo.txt");
    let held = take(&file, Mode::Exclusive, ONE_SECOND).expect("held");
    let copy = held.locks[0]
        .0
        .try_clone()
        .expect("a copy of the descriptor");
    drop(held);
    take(&file, Mode::Exclusive, Duration::from_millis(30)).expect("free");
    drop(copy);
}
