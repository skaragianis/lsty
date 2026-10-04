use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use lsty_core::config::Config;
use lsty_core::discover::{self, Found, Input};
use notify::Event;
use notify::event::{
    AccessKind, CreateKind, EventKind, ModifyKind, RemoveKind, RenameMode,
};
use tempfile::TempDir;

use crate::test_util::{deps, workdir};
use crate::watch::{
    FolderWatcher, Notice, Sink, Targets, Watcher, forward, reason,
};

/// How long the system's watch gets to report something.
const PATIENCE: Duration = Duration::from_secs(10);
/// How long counts as nothing happening.
const QUIET: Duration = Duration::from_millis(300);

fn found_in(dir: &TempDir) -> Found {
    let dir = fs::canonicalize(dir.path()).unwrap_or_else(|e| panic!("{e}"));
    deps(&dir, Config::default()).found
}

fn watch(found: &Found) -> (FolderWatcher, Receiver<Notice>) {
    let (tx, rx) = channel();
    let sink: Sink = Arc::new(move |notice| drop(tx.send(notice)));
    let watcher =
        FolderWatcher::new(found, sink).unwrap_or_else(|e| panic!("{e}"));
    (watcher, rx)
}

/// Waits up to `wait` for an event and takes every one that is there.
fn event_within(rx: &Receiver<Notice>, wait: Duration) -> bool {
    if rx.recv_timeout(wait).is_err() {
        return false;
    }
    while rx.try_recv().is_ok() {}
    true
}

/// Waits until nothing has come for [`QUIET`].
fn settle(rx: &Receiver<Notice>) {
    while event_within(rx, QUIET) {}
}

/// Replaces `path` as another process would: a temporary file, renamed.
fn write_atomically(path: &Path, text: &str) {
    let mut staged = path.as_os_str().to_owned();
    staged.push(".tmp");
    fs::write(&staged, text).unwrap_or_else(|e| panic!("{e}"));
    fs::rename(&staged, path).unwrap_or_else(|e| panic!("{e}"));
}

// 33.1
#[test]
fn an_atomic_write_of_the_todo_file_is_an_event_each_time() {
    let dir = workdir("A\n");
    let found = found_in(&dir);
    let (_w, rx) = watch(&found);
    for text in ["B\n", "C\n", "D\n"] {
        settle(&rx);
        write_atomically(&found.todo, text);
        assert!(event_within(&rx, PATIENCE), "after writing {text:?}");
    }
}

// 33.2
#[test]
fn a_file_with_another_name_is_no_event() {
    let dir = workdir("A\n");
    let found = found_in(&dir);
    let (_w, rx) = watch(&found);
    settle(&rx);
    write_atomically(&found.todo.with_file_name("notes.txt"), "x\n");
    assert!(!event_within(&rx, QUIET));
}

// 33.3
#[test]
fn writing_the_done_file_is_an_event() {
    let dir = workdir("A\n");
    let found = found_in(&dir);
    let (_w, rx) = watch(&found);
    settle(&rx);
    write_atomically(&found.done, "x A\n");
    assert!(event_within(&rx, PATIENCE));
}

// 33.4
#[test]
fn a_symlinked_todo_file_is_watched_where_it_really_is() {
    let dir = workdir("");
    let root = fs::canonicalize(dir.path()).unwrap_or_else(|e| panic!("{e}"));
    fs::create_dir(root.join("real")).unwrap_or_else(|e| panic!("{e}"));
    fs::create_dir(root.join("links")).unwrap_or_else(|e| panic!("{e}"));
    fs::write(root.join("real/todo.txt"), "A\n")
        .unwrap_or_else(|e| panic!("{e}"));
    symlink(root.join("real/todo.txt"), root.join("links/todo.txt"))
        .unwrap_or_else(|e| panic!("{e}"));
    let vars = |_: &str| None;
    let found = discover::find(&Input {
        explicit: Some(&root.join("links/todo.txt")),
        vars: &vars,
        cwd: &root,
        home: Some(&root),
        default_file: None,
    })
    .unwrap_or_else(|e| panic!("{}", e.message()));
    assert_eq!(found.todo, root.join("real/todo.txt"));
    let (_w, rx) = watch(&found);
    settle(&rx);
    write_atomically(&root.join("real/todo.txt"), "B\n");
    assert!(event_within(&rx, PATIENCE));
}

#[test]
fn a_folder_that_is_made_again_is_watched_after_rewatch() {
    let dir = workdir("");
    let root = fs::canonicalize(dir.path()).unwrap_or_else(|e| panic!("{e}"));
    let sub = root.join("sub");
    fs::create_dir(&sub).unwrap_or_else(|e| panic!("{e}"));
    let vars = |_: &str| None;
    let found = discover::find(&Input {
        explicit: Some(&sub.join("todo.txt")),
        vars: &vars,
        cwd: &root,
        home: Some(&root),
        default_file: None,
    })
    .unwrap_or_else(|e| panic!("{}", e.message()));
    let (mut w, rx) = watch(&found);
    fs::remove_dir_all(&sub).unwrap_or_else(|e| panic!("{e}"));
    assert!(event_within(&rx, PATIENCE), "the folder going away");
    fs::create_dir(&sub).unwrap_or_else(|e| panic!("{e}"));
    w.rewatch().unwrap_or_else(|e| panic!("{e}"));
    settle(&rx);
    write_atomically(&found.todo, "A\n");
    assert!(event_within(&rx, PATIENCE));
}

