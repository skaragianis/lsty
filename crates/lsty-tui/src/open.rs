//! `gx`: opening the `file:` links of the item under the cursor.

use std::os::unix::ffi::OsStrExt as _;
use std::path::{Path, PathBuf};

use lsty_core::errs::lossy;
use lsty_core::links::{self, Kind, editor_command, linked_files};

use crate::focus::Focus;
use crate::hit::{Pane, Target};
use crate::model::Model;
use crate::msg::Cmd;
use crate::picker::Picker;

/// What the status line says about an item without links.
const NO_LINK: &str = "no file: link on this item";

/// A `file:` link of an item: as written and where it points.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LinkRef {
    pub(crate) raw: Vec<u8>,
    pub(crate) path: PathBuf,
}

impl LinkRef {
    /// The sign of whether the file exists now.
    pub(crate) fn mark(&self) -> &'static str {
        exists_mark(self.path.exists())
    }
}

/// The sign of a file that exists or does not.
pub(crate) fn exists_mark(exists: bool) -> &'static str {
    if exists { "✓" } else { "✗" }
}

fn shown(path: &Path) -> String {
    lossy(path.as_os_str().as_bytes())
}

impl Model {
    /// The links of the item under the cursor.
    fn cursor_links(&self) -> Vec<LinkRef> {
        let item = self
            .rows
            .get(self.cursor)
            .and_then(|row| self.snap.item(row.id));
        let Some(item) = item else {
            return Vec::new();
        };
        linked_files(&self.snap, item, self.deps.home.as_deref())
            .into_iter()
            .map(|file| LinkRef {
                raw: file.raw.to_vec(),
                path: file.path,
            })
            .collect()
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

    /// Opens link `n` of `links`, counted from 1.
    pub(crate) fn open_number(
        &mut self,
        links: &[LinkRef],
        n: usize,
    ) -> Vec<Cmd> {
        let Some(link) = n.checked_sub(1).and_then(|i| links.get(i)) else {
            self.message =
                Some(format!("this item has {} links", links.len()));
            return Vec::new();
        };
        self.launch(&link.path)
    }

    /// Opens `path` as [`links::classify`] decides.
    fn launch(&mut self, path: &Path) -> Vec<Cmd> {
        match links::classify(path) {
            Kind::Missing(reason) | Kind::Refused(reason) => {
                self.alert(reason);
            }
            Kind::Text => {
                let editor = self.deps.editor.as_deref();
                return vec![Cmd::Edit(editor_command(editor, path))];
            }
            Kind::Other => self.start_opener(path),
        }
        Vec::new()
    }

    fn start_opener(&mut self, path: &Path) {
        match (self.deps.opener)(path) {
            Ok(()) => self.message = Some(format!("opened {}", shown(path))),
            Err(reason) => {
                self.alert(format!("can't open {}: {reason}", shown(path)));
            }
        }
    }

    /// Opens the link that a click on a row or on the detail panel
    /// shows.
    pub(crate) fn click_link(&mut self, t: &Target) -> Vec<Cmd> {
        if t.pane == Pane::List {
            self.set_focus(Focus::List);
            self.exit_visual();
            self.cursor_to_row(t.at);
        }
        let links = self.cursor_links();
        let found = links.iter().enumerate().find(|(i, link)| match t.pane {
            Pane::Detail => *i == t.link,
            Pane::List => link.raw == t.name,
            Pane::None | Pane::Sidebar | Pane::Popup => false,
        });
        match found {
            Some((_, link)) => self.launch(&link.path),
            None => Vec::new(),
        }
    }
}
