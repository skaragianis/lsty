use std::{
    cell::Cell,
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    rc::Rc,
    time::Duration,
};

use lsty_core::{config::Config, errs::Source, lock::Cancel};

use crate::{
    Notice, Watcher,
    live::{self, DEBOUNCE},
    model::Model,
    msg::{Cmd, Msg},
    test_util::{
        alone, at, deps, held_lock, keys, load_of, press, reload, start,
        started_from, workdir, workdir_model,
    },
};

const LIST: &str = "A +p\nB +p\nC +p\n";
const NEXT: &str = "A +p\nB +p\nC +p\nD +p\n";

/// What a test sees of the watcher the model starts.
#[derive(Default)]
struct Feed {
    rewatched: Cell<usize>,
}

struct Scripted(Rc<Feed>);

impl Watcher for Scripted {
    fn rewatch(&mut self) -> Result<(), String> {
        self.0.rewatched.set(self.0.rewatched.get() + 1);
        Ok(())
    }
}

fn watched(dir: &std::path::Path, config: Config) -> (Model, Rc<Feed>) {
    let feed = Rc::new(Feed::default());
    let d = deps(dir, config);
    let mut s = start(dir);
    let shared = Rc::clone(&feed);
    s.watch = Some(Box::new(move |_, _| {
        Ok(Box::new(Scripted(Rc::clone(&shared))))
    }));
    (started_from(s, d), feed)
}

/// The change of a file, as the watch tells it.
fn changed(model: &mut Model) -> Vec<Cmd> {
    model.update(Msg::Notice(Notice::Changed))
}

/// The clock reaching a minute, with the reading it asks for done.
fn minute(model: &mut Model) {
    let msg = load_of(model.update(Msg::Minute)).read();
    model.update(msg);
}

fn debounce() -> Vec<Cmd> {
    vec![Cmd::After {
        delay: DEBOUNCE,
        msg: Msg::Reload,
    }]
}

// 33.6
#[test]
fn three_events_within_the_debounce_start_one_reload() {
    let dir = workdir(LIST);
    let (mut model, feed) = watched(dir.path(), alone());
    assert_eq!(changed(&mut model), debounce());
    assert_eq!(changed(&mut model), Vec::new());
    assert_eq!(changed(&mut model), Vec::new());
    assert_eq!(feed.rewatched.get(), 0);
    assert_eq!(reload(&mut model), Vec::new());
    assert_eq!(feed.rewatched.get(), 1, "the folders are added again");
    // The next event starts a new debounce.
    assert_eq!(changed(&mut model), debounce());
}

// 33.7
#[test]
fn an_event_during_a_reload_owes_one_more() {
    let dir = workdir(LIST);
    let (mut model, _) = watched(dir.path(), alone());
    assert_eq!(changed(&mut model), debounce());
    let load = load_of(model.update(Msg::Reload));
    assert_eq!(changed(&mut model), Vec::new(), "the files are being read");
    assert_eq!(model.update(load.read()), debounce());
    assert_eq!(reload(&mut model), Vec::new());
    assert_eq!(changed(&mut model), debounce());
}

#[test]
fn a_reading_that_began_before_a_change_is_dropped_and_owed() {
    let dir = workdir("A\nB\n");
    let (mut model, _) = watched(dir.path(), alone());
    // The files are read, then the model's own write changes what is
    // shown before the reading arrives.
    let stale = load_of(model.update(Msg::Reload)).read();
    press(&mut model, "x");
    let written = model.snap.todo().to_bytes();
    assert_eq!(written, b"x 2026-09-29 A\nB\n");
    assert_eq!(model.update(stale), debounce());
    assert_eq!(model.snap.todo().to_bytes(), written);
}

fn first_item(model: &Model) -> *const lsty_core::list::Item {
    model.snap.items().as_ptr()
}

// 33.28
#[test]
fn an_outside_write_within_the_debounce_of_the_models_own_is_shown() {
    let dir = workdir(LIST);
    let (mut model, _) = watched(dir.path(), alone());
    let path = dir.path().join("todo.txt");
    press(&mut model, "gp");
    changed(&mut model);
    let load = load_of(model.update(Msg::Reload));
    let outside = "A +p\nOUTSIDE +p\n";
    fs::write(&path, outside).unwrap_or_else(|e| panic!("{e}"));
    model.update(load.read());
    assert_eq!(model.snap.todo().to_bytes(), outside.as_bytes());
}

