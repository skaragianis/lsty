//! The command table: names, aliases, kinds and the options each command
//! takes.

use lsty_core::config::Config;
use lsty_core::dates::Clock;
use lsty_core::discover::Found;
use lsty_core::errs;
use lsty_core::list::{self, Params, Snapshot};
use lsty_core::lock::Cancel;
use lsty_core::query::Query;

use crate::dispatch::USAGE_TEXT;
use crate::env::Env;
use crate::item_json::ItemJson;
use crate::output::Output;
use crate::parse::Parsed;

/// What a command does to the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Reads the list.
    Read,
    /// Changes the list.
    Write,
    /// Does not touch a list.
    Other,
}

/// What a handler of `help` or `tui` gets: no list is involved.
pub struct Bare<'a, 'e> {
    /// The command being run.
    pub command: &'static Command,
    /// The process's streams, variables and hooks.
    pub env: &'a mut Env<'e>,
    /// The command line.
    pub parsed: &'a Parsed,
}

/// What a handler of a command that uses a list gets once the shared
/// steps have succeeded.
pub struct Call<'a, 'e> {
    /// The command being run; `action` and messages use its name.
    pub command: &'static Command,
    /// The process's streams, variables and hooks.
    pub env: &'a mut Env<'e>,
    /// The command line.
    pub parsed: &'a Parsed,
    /// The configuration.
    pub config: Config,
    /// The clock of this run.
    pub clock: Clock,
    /// The todo and done files.
    pub found: Found,
    /// Whether `--create` allows a missing todo file.
    pub create: bool,
}

impl Call<'_, '_> {
    /// Whether the option was given.
    pub fn has(&self, name: &str) -> bool {
        self.parsed.opts.has(name)
    }

    /// The first value of the option.
    pub fn option(&self, name: &str) -> Option<&str> {
        self.parsed.opts.first(name)
    }

    /// Whether the output is the JSON envelope.
    pub fn json(&self) -> bool {
        self.has("json")
    }

    /// Reads both files under shared locks.
    pub fn load(&self) -> errs::Result<Snapshot> {
        list::load(
            &self.found,
            self.config.lock_timeout,
            &Cancel::new(),
            &Params {
                clock: self.clock.clone(),
                stale_after: self.config.claim_stale_after,
            },
        )
    }

    /// Renders the items of a snapshot as JSON objects and facts.
    pub fn item_json<'a>(&'a self, snap: &'a Snapshot) -> ItemJson<'a> {
        ItemJson {
            snap,
            home: self.env.home.as_deref(),
        }
    }

    pub(crate) fn parse_query(&self, args: &[String]) -> errs::Result<Query> {
        let filter = match self.option("filter") {
            Some(text) => Query::parse(text)?,
            None => Query::default(),
        };
        Ok(Query::and([filter, Query::parse_args(args)?]))
    }
}

/// Runs one command after the shared steps.
#[derive(Clone, Copy, Debug)]
pub enum Handler {
    /// A command that reads or writes a list.
    List(fn(&mut Call<'_, '_>) -> errs::Result<Output>),
    /// A command that touches no list.
    Bare(fn(&mut Bare<'_, '_>) -> errs::Result<Output>),
}

/// One row of the command table.
#[derive(Debug)]
pub struct Command {
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
    /// The options beyond the common ones.
    pub opts: &'static [&'static str],
    /// The handler, once the command is built.
    pub handler: Option<Handler>,
}

impl Command {
    /// Whether the command accepts `--option`.
    pub fn takes(&self, option: &str) -> bool {
        match option {
            "file" | "no-config" | "color" | "help" | "version" => true,
            "json" => !matches!(self.name, "tui" | "help" | "edit" | "open"),
            "owner" | "human" => !matches!(self.name, "tui" | "help"),
            "create" => self.kind == Kind::Write || self.name == "next",
            _ => self.opts.contains(&option),
        }
    }

    /// Whether `name`, in any case, is the command's name or an alias.
    fn answers_to(&self, name: &str) -> bool {
        self.name.eq_ignore_ascii_case(name)
            || self.aliases.iter().any(|a| a.eq_ignore_ascii_case(name))
    }
}

/// The synopsis of `tui`, which bare `lsty` and `lsty FILE` also use.
pub const TUI_SYNOPSIS: &str = "tui [FILE]";

const NONE: &[&str] = &[];
const EXPECT: &[&str] = &["expect"];
const TAKEOVER: &[&str] = &["expect", "takeover"];
const ADD: &[&str] = &[
    "top",
    "after",
    "before",
    "pri",
    "date",
    "no-date",
    "if-absent",
    "id",
    "expect",
];
const DO: &[&str] = &[
    "expect",
    "takeover",
    "ignore-blocked",
    "archive",
    "no-archive",
];
const LIST: &[&str] = &["filter", "sort", "later", "raw"];
const LISTED: &[&str] = &["filter", "later"];

