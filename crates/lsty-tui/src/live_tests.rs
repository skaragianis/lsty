use std::cell::Cell;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::rc::Rc;
use std::time::Duration;

use lsty_core::config::Config;
use lsty_core::lock::Cancel;

use crate::live::{self, DEBOUNCE};
use crate::model::Model;
use crate::msg::{Cmd, Msg};
use crate::test_util::{
    alone, deps, keys, load_of, press, reload, start, started_from, workdir,
    workdir_model,
};
use crate::{Notice, Watcher};

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

// 33.27
#[test]
fn what_the_model_wrote_itself_is_not_read_again() {
    let dir = workdir(LIST);
    let (mut model, _) = watched(dir.path(), alone());
    // The model's own write: the file changes and the model shows what it
    // wrote, as it does after each of its writes.
    fs::write(dir.path().join("todo.txt"), NEXT)
        .unwrap_or_else(|e| panic!("{e}"));
    let clock = model.clock_now().unwrap_or_else(|e| panic!("{e}"));
    let wrote = live::fetch(
        model.snap.found(),
        &model.deps.config,
        &clock,
        &Cancel::new(),
    )
    .unwrap_or_else(|e| panic!("{}", e.message()));
    model.install(wrote, clock, None);
    let before = first_item(&model);
    changed(&mut model);
    reload(&mut model);
    assert!(
        std::ptr::eq(first_item(&model), before),
        "nothing was built"
    );
}

// 33.28
#[test]
fn an_outside_write_within_the_debounce_of_the_models_own_is_shown() {
    let dir = workdir(LIST);
    let (mut model, _) = watched(dir.path(), alone());
    let path = dir.path().join("todo.txt");
    fs::write(&path, NEXT).unwrap_or_else(|e| panic!("{e}"));
    let clock = model.clock_now().unwrap_or_else(|e| panic!("{e}"));
    let own = live::fetch(
        model.snap.found(),
        &model.deps.config,
        &clock,
        &Cancel::new(),
    )
    .unwrap_or_else(|e| panic!("{}", e.message()));
    model.install(own, clock, None);
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
    let held = lsty_core::lock::acquire(
        &[lsty_core::lock::Want {
            file: path.clone(),
            mode: lsty_core::lock::Mode::Exclusive,
        }],
        Duration::from_secs(1),
        &Cancel::new(),
    );
    let mut held = held.unwrap_or_else(|e| panic!("{}", e.message()));
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

fn long_list(n: usize) -> String {
    let lines: Vec<String> = (1..=n)
        .map(|i| format!("Item {i} +p{} @c{}\n", i % 50, i % 7))
        .collect();
    lines.concat()
}

#[test]
fn reloading_ten_thousand_lines_is_fast() {
    let dir = workdir(long_list(10_000));
    let (mut model, _) = watched(dir.path(), alone());
    keys(&mut model, "G");
    let next = format!("New one +p1\n{}", long_list(10_000));
    fs::write(dir.path().join("todo.txt"), next)
        .unwrap_or_else(|e| panic!("{e}"));
    changed(&mut model);
    let at = std::time::Instant::now();
    reload(&mut model);
    let took = at.elapsed();
    eprintln!("reload of 10 000 lines {took:?}");
    assert_eq!(
        model.cursor_place(),
        Some(crate::test_util::at(lsty_core::errs::Source::Todo, 10_001))
    );
    assert!(took < Duration::from_secs(5));
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
    use std::os::unix::fs::symlink;

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
