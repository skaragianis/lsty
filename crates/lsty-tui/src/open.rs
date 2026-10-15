//! `gx`: opening the links of the task under the cursor: its `file:`
//! tags and the URLs in its text.

use std::{ffi::OsStr, os::unix::ffi::OsStrExt as _, path::Path};

use lsty_core::{
    errs::lossy_path,
    links::{self, Kind, Link, editor_command},
};

use crate::{
    focus::Focus, hit::Target, model::Model, msg::Cmd, picker::Picker,
};

/// What the status line says about a task without links.
const NO_LINK: &str = "no link on this task";

/// The sign of a file that exists or does not.
pub(crate) fn exists_mark(exists: bool) -> &'static str {
    if exists { "✓" } else { "✗" }
}

impl Model {
    /// The links of the task under the cursor.
    pub(crate) fn cursor_links(&self) -> Vec<Link> {
        let task = self
            .rows
            .get(self.cursor)
            .and_then(|row| self.snap.task(row.id));
        let Some(task) = task else {
            return Vec::new();
        };
        links::links(&self.snap, task, self.deps.home.as_deref())
    }

    /// `open_link`: opens the numbered link when there is a count, else
    /// the only one, else asks which.
    pub(crate) fn open_link(&mut self, count: usize) -> Vec<Cmd> {
        if self.cursor >= self.rows.len() {
            return Vec::new();
        }
        let links = self.cursor_links();
        if links.is_empty() {
            self.message = Some(NO_LINK.to_owned());
        } else if count > 0 {
            return self.open_number(&links, count);
        } else if links.len() == 1 {
            return self.open_number(&links, 1);
        } else {
            self.opening = Some(Picker::new(links));
        }
        Vec::new()
    }

    /// Opens the link that the detail panel has selected, else link
    /// `count` of the cursor task.
    pub(crate) fn open_key(&mut self, count: usize) -> Vec<Cmd> {
        if self.focus == Focus::Detail {
            return self.open_link(self.detail_link() + 1);
        }
        self.open_link(count)
    }

    /// Opens link `n` of `links`, counted from 1.
    pub(crate) fn open_number(
        &mut self,
        links: &[Link],
        n: usize,
    ) -> Vec<Cmd> {
        let Some(link) = n.checked_sub(1).and_then(|i| links.get(i)) else {
            self.message =
                Some(format!("this task has {} links", links.len()));
            return Vec::new();
        };
        self.launch(link)
    }

    /// Opens `link`: a URL by the system's opener, as one argument and
    /// never through a shell, the editor or [`links::classify`]; when it
    /// fails the URL goes to the clipboard instead. A file is opened as
    /// [`links::classify`] decides.
    fn launch(&mut self, link: &Link) -> Vec<Cmd> {
        match link {
            Link::File(file) => self.launch_file(&file.path),
            Link::Url(url) => {
                if self.start_opener(Path::new(OsStr::from_bytes(url))) {
                    return Vec::new();
                }
                self.note("copied it instead");
                self.copy_out(url.clone())
            }
        }
    }

    /// Opens `path` as [`links::classify`] decides.
    fn launch_file(&mut self, path: &Path) -> Vec<Cmd> {
        match links::classify(path) {
            Kind::Missing(reason) | Kind::Refused(reason) => {
                self.alert(reason);
            }
            Kind::Text => {
                let editor = self.deps.editor.as_deref();
                return vec![Cmd::Edit(editor_command(editor, path))];
            }
            Kind::Other => drop(self.start_opener(path)),
        }
        Vec::new()
    }

    /// Starts the system's opener on `path` and says whether it started.
    fn start_opener(&mut self, path: &Path) -> bool {
        let started = (self.deps.opener)(path);
        let shown = lossy_path(path);
        match &started {
            Ok(()) => self.message = Some(format!("opened {shown}")),
            Err(reason) => self.alert(format!("can't open {shown}: {reason}")),
        }
        started.is_ok()
    }

    /// Opens the link that a click on the detail panel shows.
    pub(crate) fn click_link(&mut self, t: &Target) -> Vec<Cmd> {
        let links = self.cursor_links();
        let found = if t.name.is_empty() {
            links.get(t.link)
        } else {
            links.iter().find(|l| l.shown() == t.name)
        };
        found.map_or_else(Vec::new, |link| self.launch(link))
    }
}