#[test]
fn a_reload_that_finds_the_same_content_changes_nothing() {
    let dir = workdir(LIST);
    let (mut model, _) = watched(dir.path(), alone());
    let before = first_item(&model);
    // Written again with the same bytes, as `touch` or a save would.
    fs::write(dir.path().join("todo.txt"), LIST)
        .unwrap_or_else(|e| panic!("{e}"));
    changed(&mut model);
    reload(&mut model);
    assert!(std::ptr::eq(first_item(&model), before));
}

// 33.27
#[test]
fn an_own_write_read_back_builds_nothing() {
    let dir = workdir(LIST);
    let (mut model, _) = watched(dir.path(), alone());
    press(&mut model, "gp");
    let wrote = model.snap.todo().to_bytes();
    assert_ne!(wrote, LIST.as_bytes(), "the key wrote");
    let before = first_item(&model);
    changed(&mut model);
    reload(&mut model);
    assert!(
        std::ptr::eq(first_item(&model), before),
        "nothing was built"
    );
    assert_eq!(model.snap.todo().to_bytes(), wrote);
}

#[test]
fn a_change_to_the_done_file_alone_is_shown() {
    let dir = workdir(LIST);
    let done = dir.path().join("done.txt");
    fs::write(&done, "x old\n").unwrap_or_else(|e| panic!("{e}"));
    let (mut model, _) = watched(dir.path(), alone());
    // One byte more in the done file, the todo file as it was.
    fs::write(&done, "x old\r\n").unwrap_or_else(|e| panic!("{e}"));
    changed(&mut model);
    reload(&mut model);
    assert_eq!(model.snap.done().to_bytes(), b"x old\r\n");
    assert_eq!(model.snap.todo().to_bytes(), LIST.as_bytes());
}

#[test]
fn a_folder_where_the_todo_file_was_is_told_and_owed_again() {
    let dir = workdir(LIST);
    let path = dir.path().join("todo.txt");
    let (mut model, _) = watched(dir.path(), alone());
    fs::remove_file(&path).unwrap_or_else(|e| panic!("{e}"));
    fs::create_dir(&path).unwrap_or_else(|e| panic!("{e}"));
    changed(&mut model);
    reload(&mut model);
    assert_eq!(
        model.message.as_deref(),
        Some(format!("{} is a folder", path.display()).as_str())
    );
    assert!(model.file_exists(), "the list shown stays");
    assert!(model.reload_owed().is_some(), "owed again");
}

#[test]
fn an_empty_todo_file_removed_is_told_and_its_return_too() {
    let dir = workdir("");
    let path = dir.path().join("todo.txt");
    fs::write(&path, "").unwrap_or_else(|e| panic!("{e}"));
    let (mut model, _) = watched(dir.path(), alone());
    assert!(model.file_exists());
    // Gone or there, the todo file reads as no bytes.
    fs::remove_file(&path).unwrap_or_else(|e| panic!("{e}"));
    changed(&mut model);
    reload(&mut model);
    assert!(!model.file_exists());
    assert_eq!(model.notices(), ["todo.txt deleted"]);
    fs::write(&path, "").unwrap_or_else(|e| panic!("{e}"));
    changed(&mut model);
    reload(&mut model);
    assert!(model.file_exists());
    assert_eq!(model.notices(), Vec::<String>::new());
}

#[test]
fn a_watch_that_fails_to_start_says_so_and_reloads_each_minute() {
    let dir = workdir(LIST);
    let d = deps(dir.path(), alone());
    let mut s = start(dir.path());
    s.watch = Some(Box::new(|_, _| Err("no inotify".to_owned())));
    let mut model = started_from(s, d);
    assert_eq!(
        model.message.as_deref(),
        Some("can't watch todo.txt: no inotify; reloading every minute")
    );
    fs::write(dir.path().join("todo.txt"), NEXT)
        .unwrap_or_else(|e| panic!("{e}"));
    minute(&mut model);
    assert_eq!(model.snap.todo().to_bytes(), NEXT.as_bytes());
}

#[test]
fn a_busy_lock_is_tried_again_at_the_next_minute() {
    let dir = workdir(LIST);
    let config = Config {
        lock_timeout: Duration::from_millis(1),
        ..alone()
    };
    let (mut model, _feed) = watched(dir.path(), config);
    let path = dir.path().join("todo.txt");
    let mut held = held_lock(dir.path());
    fs::write(&path, NEXT).unwrap_or_else(|e| panic!("{e}"));
    changed(&mut model);
    reload(&mut model);
    assert_eq!(
        model.notices(),
        ["todo.txt is busy; showing the last state read"]
    );
    assert_eq!(model.snap.todo().to_bytes(), LIST.as_bytes());
    held.release().unwrap_or_else(|e| panic!("{}", e.message()));
    minute(&mut model);
    assert_eq!(model.notices(), Vec::<String>::new());
    assert_eq!(model.snap.todo().to_bytes(), NEXT.as_bytes());
}

