//! The command table: names, aliases and the options each command
//! takes.

use std::path::Path;

use lsty_core::case::lower;
use lsty_core::errs;
use lsty_core::list::Snapshot;
use lsty_core::ops::{self, Outcome, Toggle};
use lsty_core::query::Query;

use crate::env::Env;
use crate::item_json::ItemJson;
use crate::output::Output;
use crate::paint::Painter;
use crate::parse::{Opt, Opts, Parsed, Scope};

/// What a handler of `help` or `tui` gets: no list is involved.
pub(crate) struct Bare<'a, 'e> {
    /// The process's streams, variables and hooks.
    pub env: &'a mut Env<'e>,
    /// The command line.
    pub parsed: &'a Parsed,
}

/// What a handler of a command that uses a list gets once the shared
/// steps have succeeded.
pub(crate) struct Call<'a> {
    /// The command being run; `action` and messages use its name.
    pub command: &'static Command,
    /// The home folder, for shortening paths.
    pub home: Option<&'a Path>,
    /// The command line.
    pub parsed: &'a Parsed,
    /// What operations run against: the files, the configuration, the
    /// clock and who is acting.
    pub ops: ops::Env<'a>,
    /// Colours the human output; off when stdout is not coloured.
    pub paint: Painter,
}

impl Call<'_> {
    /// Whether the option was given.
    pub(crate) fn has(&self, opt: Opt) -> bool {
        self.parsed.opts.has(opt)
    }

    /// On when `on` was given, off when `off` was, and the
    /// configuration's when neither.
    pub(crate) fn toggle(&self, on: Opt, off: Opt) -> Toggle {
        if self.has(on) {
            Toggle::On
        } else if self.has(off) {
            Toggle::Off
        } else {
            Toggle::Config
        }
    }

    /// The first value of the option.
    pub(crate) fn option(&self, opt: Opt) -> Option<&str> {
        self.parsed.opts.first(opt)
    }

    /// Whether the output is the JSON envelope.
    pub(crate) fn json(&self) -> bool {
        self.has(Opt::Json)
    }

    /// Reads both files under shared locks.
    pub(crate) fn load(&self) -> errs::Result<Snapshot> {
        self.ops.load()
    }

    /// Renders the items of a snapshot as JSON objects and facts.
    pub(crate) fn item_json<'a>(&'a self, snap: &'a Snapshot) -> ItemJson<'a> {
        ItemJson {
            snap,
            home: self.home,
        }
    }

    pub(crate) fn parse_query(&self, args: &[String]) -> errs::Result<Query> {
        let filter = match self.option(Opt::Filter) {
            Some(text) => Query::parse(text)?,
            None => Query::default(),
        };
        Ok(Query::and([filter, Query::parse_args(args)?]))
    }
}

/// The function of a command that hands back only what it prints.
pub(crate) type ReadHandler = fn(&Call<'_>) -> errs::Result<Output>;

/// What a write did, and what it prints.
pub(crate) struct Written {
    /// What the command prints.
    pub output: Output,
    /// What the operation did.
    pub outcome: Outcome,
}

/// The function of a command that also hands back the `Outcome`.
pub(crate) type WriteHandler = fn(&Call<'_>) -> errs::Result<Written>;

/// The function of a command that needs the process's editor, opener or
/// terminal.
pub(crate) type TtyHandler = fn(&Call<'_>, &Env<'_>) -> errs::Result<Output>;

/// The function of a command that touches no list.
pub(crate) type BareHandler = fn(&mut Bare<'_, '_>) -> errs::Result<Output>;

/// Runs one command after the shared steps.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Handler {
    /// Hands back only what it prints.
    Read(ReadHandler),
    /// Also hands back the `Outcome` of the operation.
    Write(WriteHandler),
    /// Needs the process's hooks, so only the process runs it.
    Tty(TtyHandler),
    /// A command that touches no list.
    Bare(BareHandler),
    /// The TUI, which the binary supplies.
    Tui,
}

/// One row of the command table.
#[derive(Debug)]
pub(crate) struct Command {
    /// The canonical name, which `action` and messages use.
    pub name: &'static str,
    /// Other names the command answers to.
    pub aliases: &'static [&'static str],
    /// Whether the shared guards treat the command as a write (`--create`,
    /// an identity without a terminal, ids for agents); `next --start` is
    /// checked on its own through `writes_with`.
    pub writes: bool,
    /// The text after `lsty ` in `usage: lsty …`.
    pub synopsis: &'static str,
    /// One sentence on what the command does.
    pub about: &'static str,
    /// The options beyond the common ones.
    pub opts: &'static [Opt],
    /// What runs the command.
    pub handler: Handler,
}

