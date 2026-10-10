//! `gx`: opening the `file:` links of the item under the cursor.

use std::path::Path;

use lsty_core::{
    errs::lossy_path,
    links::{self, Kind, LinkedFile, editor_command, linked_files},
};

use crate::{
    focus::Focus,
    hit::{Pane, Target},
    model::Model,
    msg::Cmd,
    picker::Picker,
};

/// What the status line says about an item without links.
const NO_LINK: &str = "no file: link on this item";

/// The sign of a file that exists or does not.
pub(crate) fn exists_mark(exists: bool) -> &'static str {
    if exists { "✓" } else { "✗" }
}

impl Model {
    /// The links of the item under the cursor.
    pub(crate) fn cursor_links(&self) -> Vec<LinkedFile> {
        let item = self
            .rows
            .get(self.cursor)
            .and_then(|row| self.snap.item(row.id));
        let Some(item) = item else {
            return Vec::new();
        };
        linked_files(&self.snap, item, self.deps.home.as_deref())
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
    /// `count` of the cursor item.
    pub(crate) fn open_key(&mut self, count: usize) -> Vec<Cmd> {
        if self.focus == Focus::Detail {
            return self.open_link(self.detail_link() + 1);
        }
        self.open_link(count)
    }

    /// Opens link `n` of `links`, counted from 1.
    pub(crate) fn open_number(
        &mut self,
        links: &[LinkedFile],
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
            Ok(()) => {
                self.message = Some(format!("opened {}", lossy_path(path)));
            }
            Err(reason) => {
                self.alert(format!(
                    "can't open {}: {reason}",
                    lossy_path(path)
                ));
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
        let found = if t.name.is_empty() {
            links.get(t.link)
        } else {
            links.iter().find(|l| l.raw == t.name)
        };
        found.map_or_else(Vec::new, |link| self.launch(&link.path))
    }
}