#[test]
fn a_folder_that_is_missing_is_no_error() {
    let dir = workdir("");
    let root = fs::canonicalize(dir.path()).unwrap_or_else(|e| panic!("{e}"));
    let vars = |_: &str| None;
    let found = discover::find(&Input {
        explicit: Some(&root.join("absent/todo.txt")),
        vars: &vars,
        cwd: &root,
        home: Some(&root),
        default_file: None,
    })
    .unwrap_or_else(|e| panic!("{}", e.message()));
    let (mut w, _rx) = watch(&found);
    assert_eq!(w.rewatch(), Ok(()));
}

fn event(kind: EventKind, path: &Path) -> Event {
    Event::new(kind).add_path(path.to_path_buf())
}

#[test]
fn only_changes_to_the_files_and_the_loss_of_their_folder_pass() {
    let dir = workdir("");
    let found = found_in(&dir);
    let targets = Targets::of(&found);
    let folder = found
        .todo
        .parent()
        .map_or_else(PathBuf::new, Path::to_path_buf);
    let other = folder.join("notes.txt");
    let rename = ModifyKind::Name(RenameMode::To);
    let rows: [(EventKind, &Path, bool); 11] = [
        (EventKind::Create(CreateKind::File), &found.todo, true),
        (EventKind::Modify(ModifyKind::Any), &found.todo, true),
        (EventKind::Modify(rename), &found.todo, true),
        (EventKind::Remove(RemoveKind::File), &found.done, true),
        (EventKind::Modify(ModifyKind::Any), &found.done, true),
        (EventKind::Access(AccessKind::Any), &found.todo, false),
        (EventKind::Create(CreateKind::File), &other, false),
        (EventKind::Remove(RemoveKind::Folder), &folder, true),
        (EventKind::Modify(rename), &folder, true),
        (EventKind::Modify(ModifyKind::Any), &folder, false),
        (EventKind::Create(CreateKind::File), &folder, false),
    ];
    for (kind, path, want) in rows {
        let got = targets.passes(&event(kind, path));
        assert_eq!(got, want, "{kind:?} on {}", path.display());
    }
}

/// The text of a failure of the system's watch, as an I/O error is
/// worded elsewhere.
#[test]
fn failures_of_the_watch_are_worded_like_io_errors() {
    use notify::Error;
    let os = |n| Error::io(std::io::Error::from_raw_os_error(n));
    let rows: [(Error, &str); 7] = [
        (os(13), "permission denied"),
        (os(24), "too many open files"),
        (os(2), "no such file or directory"),
        (
            Error::new(notify::ErrorKind::MaxFilesWatch),
            "no space left on device",
        ),
        (Error::path_not_found(), "no such file or directory"),
        (Error::generic("a text"), "a text"),
        (Error::watch_not_found(), "watch not found"),
    ];
    for (err, want) in rows {
        assert_eq!(reason(&err), want);
    }
}

#[test]
fn an_overflow_of_the_queue_is_a_failure() {
    let dir = workdir("");
    let found = found_in(&dir);
    let targets = Targets::of(&found);
    let (tx, rx) = channel();
    let sink: Sink = Arc::new(move |notice| drop(tx.send(notice)));
    let overflow =
        Event::new(EventKind::Other).set_flag(notify::event::Flag::Rescan);
    forward(&targets, &sink, Ok(overflow));
    assert_eq!(
        rx.try_recv(),
        Ok(Notice::Failed("queue or buffer overflow".to_owned()))
    );
    forward(&targets, &sink, Err(notify::Error::generic("boom")));
    assert_eq!(rx.try_recv(), Ok(Notice::Failed("boom".to_owned())));
}

#[test]
fn a_folder_that_cannot_be_watched_says_why() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = workdir("");
    let root = fs::canonicalize(dir.path()).unwrap_or_else(|e| panic!("{e}"));
    let sub = root.join("sub");
    fs::create_dir(&sub).unwrap_or_else(|e| panic!("{e}"));
    fs::set_permissions(&sub, fs::Permissions::from_mode(0o311))
        .unwrap_or_else(|e| panic!("{e}"));
    let vars = |_: &str| None;
    let found = discover::find(&Input {
        explicit: Some(&sub.join("todo.txt")),
        vars: &vars,
        cwd: &root,
        home: Some(&root),
        default_file: None,
    })
    .unwrap_or_else(|e| panic!("{}", e.message()));
    let sink: Sink = Arc::new(|_| {});
    let made = FolderWatcher::new(&found, sink);
    fs::set_permissions(&sub, fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|e| panic!("{e}"));
    // A user who may read the folder anyway has nothing to fail.
    if let Err(why) = made {
        assert_eq!(why, format!("watch {}: permission denied", sub.display()));
    }
}