impl Command {
    /// Whether a caller that is not the process can run it: it needs no
    /// editor, opener, pager or terminal of its own.
    pub(crate) const fn inline(&self) -> bool {
        matches!(self.handler, Handler::Read(_) | Handler::Write(_))
    }

    /// Whether the command writes for these options: it is a write, or
    /// `next` with `--start`.
    pub(crate) fn writes_with(&self, opts: &Opts) -> bool {
        self.writes || opts.has(Opt::Start)
    }

    /// Whether the command accepts `--option`.
    pub(crate) fn takes(&self, option: Opt) -> bool {
        match option.scope() {
            Scope::Always => true,
            Scope::Json => self.inline(),
            Scope::Identity => {
                !matches!(self.handler, Handler::Bare(_) | Handler::Tui)
            }
            Scope::Create => self.writes || self.opts.contains(&Opt::Start),
            Scope::Own => self.opts.contains(&option),
        }
    }

    /// Whether `name`, in any case, is the command's name or an alias.
    pub(crate) fn answers_to(&self, name: &str) -> bool {
        let name = lower(name);
        self.name == name || self.aliases.contains(&name.as_str())
    }
}

/// The synopsis of `tui`, which bare `lsty` and `lsty FILE` also use.
pub(crate) const TUI_SYNOPSIS: &str = "tui [FILE]";

const ADD: &[Opt] = &[
    Opt::Top,
    Opt::After,
    Opt::Before,
    Opt::Pri,
    Opt::Date,
    Opt::NoDate,
    Opt::IfAbsent,
    Opt::Id,
    Opt::PrintId,
    Opt::Expect,
];
const DO: &[Opt] = &[
    Opt::Expect,
    Opt::Takeover,
    Opt::IgnoreBlocked,
    Opt::Archive,
    Opt::NoArchive,
];
const LIST: &[Opt] = &[Opt::Filter, Opt::Sort, Opt::Raw];
const LISTED: &[Opt] = &[Opt::Filter];

/// The `writes` column: `WRITE` for a command the shared guards treat as
/// a write, `UNGUARDED` for one they let pass.
const WRITE: bool = true;
const UNGUARDED: bool = false;

