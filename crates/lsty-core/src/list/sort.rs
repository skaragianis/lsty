use std::cmp::Reverse;

use jiff::Timestamp;

use super::snapshot::{Snapshot, Task};
use crate::{
    case::lower,
    errs::{self, Nr, Source},
    todotxt::Priority,
};

/// What `sort` orders tasks by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortField {
    /// Line number; `manual` is the same.
    #[default]
    File,
    /// Effective priority A to Z, then tasks without one.
    Priority,
    /// Earliest `due:` first, then tasks without a valid one.
    Due,
    /// Oldest creation date first, then tasks without one.
    Created,
    /// The description without regard to case, then by bytes.
    Alpha,
    /// Latest completion date first, a later `ended:` first within one
    /// date, then open tasks; done tasks of both files mix.
    Done,
}

impl SortField {
    /// The values `--sort` accepts, in the order they are listed.
    pub const NAMES: [&'static str; 7] = [
        "manual", "file", "priority", "due", "created", "alpha", "done",
    ];

    /// The field a `--sort` value names, in lower case.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "file" | "manual" => Some(Self::File),
            "priority" => Some(Self::Priority),
            "due" => Some(Self::Due),
            "created" => Some(Self::Created),
            "alpha" => Some(Self::Alpha),
            "done" => Some(Self::Done),
            _ => None,
        }
    }

    /// The name of the field as the TUI's status line shows it.
    pub const fn name(self) -> &'static str {
        match self {
            Self::File => "manual",
            Self::Priority => "priority",
            Self::Due => "due",
            Self::Created => "created",
            Self::Alpha => "alpha",
            Self::Done => "done, newest first",
        }
    }
}

/// The value one task is ordered by. Every key of one sort and rank is of
/// the same variant.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Key<'a> {
    Nr(Nr),
    Letter(bool, Option<u8>),
    Date(bool, Option<&'a str>),
    Text(String, &'a [u8]),
    Done(Reverse<Option<&'a str>>, Reverse<Option<Timestamp>>),
}

impl Snapshot {
    /// Orders `tasks`, which come from this snapshot, by `field`. Tasks of
    /// the todo file stay before those of the done file, except for done
    /// tasks in the done sort, and ties keep their order.
    pub fn sort<'a>(&'a self, tasks: &mut [&'a Task], field: SortField) {
        tasks.sort_by_cached_key(|task| {
            let key = self.sort_key(task, field);
            let rank = if matches!(key, Key::Done(..)) {
                0
            } else {
                1 + u8::from(task.source() == Source::Done)
            };
            (rank, key)
        });
    }

    fn sort_key<'a>(&'a self, task: &'a Task, field: SortField) -> Key<'a> {
        let line = self.line(task);
        match field {
            SortField::Done if !task.is_open() => {
                Key::Done(Reverse(line.completed()), Reverse(task.ended()))
            }
            SortField::File | SortField::Done => Key::Nr(task.nr()),
            SortField::Priority => {
                let letter = line.effective_priority().map(Priority::letter);
                Key::Letter(letter.is_none(), letter)
            }
            SortField::Due => {
                let due = line.tags().due;
                Key::Date(due.is_none(), due)
            }
            SortField::Created => {
                let created = line.created();
                Key::Date(created.is_none(), created)
            }
            SortField::Alpha => {
                let text = line.description();
                Key::Text(lower(&errs::lossy(text)), text)
            }
        }
    }
}
