//! Random command sequences run through `lsty` and through todo.sh, the
//! original todo.txt command line. After every command the todo file and
//! the done file must agree byte for byte, and both programs must agree on
//! whether the command worked. Apart from the list commands (below), what
//! they print is never compared.
//!
//! The test runs only when `TODO_SH` names the todo.sh program (`just
//! diff RUNS SEED` sets it). `LSTY_DIFF_RUNS` sets the number of
//! sequences (default 1000) and `LSTY_DIFF_SEED` the seed of the first one
//! (default 1); sequence `i` uses the seed `LSTY_DIFF_SEED + i`, so `just
//! diff 1 N` repeats sequence `N`. `LSTY_DIFF_LINES` (`just diff RUNS
//! SEED LINES`) starts every sequence from a todo file and a done file of
//! that many random lines each, to time the programs on a long list;
//! without it the starting files are short and their length random. A
//! value that is not a number is an error. `LSTY_DIFF_ALL=1` goes on after
//! a variance and reports the first of each kind, to survey them.
//!
//! Every run ends with a table of call times: for each command and each
//! program, the number of calls and the 50th, 90th, 95th and 99th
//! percentile in milliseconds, then the same over all commands. `just
//! diff` builds in release mode, so the times are a release build's.
//!
//! After every command both programs also run one of the list commands (`ls`,
//! `lsa`, `lsp`, `lsc`, `lsprj`) on the same files, timed like the rest. Both
//! must agree on success, leave the files as they were, and show the same
//! items: todo items by line number and text, done items by text, names as
//! they are, in any order and without the footer. For `lsa` texts are compared
//! with every run of white space as one space and none at the ends, as
//! todo.sh's `lsa` prints them. Two todo.sh bugs are kept out: it never lists
//! an item whose text is only digits and spaces, and its `lsa` joins the todo
//! file's last line to the done file's first when the todo file does not end
//! with a newline (that `lsa` is not compared). The list commands are drawn
//! from a random stream of their own, so a seed's sequence of commands stays
//! the same.
//!
//! # What is left out, and why
//!
//! Three things keep a difference out of the comparison: the generator
//! avoids it, a narrow rule in `rules.rs` settles it (each rule has a
//! one-line reason), or, for the list commands, `compare_read` normalises
//! or skips it (see above). "Deliberate" means lsty does it on purpose;
//! "undecided" means nobody has said whether it is right.
//!
//! Settings (see `setup`):
//! - lsty runs with `keep_priority = false`, so lsty's default `do` (which
//!   keeps the letter as ` pri:X`) is not compared. Deliberate; todo.sh has
//!   no such setting.
//!
//! In the generator:
//! - no relative dates (`+1w`, `3d`) in `due:` or `t:`: lsty writes the
//!   date, todo.sh the text. Deliberate.
//! - no `rec:` tag: lsty refuses a value that is not a recurrence.
//!   Deliberate.
//! - no invalid dates such as `2026-13-45`: todo.sh reads them as creation
//!   dates, lsty as words. Undecided.
//! - the first word of added, replaced or prepended text is never a date,
//!   and a replacement has no date in front: lsty refuses a text edit that
//!   would make a date the creation date. Undecided.
//! - no invalid `due:`/`t:` value in a starting file, only in added or
//!   appended text (see `refused_bad_date_tag`). Undecided: what lsty does
//!   with such a line already on disk.
//! - `del N TERM` terms are single tokens, never text with a space in
//!   it. todo.sh deletes the term wherever it occurs, inside words too;
//!   lsty only where it is a whole word, on purpose (`del_whole_words`).
//! - a `del N TERM` term and the other commands' numbers are chosen from
//!   the lsty copy of the list, which is the same as todo.sh's after every
//!   step.
//! - list arguments (`do 1,2`) name distinct lines that are not empty,
//!   because todo.sh stops at the first empty line and lsty skips it.
//!   Undecided. Lines of white space only are in the lists: they are
//!   settled by `nothing_to_do` and the `do` view.
//!
//! Settled by rules: see `rules.rs`; each rule names its reason.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use libtest_mimic::{Arguments, Failed, Trial};
use lsty_core::errs::EXIT_LOCK_TIMEOUT;

/// `write!` and `writeln!` to a `String`, which cannot fail.
macro_rules! put {
    ($out:expr, $($arg:tt)*) => {
        $out.write_fmt(format_args!($($arg)*)).unwrap_or_default()
    };
}

macro_rules! putln {
    ($out:expr, $($arg:tt)*) => {
        writeln!($out, $($arg)*).unwrap_or_default()
    };
}

