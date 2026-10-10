//! Watching the folders of the todo and done files.

use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use lsty_core::discover::Found;
use lsty_core::errs;
use notify::event::{EventKind, ModifyKind};
use notify::{
    Config, Error, ErrorKind, Event, RecommendedWatcher, RecursiveMode,
    Watcher as _,
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
    /// Adds the folders that are not watched: one that was deleted and
    /// made again, or that appeared after the start, is watched from then
    /// on. A folder still watched is left alone.
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
struct FolderWatcher {
    inner: RecommendedWatcher,
    /// Each folder with the identity it had when it was last watched.
    dirs: Vec<(PathBuf, Option<Identity>)>,
}

/// Device, inode and birth time of a folder: a folder made again is
/// another one, even when the system hands its inode out a second time.
type Identity = (u64, u64, Option<SystemTime>);

pub(crate) fn identity(dir: &Path) -> Option<Identity> {
    let meta = fs::metadata(dir).ok()?;
    Some((meta.dev(), meta.ino(), meta.created().ok()))
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
        for (dir, seen) in &mut self.dirs {
            let now = identity(dir);
            if now.is_some() && now == *seen {
                continue;
            }
            *seen = None;
            match self.inner.watch(dir, RecursiveMode::NonRecursive) {
                Ok(()) => *seen = now,
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
    let dirs = targets.dirs.iter().map(|d| (d.clone(), None)).collect();
    let handler = move |result: notify::Result<Event>| {
        forward(&targets, &sink, result);
    };
    let inner = RecommendedWatcher::new(handler, Config::default())
        .map_err(|e| format!("watch: {}", reason(&e)))?;
    let mut watcher = FolderWatcher { inner, dirs };
    watcher.rewatch()?;
    Ok(Box::new(watcher))
}