const fn cmd(
    name: &'static str,
    aliases: &'static [&'static str],
    kind: Kind,
    synopsis: &'static str,
    opts: &'static [&'static str],
) -> Command {
    Command {
        name,
        aliases,
        kind,
        tty: false,
        synopsis,
        opts,
        handler: None,
    }
}

const fn tty(mut command: Command) -> Command {
    command.tty = true;
    command
}

const fn with(mut command: Command, handler: Handler) -> Command {
    command.handler = Some(handler);
    command
}

/// Describes the commands: for now, the usage text.
#[expect(clippy::unnecessary_wraps, reason = "the signature of a handler")]
fn help(_: &mut Bare<'_, '_>) -> errs::Result<Output> {
    Ok(Output {
        fields: Vec::new(),
        lines: USAGE_TEXT.lines().map(|l| l.as_bytes().to_vec()).collect(),
    })
}

const WRITE: Kind = Kind::Write;
const READ: Kind = Kind::Read;

/// Every command, in the order `lsty help` lists them.
pub static COMMANDS: [Command; 35] = [
    cmd("add", &["a"], WRITE, "add TEXT…", ADD),
    cmd("addm", &[], WRITE, "addm TEXT…", ADD),
    cmd("append", &["app"], WRITE, "append REFS TEXT…", EXPECT),
    cmd("prepend", &["prep"], WRITE, "prepend REFS TEXT…", EXPECT),
    cmd("replace", &[], WRITE, "replace REFS TEXT…", TAKEOVER),
    cmd("del", &["rm"], WRITE, "del REFS [TERM…]", TAKEOVER),
    cmd("do", &["done"], WRITE, "do REFS…", DO),
    cmd("pri", &["p"], WRITE, "pri REFS PRI [REFS PRI…]", EXPECT),
    cmd("depri", &["dp"], WRITE, "depri REFS…", EXPECT),
    with(
        cmd("ls", &["list"], READ, "ls [TERM…]", LIST),
        Handler::List(crate::read::ls),
    ),
    with(
        cmd("lsa", &["listall"], READ, "lsa [TERM…]", LIST),
        Handler::List(crate::read::lsa),
    ),
    with(
        cmd("lsp", &["listpri"], READ, "lsp [PRIORITIES] [TERM…]", LIST),
        Handler::List(crate::read::lsp),
    ),
    with(
        cmd("lsc", &["listcon"], READ, "lsc [TERM…]", LISTED),
        Handler::List(crate::read::lsc),
    ),
    with(
        cmd("lsprj", &["listproj"], READ, "lsprj [TERM…]", LISTED),
        Handler::List(crate::read::lsprj),
    ),
    cmd("archive", &[], WRITE, "archive", NONE),
    cmd("deduplicate", &[], WRITE, "deduplicate", NONE),
    with(
        cmd("help", &[], Kind::Other, "help [COMMAND…]", &["agent"]),
        Handler::Bare(help),
    ),
    cmd("reopen", &[], WRITE, "reopen REFS…", TAKEOVER),
    cmd("id", &[], WRITE, "id REFS [ID]", EXPECT),
    cmd("due", &[], WRITE, "due REFS DATE", EXPECT),
    cmd("tag", &[], WRITE, "tag REFS TAG…", EXPECT),
    cmd("untag", &[], WRITE, "untag REFS TAG…", EXPECT),
    cmd("up", &[], WRITE, "up REFS [N]", EXPECT),
    cmd("down", &[], WRITE, "down REFS [N]", EXPECT),
    cmd(
        "reorder",
        &[],
        WRITE,
        "reorder REFS POS | --after REF | --before REF",
        &["expect", "after", "before"],
    ),
    cmd("restore", &[], WRITE, "restore REFS…", TAKEOVER),
    tty(cmd("edit", &[], WRITE, "edit REF", TAKEOVER)),
    tty(cmd("open", &[], READ, "open REF [N]", EXPECT)),
    with(
        cmd("show", &[], READ, "show REF", &["raw", "expect"]),
        Handler::List(crate::show::show),
    ),
    cmd("dep", &[], WRITE, "dep REFS on REFS", &["expect", "id"]),
    cmd("undep", &[], WRITE, "undep REFS on REFS", EXPECT),
    cmd(
        "start",
        &[],
        WRITE,
        "start REFS…",
        &["expect", "takeover", "ignore-blocked"],
    ),
    cmd("stop", &[], WRITE, "stop REFS…", TAKEOVER),
    cmd(
        "next",
        &[],
        READ,
        "next [--start]",
        &["start", "filter", "sort"],
    ),
    cmd("tui", &[], Kind::Other, TUI_SYNOPSIS, NONE),
];

/// The command `name` stands for, in any case.
pub fn find(name: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|c| c.answers_to(name))
}