#[path = "todo_sh/random.rs"]
mod random;
#[path = "todo_sh/rules.rs"]
mod rules;
#[path = "todo_sh/timings.rs"]
mod timings;

use random::{
    DATES, Rng, SYSTEM_PATH, bad_tag, file_text, host_var, letter, number,
    quote, token, words,
};
use rules::{Ctx, explained, targets};
use timings::Timings;

/// The day both programs believe it is.
const DAY: &str = "2026-09-29";
const NOW: &str = "2026-09-29T0900Z";

/// The text of a new item (`new`) or of a replacement: words, and
/// sometimes a priority in front. A new item may start with a date or with
/// `x `, and either text may end in a tag with a value that is not a date
/// (each one is a refusal rule). No relative dates: see the module doc.
fn item_text(rng: &mut Rng, new: bool) -> String {
    let mut text = String::new();
    if new && rng.one_in(10) {
        text.push_str("x ");
    }
    if rng.one_in(4) {
        put!(text, "({}) ", letter(rng));
    }
    if new && rng.one_in(4) {
        put!(text, "{} ", rng.pick(DATES));
    }
    text.push_str(&words(rng, 6));
    if let Some(tag) = bad_tag(rng) {
        put!(text, " {tag}");
    }
    text
}

// -------------------------------------------------------------- commands

/// One command both programs have, with the same arguments.
enum Cmd {
    Add(String),
    Append(usize, String),
    Prepend(usize, String),
    Replace(usize, String),
    Pri(usize, char),
    Depri(Vec<usize>),
    Do(Vec<usize>),
    Addm(Vec<String>),
    Del(usize),
    DelTerm(usize, String),
    Archive,
    Deduplicate,
}

impl Cmd {
    /// The command's name in the timings table; its first word is the
    /// command lsty and todo.sh run.
    fn name(&self) -> &'static str {
        match self {
            Self::Add(_) => "add",
            Self::Append(..) => "append",
            Self::Prepend(..) => "prepend",
            Self::Replace(..) => "replace",
            Self::Pri(..) => "pri",
            Self::Depri(_) => "depri",
            Self::Do(_) => "do",
            Self::Addm(_) => "addm",
            Self::Del(_) => "del",
            Self::DelTerm(..) => "del term",
            Self::Archive => "archive",
            Self::Deduplicate => "deduplicate",
        }
    }

    fn args(&self) -> Vec<String> {
        let nr = |n: &usize| n.to_string();
        let list = |n: &[usize]| {
            n.iter().map(usize::to_string).collect::<Vec<_>>().join(",")
        };
        let mut words = vec![self.name().trim_end_matches(" term").to_owned()];
        words.extend(match self {
            Self::Add(t) => vec![t.clone()],
            Self::Append(n, t)
            | Self::Prepend(n, t)
            | Self::Replace(n, t)
            | Self::DelTerm(n, t) => vec![nr(n), t.clone()],
            Self::Pri(n, p) => vec![nr(n), p.to_string()],
            Self::Depri(n) | Self::Do(n) => vec![list(n)],
            Self::Addm(t) => vec![t.join("\n")],
            Self::Del(n) => vec![nr(n)],
            Self::Archive | Self::Deduplicate => vec![],
        });
        words
    }

    /// The command as one shell line.
    fn shell(&self) -> String {
        let words: Vec<String> =
            self.args().iter().map(|a| quote(a)).collect();
        words.join(" ")
    }
}

/// A line number: mostly in range, sometimes just outside it.
fn line_nr(rng: &mut Rng, lines: usize) -> usize {
    if rng.one_in(12) {
        rng.below(lines + 3)
    } else {
        1 + rng.below(lines.max(1))
    }
}

/// A term to remove: a word of the item, a part of one (it then occurs
/// inside a word), or any token.
fn term(rng: &mut Rng, line: &str) -> String {
    let own: Vec<&str> = line.split(' ').filter(|w| !w.is_empty()).collect();
    if own.is_empty() || rng.one_in(5) {
        return token(rng);
    }
    let word = own.get(rng.below(own.len())).copied().unwrap_or("");
    if rng.one_in(3) {
        let chars: Vec<char> = word.chars().collect();
        let from = rng.below(chars.len());
        let to = from + 1 + rng.below(chars.len() - from);
        let part: String =
            chars.get(from..to).unwrap_or_default().iter().collect();
        // a term that starts with a dash is an option to lsty
        if !part.starts_with('-') {
            return part;
        }
    }
    word.to_owned()
}

