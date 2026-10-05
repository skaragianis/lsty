use std::cell::Cell;
use std::fs;
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use lsty_core::config::Config;
use lsty_core::keys::Stroke;
use lsty_core::lock::{Cancel, Mode, Want, acquire};

use crate::test_util::{SAMPLE, deps, start, workdir};
use crate::{Feed, Input, Model, Notice, Screen, Sink, run};

/// A terminal that plays the inputs it is given, then stops.
struct Script {
    size: (u16, u16),
    inputs: Vec<Input>,
    drawn: Arc<AtomicUsize>,
}

impl Script {
    fn new(inputs: impl IntoIterator<Item = Input>) -> Self {
        Self {
            size: (80, 24),
            inputs: inputs.into_iter().collect(),
            drawn: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl Screen for Script {
    fn size(&mut self) -> io::Result<(u16, u16)> {
        Ok(self.size)
    }

    fn draw(&mut self, _: &Model) -> io::Result<()> {
        self.drawn.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    fn start(&mut self, feed: Feed) -> io::Result<()> {
        for input in self.inputs.drain(..) {
            feed.send(input);
        }
        feed.send(Input::Stop);
        Ok(())
    }
}

fn key(c: char) -> Input {
    Input::Key(Stroke::char(c))
}

#[test]
fn q_ends_the_loop_with_code_0() {
    let dir = workdir(SAMPLE);
    let mut screen = Script::new([key('j'), key('q'), key('j')]);
    let mut stderr = Vec::new();
    let code = run(
        start(dir.path()),
        deps(dir.path(), Config::default()),
        || Ok(&mut screen),
        &mut stderr,
    );
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stderr), "");
    assert_eq!(screen.drawn.load(Ordering::Relaxed), 2, "after each key");
}

/// What the helper thread of the busy-lock test needs.
struct Busy {
    dir: std::path::PathBuf,
    todo: std::path::PathBuf,
    sink: Arc<Mutex<Option<Sink>>>,
}

/// A terminal whose thread holds the lock, changes the file, tells the
/// model, and presses keys while the reload waits for the lock.
struct Contended {
    busy: Busy,
    release: Option<Receiver<()>>,
    cursor: Arc<AtomicUsize>,
    helper: Option<thread::JoinHandle<()>>,
}

impl Screen for Contended {
    fn size(&mut self) -> io::Result<(u16, u16)> {
        Ok((80, 24))
    }

    fn draw(&mut self, model: &Model) -> io::Result<()> {
        self.cursor.store(model.cursor, Ordering::Relaxed);
        Ok(())
    }

    fn start(&mut self, feed: Feed) -> io::Result<()> {
        let (todo, dir) = (self.busy.todo.clone(), self.busy.dir.clone());
        let slot = Arc::clone(&self.busy.sink);
        let release = self.release.take();
        self.helper = Some(thread::spawn(move || {
            let want = Want {
                file: todo.clone(),
                mode: Mode::Exclusive,
            };
            let held =
                acquire(&[want], Duration::from_secs(5), &Cancel::new());
            let Ok(mut held) = held else { return };
            drop(fs::write(dir.join("todo.txt"), "Changed +work\n"));
            let sink = slot.lock().ok().and_then(|s| s.clone());
            if let Some(sink) = sink {
                sink(Notice::Changed);
            }
            // The debounce passes and the reading waits for the lock.
            thread::sleep(Duration::from_millis(400));
            feed.send(key('j'));
            feed.send(key('q'));
            // The lock stays held until the run has ended.
            release.map(|wait| wait.recv().is_ok());
            drop(held.release());
        }));
        Ok(())
    }
}

#[test]
fn keys_are_handled_while_a_reload_waits_for_a_lock() {
    let dir = workdir(SAMPLE);
    let mut d = deps(dir.path(), Config::default());
    d.config.lock_timeout = Duration::from_secs(30);
    let slot: Arc<Mutex<Option<Sink>>> = Arc::default();
    let shared = Arc::clone(&slot);
    d.watch = Some(Box::new(move |_, sink| {
        if let Ok(mut s) = shared.lock() {
            *s = Some(sink);
        }
        Ok(Box::new(Idle))
    }));
    let s = start(dir.path());
    let busy = Busy {
        dir: dir.path().to_path_buf(),
        todo: s.found.todo.clone(),
        sink: slot,
    };
    let (release, wait) = channel();
    let cursor = Arc::new(AtomicUsize::new(0));
    let mut screen = Contended {
        busy,
        release: Some(wait),
        cursor: Arc::clone(&cursor),
        helper: None,
    };
    let at = Instant::now();
    let mut stderr = Vec::new();
    let code = run(s, d, || Ok(&mut screen), &mut stderr);
    let took = at.elapsed();
    release.send(()).unwrap_or_else(|e| panic!("{e}"));
    // The reading would wait 30 s for the lock, which is held until
    // now; the keys do not wait for it.
    assert!(took < Duration::from_secs(10), "{took:?}");
    assert_eq!(code, 0);
    assert_eq!(cursor.load(Ordering::Relaxed), 1, "j moved the cursor");
    if let Some(helper) = screen.helper.take() {
        helper.join().unwrap_or_else(|_| panic!("helper panicked"));
    }
}

struct Idle;

impl crate::Watcher for Idle {
    fn rewatch(&mut self) -> Result<(), String> {
        Ok(())
    }
}

#[test]
fn a_busy_lock_ends_the_run_with_its_error() {
    let dir = workdir(SAMPLE);
    let mut d = deps(dir.path(), Config::default());
    d.config.lock_timeout = Duration::from_millis(50);
    let s = start(dir.path());
    let want = Want {
        file: s.found.todo.clone(),
        mode: Mode::Exclusive,
    };
    let held = acquire(&[want], Duration::from_secs(1), &Cancel::new());
    let mut held = held.unwrap_or_else(|e| panic!("{}", e.message()));
    let opened = Cell::new(false);
    let mut stderr = Vec::new();
    let code = run(
        s,
        d,
        || {
            opened.set(true);
            Ok(Script::new([]))
        },
        &mut stderr,
    );
    held.release().unwrap_or_else(|e| panic!("{}", e.message()));
    assert_eq!(code, 4);
    let text = String::from_utf8_lossy(&stderr);
    assert_eq!(text, "lsty: todo.txt is busy (locked by another writer)\n");
    assert!(!opened.get(), "the terminal is not taken over");
}

#[test]
fn a_screen_that_cannot_be_opened_ends_the_run() {
    let dir = workdir(SAMPLE);
    let mut stderr = Vec::new();
    let code = run::<Script>(
        start(dir.path()),
        deps(dir.path(), Config::default()),
        || Err(io::Error::other("no terminal")),
        &mut stderr,
    );
    assert_eq!(code, 1);
    assert_eq!(
        String::from_utf8_lossy(&stderr),
        "lsty: terminal: no terminal\n"
    );
}

#[test]
fn a_system_error_of_the_terminal_reads_like_the_system_says_it() {
    let dir = workdir(SAMPLE);
    let mut stderr = Vec::new();
    let code = run::<Script>(
        start(dir.path()),
        deps(dir.path(), Config::default()),
        || Err(io::Error::from_raw_os_error(5)),
        &mut stderr,
    );
    assert_eq!(code, 1);
    assert_eq!(
        String::from_utf8_lossy(&stderr),
        "lsty: terminal: input/output error\n"
    );
}
