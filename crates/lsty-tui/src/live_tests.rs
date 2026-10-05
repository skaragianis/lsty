use std::cell::Cell;
use std::fs;
use std::rc::Rc;
use std::time::Duration;

use lsty_core::config::Config;
use lsty_core::lock::Cancel;

use crate::live::{self, DEBOUNCE};
use crate::test_util::{alone, deps, started, workdir};
use crate::{Cmd, Load, Model, Msg, Notice, Watcher};

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
    let mut d = deps(dir, config);
    let shared = Rc::clone(&feed);
    d.watch = Some(Box::new(move |_, _| {
        Ok(Box::new(Scripted(Rc::clone(&shared))))
    }));
    (started(d), feed)
}

/// The change of a file, as the watch tells it.
fn changed(model: &mut Model) -> Vec<Cmd> {
    model.update(Msg::Notice(Notice::Changed))
}

/// The reading that a reload asks for.
fn load_of(cmds: Vec<Cmd>) -> Box<Load> {
    for cmd in cmds {
        if let Cmd::Load(load) = cmd {
            return load;
        }
    }
    panic!("no load was asked for");
}

/// The clock reaching a minute, with the reading it asks for done.
fn minute(model: &mut Model) {
    for cmd in model.update(Msg::Minute) {
        if let Cmd::Load(load) = cmd {
            model.update(load.read());
        }
    }
}

/// A reload, with its reading done at once; what the model asks for
/// after the reading comes back.
fn reload(model: &mut Model) -> Vec<Cmd> {
    let load = load_of(model.update(Msg::Reload));
    model.update(load.read())
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
    let dir = workdir(LIST);
    let (mut model, _) = watched(dir.path(), alone());
    changed(&mut model);
    let load = load_of(model.update(Msg::Reload));
    fs::write(dir.path().join("todo.txt"), NEXT)
        .unwrap_or_else(|e| panic!("{e}"));
    // What is shown changes (as with the model's own write) while the
    // files are being read.
    model.live.serial += 1;
    assert_eq!(model.update(load.read()), debounce());
    assert_eq!(model.snap.todo().to_bytes(), LIST.as_bytes());
    assert_eq!(reload(&mut model), Vec::new());
    assert_eq!(model.snap.todo().to_bytes(), NEXT.as_bytes());
}

fn first_item(model: &Model) -> *const lsty_core::list::Item {
    model.snap.items().as_ptr()
}

// 33.27
#[test]
fn what_the_model_wrote_itself_is_not_read_again() {
    let dir = workdir(LIST);
    let (mut model, _) = watched(dir.path(), alone());
    let before = first_item(&model);
    // The model's own write: the file changes and the model notes what it
    // holds, as it does after each of its writes.
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
    model.live.seen = wrote.stamp;
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
    model.live.seen = own.stamp;
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
fn a_watch_that_fails_to_start_says_so_and_reloads_each_minute() {
    let dir = workdir(LIST);
    let mut d = deps(dir.path(), alone());
    d.watch = Some(Box::new(|_, _| Err("no inotify".to_owned())));
    let mut model = started(d);
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
    let dir = workdir(&long_list(10_000));
    let (mut model, _) = watched(dir.path(), alone());
    model.update(Msg::Key(lsty_core::keys::Stroke::char('G')));
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