/// Two or three different lines that hold an item.
fn item_list(rng: &mut Rng, lines: &[&str]) -> Option<Vec<usize>> {
    let mut open: Vec<usize> = (1..=lines.len())
        .filter(|n| lines.get(n - 1).is_some_and(|l| !l.is_empty()))
        .collect();
    if open.len() < 2 {
        return None;
    }
    let mut list = Vec::new();
    for _ in 0..2 + rng.below(2) {
        if open.is_empty() {
            break;
        }
        list.push(open.remove(rng.below(open.len())));
    }
    Some(list)
}

/// Text to append: now and then it starts with a mark that todo.sh does
/// not put a space before.
fn tail(rng: &mut Rng) -> String {
    let mut text = words(rng, 3);
    if let Some(tag) = bad_tag(rng) {
        put!(text, " {tag}");
    }
    if rng.one_in(6) {
        format!("{}{text}", rng.pick(&[",", ".", ":", ";"]))
    } else {
        text
    }
}

fn command(rng: &mut Rng, todo: &[u8]) -> Cmd {
    let text = String::from_utf8_lossy(todo).into_owned();
    let lines: Vec<&str> = text.lines().collect();
    let n = line_nr(rng, lines.len());
    let line = n.checked_sub(1).and_then(|i| lines.get(i)).unwrap_or(&"");
    let list = if rng.one_in(6) {
        item_list(rng, &lines)
    } else {
        None
    };
    match rng.below(21) {
        0..=3 => Cmd::Add(item_text(rng, true)),
        4 => Cmd::Addm(
            (0..2 + rng.below(2))
                .map(|_| item_text(rng, true))
                .collect(),
        ),
        5 | 6 => Cmd::Append(n, tail(rng)),
        7 | 8 => Cmd::Prepend(n, words(rng, 3)),
        9 => Cmd::Replace(n, item_text(rng, false)),
        10 | 11 => Cmd::Pri(n, letter(rng)),
        12 => Cmd::Depri(list.unwrap_or_else(|| vec![n])),
        13..=15 => Cmd::Do(list.unwrap_or_else(|| vec![n])),
        16 => Cmd::Del(n),
        17 | 18 => Cmd::DelTerm(n, term(rng, line)),
        19 => Cmd::Archive,
        _ => Cmd::Deduplicate,
    }
}

// -------------------------------------------------------------- programs

/// What one program left behind after a command.
#[derive(Clone, PartialEq)]
struct Files {
    /// `None` when the file does not exist.
    todo: Option<Vec<u8>>,
    done: Option<Vec<u8>>,
}

struct Step {
    ok: bool,
    /// How the program died, when it did not just exit.
    crash: Option<String>,
    files: Files,
    /// What the program printed on stdout.
    out: String,
    log: String,
    /// How long the program ran.
    took: Duration,
}

/// One program's folder and the way to run it.
struct Side {
    name: &'static str,
    /// Whether this is lsty (the other side is todo.sh).
    lsty: bool,
    dir: PathBuf,
    program: PathBuf,
    shim: PathBuf,
}

impl Side {
    fn todo(&self) -> PathBuf {
        self.dir.join("todo.txt")
    }

    fn done(&self) -> PathBuf {
        self.dir.join("done.txt")
    }

    fn set(&self, files: &Files) -> Result<(), String> {
        put_file(&self.todo(), files.todo.as_deref())?;
        put_file(&self.done(), files.done.as_deref())
    }

    fn read(&self) -> Result<Files, String> {
        Ok(Files {
            todo: read_file(&self.todo())?,
            done: read_file(&self.done())?,
        })
    }

    fn command(&self, args: &[String]) -> Command {
        let home = self.dir.join("home");
        let mut cmd = Command::new(&self.program);
        cmd.env_clear()
            .current_dir(&self.dir)
            .env("HOME", &home)
            .env("TZ", "UTC")
            .env("PATH", format!("{}:{SYSTEM_PATH}", self.shim.display()));
        if self.lsty {
            cmd.arg("--human")
                .env("LSTY_NOW", NOW)
                .env("TODO_FILE", self.todo())
                .env("DONE_FILE", self.done())
                .env("LSTY_CONFIG", self.dir.join("config.toml"))
                .env("XDG_CONFIG_HOME", home.join("config"))
                .env("XDG_STATE_HOME", home.join("state"));
            if let Some(file) = lsty_scripttest::coverage_profile() {
                cmd.env("LLVM_PROFILE_FILE", file);
            }
        } else {
            cmd.arg("-d").arg(self.dir.join("todo.cfg"));
        }
        cmd.args(args);
        cmd
    }

