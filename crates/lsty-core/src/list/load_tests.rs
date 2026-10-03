use super::Item;
use std::fs;
use std::time::{Duration, Instant};

use super::load;
use super::test_util::{found, params};
use crate::errs::Code;
use crate::errs::Source;
use crate::lock::{self, Cancel, Mode, Want};

fn folder() -> (tempfile::TempDir, String) {
    let tmp = tempfile::tempdir().expect("temp dir");
    let real = fs::canonicalize(tmp.path()).expect("canonicalize");
    (tmp, real.to_string_lossy().into_owned())
}

#[test]
fn missing_done_file_is_empty() {
    let (_tmp, dir) = folder();
    let files = found(&dir);
    fs::write(&files.todo, "A\n").expect("write");
    let snap = load(&files, Duration::from_secs(1), &Cancel::new(), &params())
        .expect("load");
    assert!(snap.done().lines.is_empty(), "08.28");
    assert_eq!(snap.items().len(), 1);
}

#[test]
fn missing_todo_file_is_empty() {
    let (_tmp, dir) = folder();
    let files = found(&dir);
    fs::write(&files.done, "x 2026-09-28 A\n").expect("write");
    let snap = load(&files, Duration::from_secs(1), &Cancel::new(), &params())
        .expect("load");
    assert_eq!(snap.todo().lines.len(), 0);
    assert_eq!(snap.items().first().map(Item::source), Some(Source::Done));
}

#[test]
fn both_files_are_read() {
    let (_tmp, dir) = folder();
    let files = found(&dir);
    fs::write(&files.todo, "A id:k\n\nB dep:k\n").expect("write");
    fs::write(&files.done, "x 2026-09-28 C id:k\n").expect("write");
    let snap = load(&files, Duration::from_secs(1), &Cancel::new(), &params())
        .expect("load");
    assert_eq!(snap.items().len(), 3);
    assert_eq!(snap.found(), &files);
    assert!(snap.items().get(1).is_some_and(Item::blocked));
    assert_eq!(snap.warnings().len(), 1);
}

#[test]
fn lock_timeout_while_another_holds_the_lock() {
    let (_tmp, dir) = folder();
    let files = found(&dir);
    let want = Want {
        file: files.todo.clone(),
        mode: Mode::Exclusive,
    };
    let _held = lock::acquire(&[want], Duration::from_secs(1), &Cancel::new())
        .expect("lock");
    let start = Instant::now();
    let err = load(
        &files,
        Duration::from_millis(200),
        &Cancel::new(),
        &params(),
    )
    .expect_err("busy");
    assert_eq!(err.code(), Code::LockTimeout, "08.29");
    assert!(start.elapsed() >= Duration::from_millis(150));
}

#[test]
fn shared_readers_do_not_block_each_other() {
    let (_tmp, dir) = folder();
    let files = found(&dir);
    let want = Want {
        file: files.todo.clone(),
        mode: Mode::Shared,
    };
    let _held = lock::acquire(&[want], Duration::from_secs(1), &Cancel::new())
        .expect("lock");
    let ok = load(
        &files,
        Duration::from_millis(200),
        &Cancel::new(),
        &params(),
    );
    assert!(ok.is_ok());
}

#[test]
fn the_lock_is_released_after_loading() {
    let (_tmp, dir) = folder();
    let files = found(&dir);
    load(&files, Duration::from_secs(1), &Cancel::new(), &params())
        .expect("load");
    let want = Want {
        file: files.todo,
        mode: Mode::Exclusive,
    };
    let held =
        lock::acquire(&[want], Duration::from_millis(200), &Cancel::new());
    assert!(held.is_ok());
}
