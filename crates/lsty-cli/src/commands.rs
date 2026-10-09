//! The command table: names, aliases, kinds and the options each command
//! takes.

use lsty_core::case::lower;
use lsty_core::errs;
use lsty_core::list::Snapshot;
use lsty_core::ops::{self, Outcome, Toggle};
use lsty_core::query::Query;

use crate::env::Env;
use crate::item_json::ItemJson;
use crate::output::Output;
use crate::paint::Painter;
use crate::parse::{Opt, Parsed};

/// What a command does to the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// Reads the list.
    Read,
    /// Changes the list.
    Write,
    /// Does not touch a list.
    Other,
}

/// What a handler of `help` or `tui` gets: no list is involved.
pub(crate) struct Bare<'a, 'e> {
    /// The process's streams, variables and hooks.
    pub env: &'a mut Env<'e>,
    /// The command line.
    pub parsed: &'a Parsed,
}

/// What a handler of a command that uses a list gets once the shared
/// steps have succeeded.
pub(crate) struct Call<'a, 'e> {
    /// The command being run; `action` and messages use its name.
    pub command: &'static Command,
    /// The process's streams, variables and hooks.
    pub env: &'a mut Env<'e>,
    /// The command line.
    pub parsed: &'a Parsed,
    /// What operations run against: the files, the configuration, the
    /// clock and who is acting.
    pub ops: ops::Env<'a>,
    /// Colours the human output; off when stdout is not coloured.
    pub paint: Painter,
}

impl Call<'_, '_> {
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
            home: self.env.home.as_deref(),
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

/// The function of a command that reads a list.
pub(crate) type ReadHandler = fn(&mut Call<'_, '_>) -> errs::Result<Output>;

/// What a write did, and what it prints.
pub(crate) struct Written {
    /// What the command prints.
    pub output: Output,
    /// What the operation did.
    pub outcome: Outcome,
}

/// The function of a command that writes a list.
pub(crate) type WriteHandler = fn(&mut Call<'_, '_>) -> errs::Result<Written>;

/// The function of a command that touches no list.
pub(crate) type BareHandler = fn(&mut Bare<'_, '_>) -> errs::Result<Output>;

/// Runs one command after the shared steps.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Handler {
    /// A command that reads a list.
    Read(ReadHandler),
    /// A command that writes a list.
    Write(WriteHandler),
    /// A command that touches no list.
    Bare(BareHandler),
    /// The TUI, which the binary supplies.
    Tui,
}

/// The options that most commands take and a few do not.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Accepts {
    /// `--json`.
    pub json: bool,
    /// `--owner` and `--human`.
    pub identity: bool,
    /// `--create`.
    pub create: bool,
}

/// One row of the command table.
#[derive(Debug)]
pub(crate) struct Command {
    /// The canonical name, which `action` and messages use.
    pub name: &'static str,
    /// Other names the command answers to.
    pub aliases: &'static [&'static str],
    /// What the command does to the list.
    pub kind: Kind,
    /// Whether the command needs a terminal.
    pub tty: bool,
    /// The text after `lsty ` in `usage: lsty …`.
    pub synopsis: &'static str,
    /// One sentence on what the command does.
    pub about: &'static str,
    /// Which of the options that most commands take this one does.
    pub accepts: Accepts,
    /// The options beyond the common ones.
    pub opts: &'static [Opt],
    /// What runs the command.
    pub handler: Handler,
}

impl Command {
    /// Whether a caller that is not the process can run it: it needs no
    /// editor, opener, pager or terminal of its own.
    pub(crate) const fn inline(&self) -> bool {
        !self.tty
            && matches!(self.handler, Handler::Read(_) | Handler::Write(_))
    }