    /// How the program died, when it did not just succeed or refuse.
    /// todo.sh only ever exits 0 or 1 itself, and lsty 0 to 4; anything
    /// else is the shell or a missing tool failing, or a panic.
    fn crash(&self, status: std::process::ExitStatus) -> Option<String> {
        let last = if self.lsty {
            EXIT_LOCK_TIMEOUT.into()
        } else {
            1
        };
        let allowed = 0..=last;
        match status.code() {
            None => Some(format!("killed by a signal ({status})")),
            Some(code) if allowed.contains(&code) => None,
            Some(code) => Some(format!("exit status {code}")),
        }
    }

    fn run(&self, args: &[String]) -> Result<Step, String> {
        let started = Instant::now();
        let out = self
            .command(args)
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|e| format!("{}: {e}", self.name))?;
        let took = started.elapsed();
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let log = format!(
            "{}\n{}",
            stdout.trim_end(),
            String::from_utf8_lossy(&out.stderr).trim_end()
        );
        let crash = self.crash(out.status);
        Ok(Step {
            ok: out.status.success(),
            crash,
            files: self.read()?,
            out: stdout,
            log: log.trim().to_owned(),
            took,
        })
    }
}

/// A file's bytes, or `None` when it is not there.
fn read_file(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// Writes the file, or removes it for `None`.
fn put_file(path: &Path, bytes: Option<&[u8]>) -> Result<(), String> {
    match bytes {
        Some(bytes) => write(path, bytes),
        None => match fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(format!("{}: {e}", path.display()))
            }
            _ => Ok(()),
        },
    }
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// The settings that make todo.sh behave as `lsty` does by default, where
/// both have the setting:
///
/// - the creation date is stamped on `add` (`include_creation`);
/// - nothing is archived when an item is done unless `archive` says so
///   (`auto_archive`, which each sequence draws);
/// - `del` and `deduplicate` leave a blank line behind, so the numbers of
///   the other items hold (lsty does so always, no setting);
/// - no default priority on `add`.
///
/// `lsty` is told to drop the priority of a done item, as todo.sh does
/// (`keep_priority`), because todo.sh has no setting to keep it: by
/// default lsty keeps the letter as a `pri:` tag, and that is not compared.
fn setup(
    root: &Path,
    auto_archive: bool,
    todo_sh: &Path,
) -> Result<(Side, Side), String> {
    let shim = root.join("shim");
    let mk = |lsty: bool, dir: &str, program: &Path| Side {
        name: if lsty { "lsty" } else { "todo.sh" },
        lsty,
        dir: root.join(dir),
        program: program.to_path_buf(),
        shim: shim.clone(),
    };
    let lsty = mk(true, "lsty", Path::new(env!("CARGO_BIN_EXE_lsty")));
    let sh = mk(false, "todosh", todo_sh);
    for side in [&lsty, &sh] {
        fs::create_dir_all(side.dir.join("home"))
            .map_err(|e| format!("{}: {e}", side.dir.display()))?;
    }
    fs::create_dir_all(&shim).map_err(|e| e.to_string())?;
    // todo.sh asks `date` for the day, and for nothing else here; any other
    // call fails loudly rather than getting the real date
    let script = format!(
        "#!/bin/sh\ncase \"$*\" in\n  '+%Y-%m-%d') echo {DAY} ;;\n  *) echo \"date shim: unexpected call: $*\" >&2; exit 99 ;;\nesac\n"
    );
    write(&shim.join("date"), script.as_bytes())?;
    make_executable(&shim.join("date"))?;
    let archive = u8::from(auto_archive);
    let at = |name: &str| quote(&format!("{}/{name}", sh.dir.display()));
    let cfg = format!(
        "export TODO_DIR={}\nexport TODO_FILE={}\n\
         export DONE_FILE={}\nexport REPORT_FILE={}\n\
         export TODOTXT_FORCE=1\nexport TODOTXT_PLAIN=1\n\
         export TODOTXT_DATE_ON_ADD=1\nexport TODOTXT_AUTO_ARCHIVE={archive}\n\
         export TODOTXT_PRESERVE_LINE_NUMBERS=1\nexport TODOTXT_PRIORITY_ON_ADD=\n",
        quote(&sh.dir.display().to_string()),
        at("todo.txt"),
        at("done.txt"),
        at("report.txt"),
    );
    write(&sh.dir.join("todo.cfg"), cfg.as_bytes())?;
    let toml = format!(
        "auto_archive = {auto_archive}\nkeep_priority = false\n\
         include_creation = true\n"
    );
    write(&lsty.dir.join("config.toml"), toml.as_bytes())?;
    Ok((lsty, sh))
}

fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("{}: {e}", path.display()))
}

// ------------------------------------------------------------ sequences

struct Settings {
    todo_sh: PathBuf,
    runs: usize,
    seed: u64,
    /// The length of every starting file, when set.
    lines: Option<usize>,
    all: bool,
    timings: Timings,
}

/// Files longer than this are shown only near `near`.
const SHOWN: usize = 40;

/// Lines shown on each side of `near` in a long file.
const AROUND: usize = 5;

/// A file for a report; a long one only around line `near` (0-based),
/// with line numbers.
fn show(bytes: Option<&[u8]>, near: usize) -> String {
    let Some(bytes) = bytes else {
        return "    (file is missing)\n".to_owned();
    };
    let text = String::from_utf8_lossy(bytes);
    if text.is_empty() {
        return "    (empty)\n".to_owned();
    }
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let long = lines.len() > SHOWN;
    let near = near.min(lines.len());
    let (from, to) = if long {
        (near.saturating_sub(AROUND), near + AROUND + 1)
    } else {
        (0, lines.len())
    };
    let mut out = String::new();
    if from > 0 {
        putln!(out, "    ... {from} lines");
    }
    for (i, line) in lines.iter().enumerate().take(to).skip(from) {
        let line = line.replace('\n', "$\n");
        if long {
            put!(out, "{:>6}|{line}", i + 1);
        } else {
            put!(out, "    |{line}");
        }
    }
    let rest = lines.len().saturating_sub(to);
    if rest > 0 {
        putln!(out, "    ... {rest} lines");
    }
    out
}

/// The first line (0-based) where two versions of a file differ, or
/// `same` when they are equal.
fn first_difference(a: Option<&[u8]>, b: Option<&[u8]>, same: usize) -> usize {
    if a == b {
        return same;
    }
    let a = a.unwrap_or_default().split_inclusive(|&c| c == b'\n');
    let b = b.unwrap_or_default().split_inclusive(|&c| c == b'\n');
    a.zip(b).take_while(|(x, y)| x == y).count()
}

struct Trace {
    seed: u64,
    lines: Option<usize>,
    start: Files,
    auto_archive: bool,
    done: Vec<String>,
}

impl Trace {
    /// The start of every report: why, the seed and the commands so far.
    fn header(&self, why: &str) -> String {
        let mut out = format!("{why}\nseed {}\n", self.seed);
        putln!(out, "auto_archive {}", self.auto_archive);
        out.push_str("commands so far:\n");
        for line in &self.done {
            putln!(out, "    {line}");
        }
        out
    }

    fn report(&self, why: &str, cmd: &Cmd, lsty: &Step, sh: &Step) -> String {
        let mut out = self.header(why);
        let target = targets(cmd)
            .first()
            .map_or(usize::MAX, |n| n.saturating_sub(1));
        let near_todo = first_difference(
            lsty.files.todo.as_deref(),
            sh.files.todo.as_deref(),
            target,
        );
        let near_done = first_difference(
            lsty.files.done.as_deref(),
            sh.files.done.as_deref(),
            usize::MAX,
        );
        putln!(
            out,
            "starting todo.txt:\n{}",
            show(self.start.todo.as_deref(), 0)
        );
        putln!(
            out,
            "starting done.txt:\n{}",
            show(self.start.done.as_deref(), 0)
        );
        putln!(out, "failing command:\n    {}", cmd.shell());
        for (name, s) in [("lsty", lsty), ("todo.sh", sh)] {
            out.push_str(&side_lines(name, s));
            let todo = show(s.files.todo.as_deref(), near_todo);
            let done = show(s.files.done.as_deref(), near_done);
            putln!(out, "{name} todo.txt:\n{todo}");
            putln!(out, "{name} done.txt:\n{done}");
        }
        out.push_str(&self.repeat());
        out
    }

    /// The line that runs this sequence again.
    fn repeat(&self) -> String {
        match self.lines {
            Some(n) => format!("repeat: just diff 1 {} {n}\n", self.seed),
            None => format!("repeat: just diff 1 {}\n", self.seed),
        }
    }
}

/// What one program did: success, a crash, and the start of what it said
/// (the report shows the files).
fn side_lines(name: &str, step: &Step) -> String {
    let mut out = String::new();
    putln!(out, "{name}: exit ok = {}", step.ok);
    if let Some(how) = &step.crash {
        putln!(out, "{name} CRASHED: {how}");
    }
    let said: String =
        step.log.replace('\n', " / ").chars().take(300).collect();
    putln!(out, "{name} said: {said}");
    out
}

