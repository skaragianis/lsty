//! The item object of the JSON output.

use std::{os::unix::ffi::OsStrExt as _, path::Path};

use jiff::Timestamp;
use lsty_core::{
    links::linked_files,
    list::{Item, Snapshot},
    todotxt::{Kind, Line, Tags},
};

use crate::{
    jsonw::{self, Arr, Obj},
    output::{Json, ref_json},
};

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
                arr.object(|obj| self.write(obj, item));
            }
        });
        Json::raw(out)
    }

    fn write(&self, obj: &mut Obj<'_>, item: &Item) {
        let tags = self.snap.line(item).tags();
        self.identity(obj, item);
        self.dates_and_tags(obj, item, &tags);
        self.state(obj, item);
    }

    fn identity(&self, obj: &mut Obj<'_>, item: &Item) {
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

    fn dates_and_tags(&self, obj: &mut Obj<'_>, item: &Item, tags: &Tags<'_>) {
        let line = self.snap.line(item);
        let rec = rec_text(line, tags);
        let instant = |at: Option<Timestamp>| {
            at.map(|t| t.strftime("%Y-%m-%dT%H:%M:%SZ").to_string())
        };
        obj.opt_string("due", tags.due);
        obj.opt_string("threshold", tags.threshold);
        obj.opt_bytes("rec", rec);
        obj.opt_bytes("id", tags.id);
        obj.array("deps", |arr| {
            for dep in &tags.deps {
                arr.bytes(dep);
            }
        });
        obj.opt_string("started", instant(item.started()).as_deref());
        obj.opt_string("ended", instant(item.ended()).as_deref());
        obj.opt_bytes("owner", tags.owner);
    }

    fn state(&self, obj: &mut Obj<'_>, item: &Item) {
        obj.string("state", item.state().as_str());
        obj.bool("blocked", item.blocked());
        obj.array("blockers", |arr| self.blockers(arr, item));
        obj.array("dangling", |arr| {
            for id in self.snap.dangling(item) {
                arr.bytes(id);
            }
        });
        obj.bool("cycle", item.cycle());
        obj.bool("later", item.later());
        obj.array("files", |arr| self.files(arr, item));
    }

    fn blockers(&self, arr: &mut Arr<'_>, item: &Item) {
        let blockers =
            item.blockers().iter().filter_map(|&b| self.snap.item(b));
        for blocker in blockers {
            arr.raw(ref_json(&self.snap.ref_of(blocker)).as_str());
        }
    }

    fn files(&self, arr: &mut Arr<'_>, item: &Item) {
        for file in linked_files(self.snap, item, self.home) {
            arr.object(|obj| {
                obj.bytes("raw", &file.raw);
                obj.bytes("path", file.path.as_os_str().as_bytes());
                obj.bool("exists", file.path.exists());
            });
        }
    }
}

/// The text of the first `rec:` tag of `line`, if its value parses.
pub(crate) fn rec_text<'a>(
    line: &'a Line,
    tags: &Tags<'_>,
) -> Option<&'a [u8]> {
    tags.rec?;
    line.tokens().find(|t| t.is_tag(b"rec")).map(|t| t.value)
}

fn names(obj: &mut Obj<'_>, key: &str, line: &Line, kind: Kind) {
    obj.array(key, |arr| {
        for token in line.tokens().filter(|t| t.kind == kind) {
            arr.bytes(token.value);
        }
    });
}

fn tag_pairs(obj: &mut Obj<'_>, line: &Line) {
    obj.array("tags", |arr| {
        for token in line.tokens().filter(|t| t.kind == Kind::Tag) {
            arr.array(|pair| {
                pair.bytes(token.key);
                pair.bytes(token.value);
            });
        }
    });
}