/// Every command, in the order `lsty help` lists them.
pub(crate) static COMMANDS: [Command; 35] = [
    Command {
        name: "add",
        aliases: &["a"],
        writes: WRITE,
        synopsis: "add TEXT…",
        about: "Adds an item at the end of the list.",
        opts: ADD,
        handler: Handler::Write(crate::add::add),
    },
    Command {
        name: "addm",
        aliases: &[],
        writes: WRITE,
        synopsis: "addm TEXT…",
        about: "Adds one item per line of the text, in one write.",
        opts: ADD,
        handler: Handler::Write(crate::add::addm),
    },
    Command {
        name: "append",
        aliases: &["app"],
        writes: WRITE,
        synopsis: "append REFS TEXT…",
        about: "Adds text at the end of each item.",
        opts: &[Opt::Expect],
        handler: Handler::Write(crate::edit::append),
    },
    Command {
        name: "prepend",
        aliases: &["prep"],
        writes: WRITE,
        synopsis: "prepend REFS TEXT…",
        about: "Puts text in front of the description of each item.",
        opts: &[Opt::Expect],
        handler: Handler::Write(crate::edit::prepend),
    },
    Command {
        name: "replace",
        aliases: &[],
        writes: WRITE,
        synopsis: "replace REFS TEXT…",
        about: "Gives each item new text and keeps its structure.",
        opts: &[Opt::Expect, Opt::Takeover],
        handler: Handler::Write(crate::edit::replace),
    },
    Command {
        name: "del",
        aliases: &["rm"],
        writes: WRITE,
        synopsis: "del REFS [TERM…]",
        about: "Deletes items, or a whole-word term in them.",
        opts: &[Opt::Expect, Opt::Takeover],
        handler: Handler::Write(crate::edit::del),
    },
    Command {
        name: "do",
        aliases: &["done"],
        writes: WRITE,
        synopsis: "do REFS…",
        about: "Completes items; recurring ones spawn the next.",
        opts: DO,
        handler: Handler::Write(crate::lifecycle::finish),
    },
    Command {
        name: "pri",
        aliases: &["p"],
        writes: WRITE,
        synopsis: "pri REFS PRI [REFS PRI…]",
        about: "Sets the priority of items.",
        opts: &[Opt::Expect],
        handler: Handler::Write(crate::edit::pri),
    },
    Command {
        name: "depri",
        aliases: &["dp"],
        writes: WRITE,
        synopsis: "depri REFS…",
        about: "Removes the priority of items.",
        opts: &[Opt::Expect],
        handler: Handler::Write(crate::edit::depri),
    },
    Command {
        name: "ls",
        aliases: &["list"],
        writes: UNGUARDED,
        synopsis: "ls [TERM…]",
        about: "Lists the open items.",
        opts: LIST,
        handler: Handler::Read(crate::read::ls),
    },
    Command {
        name: "lsa",
        aliases: &["listall"],
        writes: UNGUARDED,
        synopsis: "lsa [TERM…]",
        about: "Lists every item, done ones and the done file too.",
        opts: LIST,
        handler: Handler::Read(crate::read::lsa),
    },
    Command {
        name: "lsp",
        aliases: &["listpri"],
        writes: UNGUARDED,
        synopsis: "lsp [PRIORITIES] [TERM…]",
        about: "Lists the items that have a priority.",
        opts: LIST,
        handler: Handler::Read(crate::read::lsp),
    },
    Command {
        name: "lsc",
        aliases: &["listcon"],
        writes: UNGUARDED,
        synopsis: "lsc [TERM…]",
        about: "Lists the contexts in use.",
        opts: LISTED,
        handler: Handler::Read(crate::read::lsc),
    },
    Command {
        name: "lsprj",
        aliases: &["listproj"],
        writes: UNGUARDED,
        synopsis: "lsprj [TERM…]",
        about: "Lists the projects in use.",
        opts: LISTED,
        handler: Handler::Read(crate::read::lsprj),
    },
    Command {
        name: "archive",
        aliases: &[],
        writes: WRITE,
        synopsis: "archive",
        about: "Moves done items to the done file; removes todo blanks.",
        opts: &[],
        handler: Handler::Write(crate::lifecycle::archive),
    },
    Command {
        name: "deduplicate",
        aliases: &[],
        writes: WRITE,
        synopsis: "deduplicate",
        about: "Blanks the later copies of identical items.",
        opts: &[],
        handler: Handler::Write(crate::lifecycle::deduplicate),
    },
    Command {
        name: "help",
        aliases: &[],
        writes: UNGUARDED,
        synopsis: "help [COMMAND…]",
        about: "Describes the commands.",
        opts: &[Opt::Agent],
        handler: Handler::Bare(crate::help::help),
    },
    Command {
        name: "reopen",
        aliases: &[],
        writes: WRITE,
        synopsis: "reopen REFS…",
        about: "Returns done items to whoever finished them.",
        opts: &[Opt::Expect, Opt::Takeover],
        handler: Handler::Write(crate::lifecycle::reopen),
    },
    Command {
        name: "id",
        aliases: &[],
        writes: WRITE,
        synopsis: "id REFS [ID]",
        about: "Gives an item an id, or checks the one it has.",
        opts: &[Opt::Expect],
        handler: Handler::Write(crate::edit::id),
    },
    Command {
        name: "due",
        aliases: &[],
        writes: WRITE,
        synopsis: "due REFS DATE",
        about: "Sets the due date of items.",
        opts: &[Opt::Expect],
        handler: Handler::Write(crate::edit::due),
    },
    Command {
        name: "tag",
        aliases: &[],
        writes: WRITE,
        synopsis: "tag REFS TAG…",
        about: "Sets tags on items.",
        opts: &[Opt::Expect],
        handler: Handler::Write(crate::edit::tag),
    },
    Command {
        name: "untag",
        aliases: &[],
        writes: WRITE,
        synopsis: "untag REFS TAG…",
        about: "Removes tags from items.",
        opts: &[Opt::Expect],
        handler: Handler::Write(crate::edit::untag),
    },
    Command {
        name: "up",
        aliases: &[],
        writes: WRITE,
        synopsis: "up REFS [N]",
        about: "Moves items up the list.",
        opts: &[Opt::Expect],
        handler: Handler::Write(crate::moves::up),
    },
    Command {
        name: "down",
        aliases: &[],
        writes: WRITE,
        synopsis: "down REFS [N]",
        about: "Moves items down the list.",
        opts: &[Opt::Expect],
        handler: Handler::Write(crate::moves::down),
    },
    Command {
        name: "reorder",
        aliases: &[],
        writes: WRITE,
        synopsis: "reorder REFS POS | --after REF | --before REF",
        about: "Moves items to a position, or next to another item.",
        opts: &[Opt::Expect, Opt::After, Opt::Before],
        handler: Handler::Write(crate::moves::reorder),
    },
    Command {
        name: "restore",
        aliases: &[],
        writes: WRITE,
        synopsis: "restore REFS…",
        about: "Brings archived items back to the todo file.",
        opts: &[Opt::Expect, Opt::Takeover],
        handler: Handler::Write(crate::lifecycle::restore),
    },
    Command {
        name: "edit",
        aliases: &[],
        writes: WRITE,
        synopsis: "edit REF",
        about: "Edits one item in $EDITOR.",
        opts: &[Opt::Expect, Opt::Takeover],
        handler: Handler::Tty(crate::editor::edit),
    },
    Command {
        name: "open",
        aliases: &[],
        writes: UNGUARDED,
        synopsis: "open REF [N]",
        about: "Opens a file: link of an item.",
        opts: &[Opt::Expect],
        handler: Handler::Tty(crate::open::open),
    },
    Command {
        name: "show",
        aliases: &[],
        writes: UNGUARDED,
        synopsis: "show REF",
        about: "Shows one item with its facts.",
        opts: &[Opt::Raw, Opt::Expect],
        handler: Handler::Read(crate::show::show),
    },
    Command {
        name: "dep",
        aliases: &[],
        writes: WRITE,
        synopsis: "dep REFS on REFS",
        about: "Makes items wait for the prerequisites.",
        opts: &[Opt::Expect, Opt::Id],
        handler: Handler::Write(crate::deps::dep),
    },
    Command {
        name: "undep",
        aliases: &[],
        writes: WRITE,
        synopsis: "undep REFS on REFS",
        about: "Removes prerequisites from items.",
        opts: &[Opt::Expect],
        handler: Handler::Write(crate::deps::undep),
    },
    Command {
        name: "start",
        aliases: &[],
        writes: WRITE,
        synopsis: "start REFS…",
        about: "Claims items: they are underway and yours.",
        opts: &[Opt::Expect, Opt::Takeover, Opt::IgnoreBlocked],
        handler: Handler::Write(crate::lifecycle::start),
    },
    Command {
        name: "stop",
        aliases: &[],
        writes: WRITE,
        synopsis: "stop REFS…",
        about: "Ends a claim.",
        opts: &[Opt::Expect, Opt::Takeover],
        handler: Handler::Write(crate::lifecycle::stop),
    },
    Command {
        name: "next",
        aliases: &[],
        writes: UNGUARDED,
        synopsis: "next [--start]",
        about: "Shows the item to work on; --start claims it.",
        opts: &[Opt::Start, Opt::Filter, Opt::Sort, Opt::PrintId],
        handler: Handler::Read(crate::next::next),
    },
    Command {
        name: "tui",
        aliases: &[],
        writes: UNGUARDED,
        synopsis: TUI_SYNOPSIS,
        about: "Opens the list in the TUI.",
        opts: &[],
        handler: Handler::Tui,
    },
];

/// The `tui` row, the last of the table.
pub(crate) static TUI: &Command = &COMMANDS[34];

/// The command `name` stands for, in any case.
pub(crate) fn find(name: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|c| c.answers_to(name))
}