/// A command after which the programs differ, with what to read about it.
struct Variance {
    class: String,
    report: String,
}

/// The starting files: `lines` lines each, or a few when it is `None`
/// (drawn in the same order as always, so a seed keeps its sequence).
fn start_files(rng: &mut Rng, lines: Option<usize>) -> Files {
    let todo = lines.unwrap_or_else(|| rng.below(9));
    let todo = file_text(rng, todo);
    let done = match lines {
        Some(n) => file_text(rng, n),
        None if rng.one_in(3) => {
            let n = rng.below(5);
            file_text(rng, n)
        }
        None => Vec::new(),
    };
    Files {
        todo: Some(todo),
        done: Some(done),
    }
}

/// Whether a program crashed or the programs disagree on success.
fn disagreement(lsty: &Step, sh: &Step) -> Option<&'static str> {
    if lsty.crash.is_some() || sh.crash.is_some() {
        Some("a program crashed")
    } else if lsty.ok != sh.ok {
        Some("the programs disagree on success")
    } else {
        None
    }
}

/// The list commands both programs have, timed after every command.
const READS: [&str; 5] = ["ls", "lsa", "lsp", "lsc", "lsprj"];

/// Turns every run of white space into one space.
fn squeeze(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// What a list command showed, in no order: todo items by line number and
/// text, done items by text alone (todo.sh numbers them all 0), and the
/// other lines (names, from `lsc` and `lsprj`) as they are. The footer,
/// from the line `--` or `-- …` on, is left out.
#[derive(Default, PartialEq)]
struct Shown {
    todo: Vec<(usize, String)>,
    done: Vec<String>,
    other: Vec<String>,
}

impl Shown {
    /// `lsa` squeezes white space runs to one space, as todo.sh's does.
    fn read(out: &str, lsa: bool) -> Self {
        let mut shown = Self::default();
        for line in out.lines() {
            if line == "--" || line.starts_with("-- ") {
                break;
            }
            if lsa {
                shown.add(&squeeze(line));
            } else {
                shown.add(line);
            }
        }
        shown.todo.sort();
        shown.done.sort();
        shown.other.sort();
        shown
    }

    fn add(&mut self, line: &str) {
        let (nr, text) = line.split_once(' ').unwrap_or((line, ""));
        let n = nr.parse::<usize>();
        if nr.starts_with("done:") || n == Ok(0) {
            self.done.push(text.to_owned());
        } else if let Ok(n) = n {
            self.todo.push((n, text.to_owned()));
        } else {
            self.other.push(line.to_owned());
        }
    }

    /// Leaves out the items whose text is only digits and spaces: todo.sh
    /// never lists them (it takes them for the numbers of empty lines).
    fn without_numbers(&mut self) {
        let number =
            |text: &str| text.bytes().all(|b| b.is_ascii_digit() || b == b' ');
        self.todo.retain(|(_, text)| !number(text));
        self.done.retain(|text| !number(text));
    }

    /// What `self` shows more often than `other`, each line counted.
    fn beyond(&self, other: &Self) -> Self {
        Self {
            todo: more(&self.todo, &other.todo),
            done: more(&self.done, &other.done),
            other: more(&self.other, &other.other),
        }
    }

    /// The lines, one per line, indented.
    fn lines(&self) -> String {
        let mut out = String::new();
        for (n, text) in &self.todo {
            putln!(out, "    {n} {text}");
        }
        for text in &self.done {
            putln!(out, "    done {text}");
        }
        for line in &self.other {
            putln!(out, "    {line}");
        }
        out
    }
}

/// The values of `a` left after taking out one for each value of `b`.
/// Both are sorted.
fn more<T: Clone + Ord>(a: &[T], b: &[T]) -> Vec<T> {
    let mut b = b.iter().peekable();
    a.iter()
        .filter(|v| {
            while b.next_if(|w| w < v).is_some() {}
            b.next_if_eq(v).is_none()
        })
        .cloned()
        .collect()
}

/// Where a read variance shows in each file (0-based lines, for `show`):
/// the first todo item one side lacks, and the first done line whose text
/// one side lacks.
fn read_near(files: &Files, extra: [&Shown; 2]) -> (usize, usize) {
    let todo = extra
        .iter()
        .flat_map(|s| s.todo.iter().map(|(n, _)| n.saturating_sub(1)))
        .min()
        .unwrap_or(0);
    let done =
        String::from_utf8_lossy(files.done.as_deref().unwrap_or_default())
            .lines()
            .position(|line| {
                let line = squeeze(line);
                extra.iter().any(|s| s.done.contains(&line))
            })
            .unwrap_or(0);
    (todo, done)
}

/// Whether todo.sh's `lsa` joins the todo file's last line to the done
/// file's first: it does when the todo file does not end with a newline.
fn glued(name: &str, files: &Files) -> bool {
    let todo = files.todo.as_deref().unwrap_or_default();
    let done = files.done.as_deref().unwrap_or_default();
    name == "lsa"
        && !done.is_empty()
        && todo.last().is_some_and(|b| *b != b'\n')
}

/// Runs one list command through both programs, timing it, and returns a
/// variance when they crash, disagree on success, change the files or show
/// different items. A change is put back, so the sequence goes on from
/// `files`.
fn compare_read(
    rng: &mut Rng,
    set: &Settings,
    pair: (&Side, &Side),
    trace: &Trace,
    files: &Files,
) -> Result<Option<Variance>, String> {
    let (lsty, sh) = pair;
    let name = rng.pick(&READS);
    let args = [name.to_owned()];
    let (a, b) = (lsty.run(&args)?, sh.run(&args)?);
    set.timings.add(lsty.name, name, a.took);
    set.timings.add(sh.name, name, b.took);
    let lsa = name == "lsa";
    let mut ours = Shown::read(&a.out, lsa);
    let theirs = Shown::read(&b.out, lsa);
    ours.without_numbers();
    let why = disagreement(&a, &b).or_else(|| {
        (a.files != *files || b.files != *files)
            .then_some("a list command changed the files")
    });
    let why = match why {
        Some(why) => {
            lsty.set(files)?;
            sh.set(files)?;
            why
        }
        None if ours == theirs || glued(name, files) => return Ok(None),
        None => "the programs show different items",
    };
    let (mine, others) = (ours.beyond(&theirs), theirs.beyond(&ours));
    let (near_todo, near_done) = read_near(files, [&mine, &others]);
    let mut report = trace.header(why);
    putln!(report, "list command:\n    {name}");
    putln!(
        report,
        "todo.txt:\n{}",
        show(files.todo.as_deref(), near_todo)
    );
    putln!(
        report,
        "done.txt:\n{}",
        show(files.done.as_deref(), near_done)
    );
    putln!(report, "only lsty shows:\n{}", mine.lines());
    putln!(report, "only todo.sh shows:\n{}", others.lines());
    for (side, step) in [("lsty", &a), ("todo.sh", &b)] {
        report.push_str(&side_lines(side, step));
        if step.files != *files {
            putln!(report, "{side} changed the files");
        }
    }
    report.push_str(&trace.repeat());
    Ok(Some(Variance {
        class: format!("{name}: {why}"),
        report,
    }))
}

/// One sequence. Returns the variances it found: the first one only, unless
/// `set.all` asks to go on after each (todo.sh then gets lsty's files).
fn sequence(seed: u64, set: &Settings) -> Result<Vec<Variance>, String> {
    let mut rng = Rng(seed);
    let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let root = fs::canonicalize(tmp.path()).map_err(|e| e.to_string())?;
    let auto_archive = rng.one_in(2);
    let (lsty, sh) = setup(&root, auto_archive, &set.todo_sh)?;
    let start = start_files(&mut rng, set.lines);
    lsty.set(&start)?;
    sh.set(&start)?;
    let mut trace = Trace {
        seed,
        lines: set.lines,
        start: start.clone(),
        auto_archive,
        done: Vec::new(),
    };
    let mut found = Vec::new();
    let mut files = start;
    let mut reads = Rng(seed ^ 0x5EED_0F15_5EED_0F15);
    for _ in 0..=rng.below(20) {
        let cmd = command(&mut rng, files.todo.as_deref().unwrap_or_default());
        let args = cmd.args();
        let (a, b) = (lsty.run(&args)?, sh.run(&args)?);
        set.timings.add(lsty.name, cmd.name(), a.took);
        set.timings.add(sh.name, cmd.name(), b.took);
        let why = disagreement(&a, &b)
            .or_else(|| (a.files != b.files).then_some("the files differ"));
        let ctx = Ctx::new(&cmd, auto_archive, &files, &a, &b);
        let unknown = why.filter(|_| !explained(&ctx));
        if let Some(why) = unknown {
            found.push(Variance {
                class: format!(
                    "{}: {why}",
                    args.first().map_or("", String::as_str)
                ),
                report: trace.report(why, &cmd, &a, &b),
            });
            if !set.all {
                return Ok(found);
            }
        }
        if why.is_some() {
            sh.set(&a.files)?;
        }
        trace.done.push(cmd.shell());
        files = a.files;
        let pair = (&lsty, &sh);
        if let Some(v) = compare_read(&mut reads, set, pair, &trace, &files)? {
            found.push(v);
            if !set.all {
                return Ok(found);
            }
        }
    }
    Ok(found)
}

/// The variances found so far: by sequence number, then by class.
type Found = Mutex<Vec<(usize, Variance)>>;

/// Runs the sequences on up to three threads. Fails with the variance of
/// the lowest seed, or with the first of every class when `all` is set.
fn run_all(set: &Settings) -> Result<(), Failed> {
    let next = AtomicUsize::new(0);
    let found: Found = Mutex::new(Vec::new());
    let workers = thread::available_parallelism()
        .map_or(1, usize::from)
        .min(3);
    thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| worker(set, &next, &found));
        }
    });
    print_table(set, workers);
    let mut found = found.into_inner().map_err(|e| e.to_string())?;
    found.sort_by_key(|(i, _)| *i);
    let mut classes = Vec::new();
    let mut text = String::new();
    for (_, v) in &found {
        if classes.contains(&&v.class) {
            continue;
        }
        classes.push(&v.class);
        if set.all || classes.len() == 1 {
            putln!(text, "=== {}\n{}", v.class, v.report);
        }
    }
    if found.is_empty() {
        return Ok(());
    }
    if set.all {
        putln!(text, "{} variances, {} classes", found.len(), classes.len());
    }
    Err(text.into())
}

