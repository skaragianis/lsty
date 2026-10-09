//! Watching the folders of the todo and done files.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use lsty_core::discover::Found;
use lsty_core::errs;
use notify::event::{EventKind, ModifyKind};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher as _};

/// What a watcher reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    /// Something happened to one of the two files.
    Changed,
    /// The system's watch failed; the reason is its own text.
    Failed(String),
}

/// Where a watcher hands its notices. It is called from the system's
/// thread and must not block.
pub type Sink = Arc<dyn Fn(Notice) + Send + Sync>;

/// A watch on the two files.
pub trait Watcher {
    /// Adds the folders again: one that was deleted and made again, or
    /// that appeared after the start, is watched from then on.
    ///
    /// # Errors
    ///
    /// The reason a folder cannot be watched.
    fn rewatch(&mut self) -> Result<(), String>;
}

/// Starts a watch on the files of `found`, handing its notices to the
/// sink.
pub type Watch =
    Box<dyn Fn(&Found, Sink) -> Result<Box<dyn Watcher>, String> + 'static>;

/// The reason when the system's queue of events overflowed.
const OVERFLOW: &str = "queue or buffer overflow";

/// Watches the folders of the files with the system's facility and
/// passes on the events of the files themselves. A watch on a file would
/// go dead with the first rename.
pub(crate) struct FolderWatcher {
    inner: RecommendedWatcher,
    dirs: Vec<PathBuf>,
}

/// The files and folders an event is matched against.
#[derive(Clone)]
pub(crate) struct Targets {
    pub(crate) files: Vec<PathBuf>,
    pub(crate) dirs: Vec<PathBuf>,
}

impl Targets {
    pub(crate) fn of(found: &Found) -> Self {
        let files = vec![found.todo.clone(), found.done.clone()];
        let mut dirs: Vec<PathBuf> = Vec::new();
        for file in &files {
            let dir =
                file.parent().map_or_else(PathBuf::new, Path::to_path_buf);
            if !dirs.contains(&dir) {
                dirs.push(dir);
            }
        }
        Self { files, dirs }
    }

    /// Whether the event concerns one of the files, or the loss of one of
    /// the folders. Every kind of change counts; looking at a file does
    /// not.
    pub(crate) fn passes(&self, event: &Event) -> bool {
        if matches!(event.kind, EventKind::Access(_)) {
            return false;
        }
        let gone = matches!(
            event.kind,
            EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(_))
        );
        event.paths.iter().any(|path| {
            self.files.contains(path) || gone && self.dirs.contains(path)
        })
    }
}

impl FolderWatcher {
    /// Watches the folders of the todo and done files and hands what
    /// happens to them to `sink`. A folder that does not exist is not an
    /// error.
    ///
    /// # Errors
    ///
    /// The system's watch cannot be created or a folder cannot be
    /// watched.
    pub(crate) fn new(found: &Found, sink: Sink) -> Result<Self, String> {
        let targets = Targets::of(found);
        let dirs = targets.dirs.clone();
        let handler = move |result: notify::Result<Event>| {
            forward(&targets, &sink, result);
        };
        let inner =
            RecommendedWatcher::new(handler, notify::Config::default())
                .map_err(|e| format!("watch: {}", reason(&e)))?;
        let mut watcher = Self { inner, dirs };
        watcher.rewatch()?;
        Ok(watcher)
    }
}

pub(crate) fn forward(
    targets: &Targets,
    sink: &Sink,
    result: notify::Result<Event>,
) {
    match result {
        Ok(event) if event.need_rescan() => {
            sink(Notice::Failed(OVERFLOW.to_owned()));
        }
        Ok(event) if targets.passes(&event) => sink(Notice::Changed),
        Ok(_) => {}
        Err(e) => sink(Notice::Failed(reason(&e))),
    }
}

/// Why the system's watch failed, worded as an I/O error is elsewhere.
pub(crate) fn reason(e: &notify::Error) -> String {
    match &e.kind {
        notify::ErrorKind::Io(err) => errs::io_text(err),
        notify::ErrorKind::MaxFilesWatch => {
            errs::io_text(&io::Error::from_raw_os_error(NO_SPACE))
        }
        notify::ErrorKind::PathNotFound => {
            errs::io_text(&io::Error::from_raw_os_error(NO_ENTRY))
        }
        notify::ErrorKind::Generic(text) => text.clone(),
        notify::ErrorKind::WatchNotFound => "watch not found".to_owned(),
        notify::ErrorKind::InvalidConfig(_) => {
            "invalid configuration".to_owned()
        }
    }
}

/// `ENOENT`.
const NO_ENTRY: i32 = 2;

/// `ENOSPC`, which is what a full table of watches means.
const NO_SPACE: i32 = 28;

impl Watcher for FolderWatcher {
    fn rewatch(&mut self) -> Result<(), String> {
        for dir in &self.dirs {
            match self.inner.watch(dir, RecursiveMode::NonRecursive) {
                Ok(()) => {}
                Err(e) if is_absent(&e) => {}
                Err(e) => {
                    return Err(format!(
                        "watch {}: {}",
                        errs::lossy_path(dir),
                        reason(&e)
                    ));
                }
            }
        }
        Ok(())
    }
}

fn is_absent(e: &notify::Error) -> bool {
    match &e.kind {
        notify::ErrorKind::PathNotFound => true,
        notify::ErrorKind::Io(io) => io.kind() == io::ErrorKind::NotFound,
        notify::ErrorKind::Generic(_)
        | notify::ErrorKind::WatchNotFound
        | notify::ErrorKind::InvalidConfig(_)
        | notify::ErrorKind::MaxFilesWatch => false,
    }
}

/// The watch the binary starts: one on the folders of the files.
pub fn watch_folders(
    found: &Found,
    sink: Sink,
) -> Result<Box<dyn Watcher>, String> {
    FolderWatcher::new(found, sink).map(|w| Box::new(w) as Box<dyn Watcher>)
}
