//! Watching the folders of the todo and done files.

use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use lsty_core::{discover::Found, errs};
use notify::{
    Config, Error, ErrorKind, Event, RecommendedWatcher, RecursiveMode,
    Watcher as _,
    event::{EventKind, ModifyKind},
};
use rustix::io::Errno;

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
    /// Adds the folders again, so that one deleted and made again, or one
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
    pub(crate) inner: RecommendedWatcher,
    /// Each folder with whether the last attempt to watch it worked.
    pub(crate) dirs: Vec<(PathBuf, bool)>,
}

/// The files and folders an event is matched against.
pub(crate) struct Targets {
    pub(crate) files: Vec<PathBuf>,
    pub(crate) dirs: Vec<PathBuf>,
}

impl Targets {
    pub(crate) fn of(found: &Found) -> Self {
        let files = vec![found.todo.clone(), found.done.clone()];
        let mut dirs: Vec<PathBuf> = files
            .iter()
            .filter_map(|file| file.parent().map(Path::to_path_buf))
            .collect();
        dirs.dedup();
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
pub(crate) fn reason(e: &Error) -> String {
    match &e.kind {
        ErrorKind::Io(err) => errs::io_text(err),
        ErrorKind::MaxFilesWatch => {
            errs::io_text(&io::Error::from(Errno::NOSPC))
        }
        ErrorKind::PathNotFound => {
            errs::io_text(&io::Error::from(Errno::NOENT))
        }
        ErrorKind::Generic(text) => text.clone(),
        ErrorKind::WatchNotFound => "watch not found".to_owned(),
        ErrorKind::InvalidConfig(_) => "invalid configuration".to_owned(),
    }
}

impl Watcher for FolderWatcher {
    fn rewatch(&mut self) -> Result<(), String> {
        let mut first = None;
        for (dir, watched) in &mut self.dirs {
            // inotify watches an inode, so Linux adds every folder again;
            // FSEvents watches a path, and adding one restarts its stream.
            if *watched && !cfg!(target_os = "linux") {
                continue;
            }
            let result = self.inner.watch(dir, RecursiveMode::NonRecursive);
            *watched = result.is_ok();
            if let Err(e) = result
                && first.is_none()
                && !is_absent(&e)
            {
                let path = errs::lossy_path(dir);
                first = Some(format!("watch {path}: {}", reason(&e)));
            }
        }
        first.map_or(Ok(()), Err)
    }
}

fn is_absent(e: &Error) -> bool {
    match &e.kind {
        ErrorKind::PathNotFound => true,
        ErrorKind::Io(io) => io.kind() == io::ErrorKind::NotFound,
        ErrorKind::Generic(_)
        | ErrorKind::WatchNotFound
        | ErrorKind::InvalidConfig(_)
        | ErrorKind::MaxFilesWatch => false,
    }
}

/// Watches the folders of the todo and done files and hands what
/// happens to them to `sink`. A folder that does not exist is not an
/// error.
///
/// # Errors
///
/// The system's watch cannot be created or a folder cannot be watched.
pub fn watch_folders(
    found: &Found,
    sink: Sink,
) -> Result<Box<dyn Watcher>, String> {
    let targets = Targets::of(found);
    let dirs = targets.dirs.iter().map(|d| (d.clone(), false)).collect();
    let handler = move |result: notify::Result<Event>| {
        forward(&targets, &sink, result);
    };
    let inner = RecommendedWatcher::new(handler, Config::default())
        .map_err(|e| format!("watch: {}", reason(&e)))?;
    let mut watcher = FolderWatcher { inner, dirs };
    watcher.rewatch()?;
    Ok(Box::new(watcher))
}