/// The call times go to stdout whether or not the run found a variance.
fn print_table(set: &Settings, parallel: usize) {
    use std::io::Write as _;
    let start = set.lines.map_or_else(
        || "short starting files".to_owned(),
        |n| format!("starting files of {n} lines"),
    );
    let build = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let mut out = std::io::stdout().lock();
    writeln!(
        out,
        "\ncall times in ms ({build} build of lsty, {parallel} sequences \
         at once, {start})\n{}",
        set.timings.table()
    )
    .unwrap_or_default();
}

fn worker(set: &Settings, next: &AtomicUsize, found: &Found) {
    loop {
        let i = next.fetch_add(1, Ordering::SeqCst);
        if i >= set.runs {
            return;
        }
        // later sequences cannot beat a failure of an earlier one
        let beaten = !set.all
            && found.lock().is_ok_and(|f| f.iter().any(|(j, _)| *j < i));
        if beaten {
            return;
        }
        let seed = set.seed.wrapping_add(u64::try_from(i).unwrap_or(0));
        let result = sequence(seed, set).unwrap_or_else(|e| {
            vec![Variance {
                class: "harness".to_owned(),
                report: e,
            }]
        });
        if let Ok(mut f) = found.lock() {
            f.extend(result.into_iter().map(|v| (i, v)));
        }
    }
}