#[test]
fn a_reload_that_inserts_above_the_cursor_keeps_it_on_its_item() {
    let dir = workdir("One\nTwo\nThree\n");
    let (mut model, _) = watched(dir.path(), alone());
    keys(&mut model, "G");
    fs::write(dir.path().join("todo.txt"), "New one\nOne\nTwo\nThree\n")
        .unwrap_or_else(|e| panic!("{e}"));
    changed(&mut model);
    reload(&mut model);
    assert_eq!(model.cursor_place(), Some(at(Source::Todo, 4)));
}

/// Gives a folder its mode back when dropped, so it can be deleted.
struct Unlocked<'a>(&'a std::path::Path);

impl Drop for Unlocked<'_> {
    fn drop(&mut self) {
        drop(fs::set_permissions(
            self.0,
            fs::Permissions::from_mode(0o755),
        ));
    }
}

// A reading of an open list and `find` classify the todo file alike: the
// same state, or the same error text, for paths that are awkward to examine.
#[test]
fn a_reading_and_find_agree_on_awkward_paths() {
    use lsty_core::discover::{self, FoundBy, Input};

    let (dir, model) = workdir_model("", alone());
    let root = fs::canonicalize(dir.path()).unwrap_or_else(|e| panic!("{e}"));
    let make = |p: &str| fs::create_dir_all(root.join(p));
    make("dir/inner").unwrap_or_else(|e| panic!("{e}"));
    make("locked/sub").unwrap_or_else(|e| panic!("{e}"));
    fs::write(root.join("file"), "").unwrap_or_else(|e| panic!("{e}"));
    symlink(root.join("nodir/todo.txt"), root.join("dangling"))
        .unwrap_or_else(|e| panic!("{e}"));
    symlink(root.join("dir"), root.join("tofolder"))
        .unwrap_or_else(|e| panic!("{e}"));
    symlink(root.join("loop"), root.join("loop"))
        .unwrap_or_else(|e| panic!("{e}"));
    fs::set_permissions(
        root.join("locked"),
        fs::Permissions::from_mode(0o000),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let _restore = Unlocked(&root.join("locked"));
    let searchable = fs::read_dir(root.join("locked")).is_ok();
    let mut paths =
        vec!["dangling", "tofolder", "loop", "file/x", "gone/todo.txt"];
    if !searchable {
        paths.extend(["locked/todo.txt", "locked/sub/todo.txt"]);
    }
    let clock = model.clock_now().unwrap_or_else(|e| panic!("{e}"));
    for name in paths {
        let path = root.join(name);
        let input = Input {
            explicit: Some(&path),
            vars: &|_| None,
            cwd: &root,
            home: None,
            default_file: None,
        };
        let found = discover::find(&input);
        let mut shown = discover::Found {
            todo: path.clone(),
            done: root.join("done.txt"),
            found_by: FoundBy::File,
            state: discover::FileState::Exists,
        };
        if let Ok(f) = &found {
            shown.todo.clone_from(&f.todo);
        }
        let read =
            live::fetch(&shown, &model.deps.config, &clock, &Cancel::new());
        match (found, read) {
            (Ok(f), Ok(s)) => assert_eq!(f.state, s.found().state, "{name}"),
            (Err(f), Err(r)) => assert_eq!(f.message(), r.message(), "{name}"),
            (f, r) => panic!("{name}: {:?} / {:?}", f.is_ok(), r.is_ok()),
        }
    }
}

/// A model on 40 items with the new-item field open on the last page, its
/// folder, and the view.
fn field_at_the_end() -> (tempfile::TempDir, Model, String, usize) {
    let lines: Vec<String> =
        (1..=40).map(|i| format!("Item{i:02} +p\n")).collect();
    let todo = lines.concat();
    let (dir, mut model) = workdir_model(&todo, alone());
    keys(&mut model, "G4ko");
    assert!(model.edit.is_some());
    let top = model.top;
    (dir, model, todo, top)
}

#[test]
fn a_minute_tick_leaves_the_view_alone_with_a_field_open() {
    let (_dir, mut model, _, top) = field_at_the_end();
    model.update(Msg::Minute);
    assert_eq!(model.top, top);
}

#[test]
fn an_outside_append_leaves_the_view_alone_with_a_field_open() {
    let (dir, mut model, todo, top) = field_at_the_end();
    fs::write(dir.path().join("todo.txt"), format!("{todo}New01 +p\n"))
        .unwrap_or_else(|e| panic!("{e}"));
    changed(&mut model);
    reload(&mut model);
    assert!(model.edit.is_some());
    assert_eq!(model.top, top);
}
