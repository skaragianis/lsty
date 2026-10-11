use std::{
    cell::Cell,
    fs, io,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc::{Sender, channel},
    },
    thread,
    time::{Duration, Instant},
};

use lsty_core::{
    config::Config,
    keys::Stroke,
    lock::{Cancel, Mode, Want, acquire},
};

use crate::{
    Notice, Sink,
    model::Model,
    run::{Feed, Input, Screen, run_with},
    test_util::{SAMPLE, deps, held_lock, start, workdir},
};

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
    let screen = Script::new([key('j'), key('q'), key('j')]);
    let drawn = Arc::clone(&screen.drawn);
    let mut stderr = Vec::new();
    let code = run_with(
        start(dir.path()),
        deps(dir.path(), Config::default()),
        || Ok(screen),
        &mut stderr,
    );
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stderr), "");
    assert_eq!(drawn.load(Ordering::Relaxed), 2, "after each key");
}

/// A terminal that hands its feed to the test's helper thread.
struct Contended {
    feed: Sender<Feed>,
    cursor: Arc<AtomicUsize>,
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
        self.feed.send(feed).map_err(io::Error::other)
    }
}

#[test]
fn keys_are_handled_while_a_reload_waits_for_a_lock() {
    let dir = workdir(SAMPLE);
    let mut d = deps(dir.path(), Config::default());
    d.config.lock_timeout = Duration::from_secs(30);
    let slot: Arc<Mutex<Option<Sink>>> = Arc::default();
    let shared = Arc::clone(&slot);
    let mut s = start(dir.path());
    s.watch = Some(Box::new(move |_, sink| {
        if let Ok(mut s) = shared.lock() {
            *s = Some(sink);
        }
        Ok(Box::new(Idle))
    }));
    let (todo, path) = (s.found.todo.clone(), dir.path().join("todo.txt"));
    let (release, wait) = channel::<()>();
    let (send, feeds) = channel::<Feed>();
    let cursor = Arc::new(AtomicUsize::new(0));
    // The helper holds the lock, changes the file, tells the model, and
    // presses keys while the reload waits for the lock.
    let helper = thread::spawn(move || {
        let Ok(feed) = feeds.recv() else { return };
        let lock = acquire(
            &[Want {
                file: todo,
                mode: Mode::Exclusive,
            }],
            Duration::from_secs(5),
            &Cancel::new(),
        );
        let Ok(mut held) = lock else { return };
        drop(fs::write(path, "Changed +work\n"));
        if let Some(sink) = slot.lock().ok().and_then(|s| s.clone()) {
            sink(Notice::Changed);
        }
        // The debounce passes and the reading waits for the lock.
        thread::sleep(Duration::from_millis(400));
        feed.send(key('j'));
        feed.send(key('q'));
        // The lock stays held until the run has ended.
        wait.recv().ok();
        drop(held.release());
    });
    let screen = Contended {
        feed: send,
        cursor: Arc::clone(&cursor),
    };
    let at = Instant::now();
    let mut stderr = Vec::new();
    let code = run_with(s, d, || Ok(screen), &mut stderr);
    let took = at.elapsed();
    release.send(()).unwrap_or_else(|e| panic!("{e}"));
    // The reading would wait 30 s for the lock, which is held until
    // now; the keys do not wait for it.
    assert!(took < Duration::from_secs(10), "{took:?}");
    assert_eq!(code, 0);
    assert_eq!(cursor.load(Ordering::Relaxed), 1, "j moved the cursor");
    helper.join().unwrap_or_else(|_| panic!("helper panicked"));
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
    let mut held = held_lock(dir.path());
    let opened = Cell::new(false);
    let mut stderr = Vec::new();
    let code = run_with(
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
fn a_screen_that_cannot_be_opened_ends_the_run_with_the_system_text() {
    for (err, want) in [
        (io::Error::other("no terminal"), "no terminal"),
        (io::Error::from(rustix::io::Errno::IO), "input/output error"),
    ] {
        let dir = workdir(SAMPLE);
        let mut stderr = Vec::new();
        let code = run_with::<Script>(
            start(dir.path()),
            deps(dir.path(), Config::default()),
            move || Err(err),
            &mut stderr,
        );
        assert_eq!(code, 1);
        assert_eq!(
            String::from_utf8_lossy(&stderr),
            format!("lsty: terminal: {want}\n")
        );
    }
}