/// `None` when `TODO_SH` is not set.
fn settings() -> Result<Option<Settings>, String> {
    let Some(todo_sh) = host_var("TODO_SH").map(PathBuf::from) else {
        return Ok(None);
    };
    Ok(Some(Settings {
        todo_sh: fs::canonicalize(&todo_sh)
            .map_err(|e| format!("TODO_SH: {}: {e}", todo_sh.display()))?,
        runs: number("LSTY_DIFF_RUNS")?.unwrap_or(1000),
        seed: number("LSTY_DIFF_SEED")?.unwrap_or(1),
        lines: number("LSTY_DIFF_LINES")?,
        all: host_var("LSTY_DIFF_ALL").is_some(),
        timings: Timings::default(),
    }))
}

fn main() -> std::process::ExitCode {
    let set = settings();
    let skipped = matches!(set, Ok(None));
    let trial =
        Trial::test(
            "random commands agree with todo.sh",
            move || match &set {
                Ok(Some(set)) => run_all(set),
                Ok(None) => Err("TODO_SH names no program".to_owned().into()),
                Err(e) => Err(e.clone().into()),
            },
        )
        .with_ignored_flag(skipped);
    let check =
        Trial::test("call time percentiles take the nearest rank", || {
            random::check_percentiles().map_err(Into::into)
        });
    let del_term =
        Trial::test("del N TERM model refuses what lsty refuses", || {
            rules::check_del_term().map_err(Into::into)
        });
    let dedup = Trial::test("deduplicate view restores white space", || {
        rules::check_dedup_view().map_err(Into::into)
    });
    let reason = Trial::test("a refusal needs lsty's reason", || {
        rules::check_refusal_reason().map_err(Into::into)
    });
    let all = vec![trial, check, del_term, dedup, reason];
    libtest_mimic::run(&Arguments::from_args(), all).exit_code()
}
