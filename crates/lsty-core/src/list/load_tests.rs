use std::{
    fs,
    time::{Duration, Instant},
};

use super::{
    Item, load,
    test_util::{found, params},
};
use crate::{
    errs::{Code, Source},
    lock::{Cancel, Mode, tests::held_lock},
};

fn folder() -> (tempfile::TempDir, String) {
    let (tmp, real) = crate::fsio::test_util::dir();
    (tmp, real.to_string_lossy().into_owned())
}

#[test]
fn missing_done_file_is_empty() {
    let (_tmp, dir) = folder();
    let found = found(&dir);
    fs::write(&found.todo, "A\n").expect("write");
    let snap = load(&found, Duration::from_secs(1), &Cancel::new(), &params())
        .expect("load");
    assert!(snap.done().lines.is_empty(), "08.28");
    assert_eq!(snap.items().len(), 1);
}

#[test]
fn missing_todo_file_is_empty() {
    let (_tmp, dir) = folder();
    let found = found(&dir);
    fs::write(&found.done, "x 2026-09-28 A\n").expect("write");
    let snap = load(&found, Duration::from_secs(1), &Cancel::new(), &params())
        .expect("load");
    assert_eq!(snap.todo().lines.len(), 0);
    assert_eq!(snap.items().first().map(Item::source), Some(Source::Done));
}

#[test]
fn both_files_are_read() {
    let (_tmp, dir) = folder();
    let found = found(&dir);
    fs::write(&found.todo, "A id:k\n\nB dep:k\n").expect("write");
    fs::write(&found.done, "x 2026-09-28 C id:k\n").expect("write");
    let snap = load(&found, Duration::from_secs(1), &Cancel::new(), &params())
        .expect("load");
    assert_eq!(snap.items().len(), 3);
    assert_eq!(snap.found(), &found);
    assert!(snap.items().get(1).is_some_and(Item::blocked));
    assert_eq!(snap.warnings().len(), 1);
}

#[test]
fn lock_timeout_while_another_holds_the_lock() {
    let (_tmp, dir) = folder();
    let found = found(&dir);
    let _held = held_lock(&found.todo, Mode::Exclusive);
    let start = Instant::now();
    let err = load(
        &found,
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
    let found = found(&dir);
    let _held = held_lock(&found.todo, Mode::Shared);
    let ok = load(
        &found,
        Duration::from_millis(200),
        &Cancel::new(),
        &params(),
    );
    assert!(ok.is_ok());
}

#[test]
fn the_lock_is_released_after_loading() {
    let (_tmp, dir) = folder();
    let found = found(&dir);
    load(&found, Duration::from_secs(1), &Cancel::new(), &params())
        .expect("load");
    drop(held_lock(&found.todo, Mode::Exclusive));
}
