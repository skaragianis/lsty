//! The item object of the JSON output, and the facts about a file link
//! that it and `show` share.

use std::os::unix::ffi::OsStrExt as _;
use std::path::Path;

use lsty_core::list::{Item, Snapshot};
use lsty_core::todotxt::{Kind, Line, Tags};

use crate::linkfiles::{LinkedFile, linked_files};
use crate::output::{Field, Json, ref_json};

/// What an item is rendered against: its snapshot and the home folder
/// that `file:~/…` links lie under.
pub struct ItemJson<'a> {
    /// The snapshot the items come from.
    pub snap: &'a Snapshot,
    /// The home folder, when there is one.
    pub home: Option<&'a Path>,
}

impl ItemJson<'_> {
    /// The JSON objects of `items`, in order.
    pub fn objects(&self, items: &[&Item]) -> Json {
        Json::array(items.iter().map(|i| self.object(i)))
    }

    /// The JSON object of one item.
    pub fn object(&self, item: &Item) -> Json {
        let line = self.snap.line(item);
        let tags = line.tags();
        let mut fields = self.identity(item);
        fields.extend(self.dates_and_tags(item, &tags));
        fields.extend(self.state(item));
        Json::object(&fields)
    }

    fn identity(&self, item: &Item) -> Vec<Field> {
        let line = self.snap.line(item);
        let priority = line.effective_priority().map(|p| p.to_string());
        vec![
            Field::int("nr", i64::from(item.nr().get())),
            Field::string("source", item.source().as_str()),
            Field::new("raw", Json::bytes(line.raw())),
            Field::bool("done", line.is_done()),
            Field::opt_string("priority", priority.as_deref()),
            Field::opt_string("created", line.created()),
            Field::opt_string("completed", line.completed()),
            Field::new("description", Json::bytes(line.description())),
            Field::new("text", Json::bytes(&line.text())),
            names("projects", line, Kind::Project),
            names("contexts", line, Kind::Context),
            tag_pairs(line),
        ]
    }

    fn dates_and_tags(&self, item: &Item, tags: &Tags<'_>) -> Vec<Field> {
        let line = self.snap.line(item);
        let rec = tags.rec.and_then(|_| first_tag(line, b"rec"));
        let started = item
            .started()
            .map(|t| t.strftime("%Y-%m-%dT%H:%M:%SZ").to_string());
        vec![
            Field::opt_string("due", tags.due),
            Field::opt_string("threshold", tags.threshold),
            Field::new("rec", rec.map_or_else(Json::null, Json::bytes)),
            Field::new("id", non_empty(tags.id)),
            Field::array("deps", tags.deps.iter().map(|d| Json::bytes(d))),
            Field::opt_string("started", started.as_deref()),
            Field::new("owner", non_empty(tags.owner)),
        ]
    }

    fn state(&self, item: &Item) -> Vec<Field> {
        let blockers = item
            .blockers()
            .iter()
            .filter_map(|&b| self.snap.item(b))
            .map(|b| ref_json(&self.snap.ref_of(b)));
        let files = linked_files(self.snap, item, self.home)
            .into_iter()
            .map(|file| file_json(&file));
        vec![
            Field::string("state", item.state().as_str()),
            Field::bool("blocked", item.blocked()),
            Field::array("blockers", blockers),
            Field::array(
                "dangling",
                self.snap.dangling(item).map(Json::bytes),
            ),
            Field::bool("cycle", item.cycle()),
            Field::bool("later", item.later()),
            Field::array("files", files),
        ]
    }
}

fn file_json(file: &LinkedFile<'_>) -> Json {
    Json::object(&[
        Field::new("raw", Json::bytes(file.raw)),
        Field::new("path", Json::bytes(file.path.as_os_str().as_bytes())),
        Field::bool("exists", file.exists),
    ])
}

/// The value of the first `key:` tag of `line`.
pub fn first_tag<'a>(line: &'a Line, key: &[u8]) -> Option<&'a [u8]> {
    line.tokens()
        .find(|t| t.kind == Kind::Tag && t.key == key)
        .map(|t| t.value)
}

fn non_empty(value: Option<&[u8]>) -> Json {
    value
        .filter(|v| !v.is_empty())
        .map_or_else(Json::null, Json::bytes)
}

fn names(key: &'static str, line: &Line, kind: Kind) -> Field {
    let values = line.tokens().filter(|t| t.kind == kind);
    Field::array(key, values.map(|t| Json::bytes(t.value)))
}

fn tag_pairs(line: &Line) -> Field {
    let tags = line.tokens().filter(|t| t.kind == Kind::Tag);
    let pair = |key: &[u8], value: &[u8]| {
        Json::array([Json::bytes(key), Json::bytes(value)])
    };
    Field::array("tags", tags.map(|t| pair(t.key, t.value)))
}