    /// Whether the command accepts `--option`.
    pub(crate) fn takes(&self, option: Opt) -> bool {
        match option {
            Opt::File
            | Opt::NoConfig
            | Opt::Color
            | Opt::Help
            | Opt::Version => true,
            Opt::Json => self.accepts.json,
            Opt::Owner | Opt::Human => self.accepts.identity,
            Opt::Create => self.accepts.create,
            Opt::After
            | Opt::Agent
            | Opt::Archive
            | Opt::Before
            | Opt::Date
            | Opt::Expect
            | Opt::Filter
            | Opt::Id
            | Opt::IfAbsent
            | Opt::IgnoreBlocked
            | Opt::NoArchive
            | Opt::NoDate
            | Opt::Pri
            | Opt::Raw
            | Opt::Sort
            | Opt::Start
            | Opt::Takeover
            | Opt::Top => self.opts.contains(&option),
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

const fn cmd(
    name: &'static str,
    aliases: &'static [&'static str],
    kind: Kind,
    synopsis: &'static str,
    handler: Handler,
) -> Command {
    Command {
        name,
        aliases,
        kind,
        tty: false,
        accepts: Accepts {
            json: true,
            identity: true,
            create: matches!(kind, Kind::Write),
        },
        synopsis,
        about: "",
        opts: &[],
        handler,
    }
}

const fn about(mut command: Command, text: &'static str) -> Command {
    command.about = text;
    command
}

const fn with_opts(mut command: Command, opts: &'static [Opt]) -> Command {
    command.opts = opts;
    command
}

const fn tty(mut command: Command) -> Command {
    command.tty = true;
    command
}

const fn no_json(mut command: Command) -> Command {
    command.accepts.json = false;
    command
}

const fn no_identity(mut command: Command) -> Command {
    command.accepts.identity = false;
    command
}

const fn creates(mut command: Command) -> Command {
    command.accepts.create = true;
    command
}

const WRITE: Kind = Kind::Write;
const READ: Kind = Kind::Read;

/// Every command, in the order `lsty help` lists them.
pub(crate) static COMMANDS: [Command; 35] = [
    about(
        with_opts(
            cmd(
                "add",
                &["a"],
                WRITE,
                "add TEXT…",
                Handler::Write(crate::add::add),
            ),
            ADD,
        ),
        "Adds an item at the end of the list.",
    ),
    about(
        with_opts(
            cmd(
                "addm",
                &[],
                WRITE,
                "addm TEXT…",
                Handler::Write(crate::add::addm),
            ),
            ADD,
        ),
        "Adds one item per line of the text, in one write.",
    ),
    about(
        with_opts(
            cmd(
                "append",
                &["app"],
                WRITE,
                "append REFS TEXT…",
                Handler::Write(crate::edit::append),
            ),
            &[Opt::Expect],
        ),
        "Adds text at the end of each item.",
    ),
    about(
        with_opts(
            cmd(
                "prepend",
                &["prep"],
                WRITE,
                "prepend REFS TEXT…",
                Handler::Write(crate::edit::prepend),
            ),
            &[Opt::Expect],
        ),
        "Puts text in front of the description of each item.",
    ),
    about(
        with_opts(
            cmd(
                "replace",
                &[],
                WRITE,
                "replace REFS TEXT…",
                Handler::Write(crate::edit::replace),
            ),
            &[Opt::Expect, Opt::Takeover],
        ),
        "Gives each item new text and keeps its structure.",
    ),
    about(
        with_opts(
            cmd(
                "del",
                &["rm"],
                WRITE,
                "del REFS [TERM…]",
                Handler::Write(crate::edit::del),
            ),
            &[Opt::Expect, Opt::Takeover],
        ),
        "Deletes items, or a whole-word term in them.",
    ),
    about(
        with_opts(
            cmd(
                "do",
                &["done"],
                WRITE,
                "do REFS…",
                Handler::Write(crate::lifecycle::finish),
            ),
            DO,
        ),
        "Completes items; recurring ones spawn the next.",
    ),
    about(
        with_opts(
            cmd(
                "pri",
                &["p"],
                WRITE,
                "pri REFS PRI [REFS PRI…]",
                Handler::Write(crate::edit::pri),
            ),
            &[Opt::Expect],
        ),
        "Sets the priority of items.",
    ),
    about(
        with_opts(
            cmd(
                "depri",
                &["dp"],
                WRITE,
                "depri REFS…",
                Handler::Write(crate::edit::depri),
            ),
            &[Opt::Expect],
        ),
        "Removes the priority of items.",
    ),
    about(
        with_opts(
            cmd(
                "ls",
                &["list"],
                READ,
                "ls [TERM…]",
                Handler::Read(crate::read::ls),
            ),
            LIST,
        ),
        "Lists the open items.",
    ),
    about(
        with_opts(
            cmd(
                "lsa",
                &["listall"],
                READ,
                "lsa [TERM…]",
                Handler::Read(crate::read::lsa),
            ),
            LIST,
        ),
        "Lists every item, done ones and the done file too.",
    ),
    about(
        with_opts(
            cmd(
                "lsp",
                &["listpri"],
                READ,
                "lsp [PRIORITIES] [TERM…]",
                Handler::Read(crate::read::lsp),
            ),
            LIST,
        ),
        "Lists the items that have a priority.",
    ),
    about(
        with_opts(
            cmd(
                "lsc",
                &["listcon"],
                READ,
                "lsc [TERM…]",
                Handler::Read(crate::read::lsc),
            ),
            LISTED,
        ),
        "Lists the contexts in use.",
    ),
    about(
        with_opts(
            cmd(
                "lsprj",
                &["listproj"],
                READ,
                "lsprj [TERM…]",
                Handler::Read(crate::read::lsprj),
            ),
            LISTED,
        ),
        "Lists the projects in use.",
    ),
    about(
        cmd(
            "archive",
            &[],
            WRITE,
            "archive",
            Handler::Write(crate::lifecycle::archive),
        ),
        "Moves done items to the done file; removes todo blanks.",
    ),
    about(
        cmd(
            "deduplicate",
            &[],
            WRITE,
            "deduplicate",
            Handler::Write(crate::lifecycle::deduplicate),
        ),
        "Blanks the later copies of identical items.",
    ),
    no_identity(no_json(about(
        with_opts(
            cmd(
                "help",
                &[],
                Kind::Other,
                "help [COMMAND…]",
                Handler::Bare(crate::help::help),
            ),
            &[Opt::Agent],
        ),
        "Describes the commands.",
    ))),
    about(
        with_opts(
            cmd(
                "reopen",
                &[],
                WRITE,
                "reopen REFS…",
                Handler::Write(crate::lifecycle::reopen),
            ),
            &[Opt::Expect, Opt::Takeover],
        ),
        "Returns done items to whoever finished them.",
    ),
    about(
        with_opts(
            cmd(
                "id",
                &[],
                WRITE,
                "id REFS [ID]",
                Handler::Write(crate::edit::id),
            ),
            &[Opt::Expect],
        ),
        "Gives an item an id, or checks the one it has.",
    ),
    about(
        with_opts(
            cmd(
                "due",
                &[],
                WRITE,
                "due REFS DATE",
                Handler::Write(crate::edit::due),
            ),
            &[Opt::Expect],
        ),
        "Sets the due date of items.",
    ),
    about(
        with_opts(
            cmd(
                "tag",
                &[],
                WRITE,
                "tag REFS TAG…",
                Handler::Write(crate::edit::tag),
            ),
            &[Opt::Expect],
        ),
        "Sets tags on items.",
    ),
    about(
        with_opts(
            cmd(
                "untag",
                &[],
                WRITE,
                "untag REFS TAG…",
                Handler::Write(crate::edit::untag),
            ),
            &[Opt::Expect],
        ),
        "Removes tags from items.",
    ),
    about(
        with_opts(
            cmd(
                "up",
                &[],
                WRITE,
                "up REFS [N]",
                Handler::Write(crate::moves::up),
            ),
            &[Opt::Expect],
        ),
        "Moves items up the list.",
    ),
    about(
        with_opts(
            cmd(
                "down",
                &[],
                WRITE,
                "down REFS [N]",
                Handler::Write(crate::moves::down),
            ),
            &[Opt::Expect],
        ),
        "Moves items down the list.",
    ),
    about(
        with_opts(
            cmd(
                "reorder",
                &[],
                WRITE,
                "reorder REFS POS | --after REF | --before REF",
                Handler::Write(crate::moves::reorder),
            ),
            &[Opt::Expect, Opt::After, Opt::Before],
        ),
        "Moves items to a position, or next to another item.",
    ),
    about(
        with_opts(
            cmd(
                "restore",
                &[],
                WRITE,
                "restore REFS…",
                Handler::Write(crate::lifecycle::restore),
            ),
            &[Opt::Expect, Opt::Takeover],
        ),
        "Brings archived items back to the todo file.",
    ),
    no_json(tty(about(
        with_opts(
            cmd(
                "edit",
                &[],
                WRITE,
                "edit REF",
                Handler::Read(crate::editor::edit),
            ),
            &[Opt::Expect, Opt::Takeover],
        ),
        "Edits one item in $EDITOR.",
    ))),
    no_json(tty(about(
        with_opts(
            cmd(
                "open",
                &[],
                READ,
                "open REF [N]",
                Handler::Read(crate::open::open),
            ),
            &[Opt::Expect],
        ),
        "Opens a file: link of an item.",
    ))),
    about(
        with_opts(
            cmd(
                "show",
                &[],
                READ,
                "show REF",
                Handler::Read(crate::show::show),
            ),
            &[Opt::Raw, Opt::Expect],
        ),
        "Shows one item with its facts.",
    ),
    about(
        with_opts(
            cmd(
                "dep",
                &[],
                WRITE,
                "dep REFS on REFS",
                Handler::Write(crate::deps::dep),
            ),
            &[Opt::Expect, Opt::Id],
        ),
        "Makes items wait for the prerequisites.",
    ),
    about(
        with_opts(
            cmd(
                "undep",
                &[],
                WRITE,
                "undep REFS on REFS",
                Handler::Write(crate::deps::undep),
            ),
            &[Opt::Expect],
        ),
        "Removes prerequisites from items.",
    ),
    about(
        with_opts(
            cmd(
                "start",
                &[],
                WRITE,
                "start REFS…",
                Handler::Write(crate::lifecycle::start),
            ),
            &[Opt::Expect, Opt::Takeover, Opt::IgnoreBlocked],
        ),
        "Claims items: they are underway and yours.",
    ),
    about(
        with_opts(
            cmd(
                "stop",
                &[],
                WRITE,
                "stop REFS…",
                Handler::Write(crate::lifecycle::stop),
            ),
            &[Opt::Expect, Opt::Takeover],
        ),
        "Ends a claim.",
    ),
    creates(about(
        with_opts(
            cmd(
                "next",
                &[],
                READ,
                "next [--start]",
                Handler::Read(crate::next::next),
            ),
            &[Opt::Start, Opt::Filter, Opt::Sort],
        ),
        "Shows the item to work on; --start claims it.",
    )),
    no_identity(no_json(about(
        cmd("tui", &[], Kind::Other, TUI_SYNOPSIS, Handler::Tui),
        "Opens the list in the TUI.",
    ))),
];

/// The command `name` stands for, in any case.
pub(crate) fn find(name: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|c| c.answers_to(name))
}
