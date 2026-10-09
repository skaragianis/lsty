//! The item object of the JSON output, and the facts about a file link
//! that it and `show` share.

use std::os::unix::ffi::OsStrExt as _;
use std::path::Path;

use lsty_core::links::linked_files;
use lsty_core::list::{Item, Snapshot};
use lsty_core::todotxt::{Kind, Line, Tags};

use crate::jsonw::{self, Seq};
use crate::output::{Json, ref_json};

/// What an item is rendered against: its snapshot and the home folder
/// that `file:~/…` links lie under.
pub(crate) struct ItemJson<'a> {
    /// The snapshot the items come from.
    pub snap: &'a Snapshot,
    /// The home folder, when there is one.
    pub home: Option<&'a Path>,
}

impl ItemJson<'_> {
    /// The JSON objects of `items`, in order, written into one string.
    pub(crate) fn objects(&self, items: &[&Item]) -> Json {
        let mut out = String::new();
        jsonw::array(&mut out, |arr| {
            for item in items {
                arr.item_object(|obj| self.write(obj, item));
            }
        });
        Json::raw(out)
    }

    fn write(&self, obj: &mut Seq<'_>, item: &Item) {
        let line = self.snap.line(item);
        let tags = line.tags();
        self.identity(obj, item);
        self.dates_and_tags(obj, item, &tags);
        self.state(obj, item);
    }

    fn identity(&self, obj: &mut Seq<'_>, item: &Item) {
        let line = self.snap.line(item);
        let priority = line.effective_priority().map(|p| p.to_string());
        obj.int("nr", i64::from(item.nr().get()));
        obj.string("source", item.source().as_str());
        obj.bytes("raw", line.raw());
        obj.bool("done", line.is_done());
        obj.opt_string("priority", priority.as_deref());
        obj.opt_string("created", line.created());
        obj.opt_string("completed", line.completed());
        obj.bytes("description", line.description());
        obj.bytes("text", &line.text());
        names(obj, "projects", line, Kind::Project);
        names(obj, "contexts", line, Kind::Context);
        tag_pairs(obj, line);
    }

    fn dates_and_tags(&self, obj: &mut Seq<'_>, item: &Item, tags: &Tags<'_>) {
        let line = self.snap.line(item);
        let rec = tags.rec.and_then(|_| first_tag(line, b"rec"));
        let started = item
            .started()
            .map(|t| t.strftime("%Y-%m-%dT%H:%M:%SZ").to_string());
        obj.opt_string("due", tags.due);
        obj.opt_string("threshold", tags.threshold);
        obj.opt_bytes("rec", rec);
        obj.non_empty("id", tags.id);
        obj.array("deps", |arr| {
            for dep in &tags.deps {
                arr.item_bytes(dep);
            }
        });
        obj.opt_string("started", started.as_deref());
        obj.non_empty("owner", tags.owner);
    }

    fn state(&self, obj: &mut Seq<'_>, item: &Item) {
        obj.string("state", item.state().as_str());
        obj.bool("blocked", item.blocked());
        obj.array("blockers", |arr| self.blockers(arr, item));
        obj.array("dangling", |arr| {
            for id in self.snap.dangling(item) {
                arr.item_bytes(id);
            }
        });
        obj.bool("cycle", item.cycle());
        obj.bool("later", item.later());
        obj.array("files", |arr| self.files(arr, item));
    }

    fn blockers(&self, arr: &mut Seq<'_>, item: &Item) {
        let blockers =
            item.blockers().iter().filter_map(|&b| self.snap.item(b));
        for blocker in blockers {
            arr.item_raw(ref_json(&self.snap.ref_of(blocker)).as_str());
        }
    }

    fn files(&self, arr: &mut Seq<'_>, item: &Item) {
        for file in linked_files(self.snap, item, self.home) {
            arr.item_object(|obj| {
                obj.bytes("raw", file.raw);
                obj.bytes("path", file.path.as_os_str().as_bytes());
                obj.bool("exists", file.exists);
            });
        }
    }
}

/// The value of the first `key:` tag of `line`.
pub(crate) fn first_tag<'a>(line: &'a Line, key: &[u8]) -> Option<&'a [u8]> {
    line.tokens().find(|t| t.is_tag(key)).map(|t| t.value)
}

fn names(obj: &mut Seq<'_>, key: &str, line: &Line, kind: Kind) {
    obj.array(key, |arr| {
        for token in line.tokens().filter(|t| t.kind == kind) {
            arr.item_bytes(token.value);
        }
    });
}

fn tag_pairs(obj: &mut Seq<'_>, line: &Line) {
    obj.array("tags", |arr| {
        for token in line.tokens().filter(|t| t.kind == Kind::Tag) {
            arr.item_array(|pair| {
                pair.item_bytes(token.key);
                pair.item_bytes(token.value);
            });
        }
    });
}
