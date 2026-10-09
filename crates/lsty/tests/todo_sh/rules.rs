//! The variances between lsty and todo.sh that are settled, as narrow
//! rules. A rule looks at the command, the files before it and what each
//! program did, and says why a difference is not a bug. See the module doc
//! of the test for what the generator leaves out.
//!
//! Three kinds of rule, each with a reason:
//!
//! - `REFUSALS`: lsty refuses a command that todo.sh runs, and leaves the
//!   files as they were.
//! - `SUCCESS`: lsty succeeds where todo.sh fails.
//! - `LINES` and `FILES`: the files differ. `LINES` explain a line of a
//!   target that differs; `FILES` the other differences.
//!
//! Some differences are not worth a rule of their own: the files are
//! put into the same form first (`Ctx::new`), because the cause is a plain
//! bug of todo.sh or only the form of a line.

use super::{Cmd, DATES, DAY, Files, Step};

/// What a rule looks at: the command, the files before it and what each
/// program did.
pub(super) struct Ctx<'a> {
    pub(super) cmd: &'a Cmd,
    pub(super) auto_archive: bool,
    pub(super) before: &'a Files,
    pub(super) lsty: &'a Step,
    pub(super) sh: &'a Step,
    /// What lsty left, in the form the rules compare (see `lsty_view`).
    lsty_files: Files,
    /// What todo.sh left, in the form the rules compare (see `sh_view`).
    sh_files: Files,
}

impl<'a> Ctx<'a> {
    pub(super) fn new(
        cmd: &'a Cmd,
        auto_archive: bool,
        before: &'a Files,
        lsty: &'a Step,
        sh: &'a Step,
    ) -> Self {
        Self {
            cmd,
            auto_archive,
            before,
            lsty,
            sh,
            lsty_files: lsty_view(cmd, before, &lsty.files),
            sh_files: sh_view(cmd, before, &sh.files),
        }
    }
}

/// Applies `f` to every line of a file, with its line end, and joins the
/// results.
fn map_lines(
    bytes: &[u8],
    mut f: impl FnMut(&[u8]) -> Option<Vec<u8>>,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    for line in bytes.split_inclusive(|c| *c == b'\n') {
        match f(line) {
            Some(new) => out.extend_from_slice(&new),
            None => out.extend_from_slice(line),
        }
    }
    out
}

/// What lsty left, with the lines the command wrote in a common form:
/// `do` keeps a lowercase `(b)` in front of the text, which todo.sh takes
/// for a priority and drops (see `lowercase_priority`).
fn lsty_view(cmd: &Cmd, before: &Files, after: &Files) -> Files {
    let Cmd::Do(nrs) = cmd else {
        return after.clone();
    };
    let lines = split(before.todo.as_ref());
    let done_as_lsty = |nr: &usize| {
        let line = text(lines.get(nr.checked_sub(1)?)?);
        is_lowercase_priority(&line).then(|| {
            let kept = line.get(4..).unwrap_or_default();
            (format!("x {DAY} {line}"), format!("x {DAY} {kept}"))
        })
    };
    let pairs: Vec<(String, String)> =
        nrs.iter().filter_map(done_as_lsty).collect();
    // todo.sh does a line of white space only as it does any item; lsty
    // takes it for a blank line and leaves it
    let mut todo = after.todo.clone();
    if let Some(bytes) = &mut todo {
        let at: Vec<usize> = nrs
            .iter()
            .filter(|nr| {
                nr.checked_sub(1)
                    .and_then(|i| lines.get(i))
                    .is_some_and(|l| white(l))
            })
            .copied()
            .collect();
        let mut nr = 0;
        *bytes = map_lines(bytes, |line| {
            nr += 1;
            at.contains(&nr).then(|| {
                let mut new = format!("x {DAY} ").into_bytes();
                new.extend_from_slice(line);
                new
            })
        });
    }
    let fix = |bytes: &Vec<u8>| {
        map_lines(bytes, |line| {
            let t = text(line);
            let (_, to) = pairs.iter().find(|(from, _)| *from == t)?;
            let mut new = to.clone().into_bytes();
            new.extend_from_slice(line.get(body(line).len()..)?);
            Some(new)
        })
    };
    Files {
        todo: todo.as_ref().map(fix),
        done: after.done.as_ref().map(fix),
    }
}

/// What todo.sh left, with its own quirks undone:
///
/// - it writes the first line it adds to a done file without a final
///   newline straight after the last line, so the two run together;
/// - `deduplicate` rewrites the todo file with a newline at the end;
/// - `archive` keeps the lines of white space only, which lsty takes for
///   blank lines and removes along with the empty ones.
fn sh_view(cmd: &Cmd, before: &Files, after: &Files) -> Files {
    let mut files = after.clone();
    if let (Some(old), Some(new)) = (&before.done, &mut files.done)
        && !old.is_empty()
        && !old.ends_with(b"\n")
        && new.len() > old.len()
        && new.starts_with(old)
    {
        new.insert(old.len(), b'\n');
    }
    if matches!(cmd, Cmd::Deduplicate)
        && let (Some(old), Some(new)) = (&before.todo, &mut files.todo)
        && !old.is_empty()
        && !old.ends_with(b"\n")
        && new.ends_with(b"\n")
    {
        new.pop();
    }
    if matches!(cmd, Cmd::Archive)
        && let Some(todo) = &files.todo
    {
        let kept = map_lines(todo, |l| white(l).then(Vec::new));
        files.todo = Some(kept);
    }
    files
}

type Rule = fn(&Ctx<'_>) -> Option<&'static str>;

/// A rule for one line of a target: the line before the command, and the
/// line each program made of it.
type LineRule = fn(&Ctx<'_>, &Line<'_>) -> Option<&'static str>;

/// Rules for a command that lsty refuses and todo.sh runs: the exit status
/// and the files both differ, and lsty left the files as they were.
const REFUSALS: &[Rule] = &[
    refused_new_done_item,
    refused_bad_date_tag,
    refused_done_priority,
    refused_del_term,
];

/// Rules for a difference in success: lsty succeeds, todo.sh fails.
const SUCCESS: &[Rule] = &[nothing_to_do];

/// Rules for a line of a target that differs.
const LINES: &[LineRule] = &[
    done_mark_first,
    lowercase_priority,
    white_space_line,
    del_whole_words,
    trailing_white_space,
];

/// Rules for the files as a whole.
const FILES: &[Rule] =
    &[list_refused, auto_archive_scope, white_space_duplicates];

/// Whether every difference after the command is settled by a rule. A
/// crash is never settled.
pub(super) fn explained(ctx: &Ctx<'_>) -> bool {
    if ctx.lsty.crash.is_some() || ctx.sh.crash.is_some() {
        return false;
    }
    let find = |rules: &[Rule]| rules.iter().find_map(|rule| rule(ctx));
    if find(REFUSALS).is_some() {
        return true;
    }
    let success = ctx.lsty.ok == ctx.sh.ok || find(SUCCESS).is_some();
    let files = ctx.lsty_files == ctx.sh_files
        || find(FILES).is_some()
        || explained_lines(ctx);
    success && files
}

// --------------------------------------------------------------- helpers

/// The lines of a file with their line ends; the last may have none.
fn split(bytes: Option<&Vec<u8>>) -> Vec<&[u8]> {
    bytes
        .map(|b| b.split_inclusive(|c| *c == b'\n').collect())
        .unwrap_or_default()
}

/// The line without its line end.
fn body(line: &[u8]) -> &[u8] {
    line.strip_suffix(b"\n").unwrap_or(line)
}

fn empty(line: &[u8]) -> bool {
    body(line).is_empty()
}

/// A line of blanks, but not an empty one.
fn white(line: &[u8]) -> bool {
    let b = body(line);
    !b.is_empty() && b.iter().all(|c| matches!(c, b' ' | b'\t'))
}

fn text(line: &[u8]) -> String {
    String::from_utf8_lossy(body(line)).into_owned()
}

/// `(X) ` at the start, with any character for X, as todo.sh reads a
/// priority.
fn has_bracket_priority(line: &str) -> bool {
    let b = line.as_bytes();
    b.len() >= 4 && b[0] == b'(' && b[2] == b')' && b[3] == b' '
}

/// `(X) ` at the start with a capital letter, a priority for lsty.
fn has_priority(line: &str) -> bool {
    has_bracket_priority(line)
        && line.as_bytes().get(1).is_some_and(u8::is_ascii_uppercase)
}

/// `(x) ` at the start with a lowercase letter: a priority for todo.sh,
/// text for lsty.
fn is_lowercase_priority(line: &str) -> bool {
    has_bracket_priority(line)
        && line.as_bytes().get(1).is_some_and(u8::is_ascii_lowercase)
}

/// `YYYY-MM-DD ` at `at`: a real date and a space.
fn date_at(line: &str, at: usize) -> bool {
    let Some(w) = line.as_bytes().get(at..at + 11) else {
        return false;
    };
    let digits = |r: std::ops::Range<usize>| {
        std::str::from_utf8(&w[r])
            .ok()
            .filter(|d| d.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|d| d.parse::<u32>().ok())
    };
    let (Some(y), Some(m), Some(d)) =
        (digits(0..4), digits(5..7), digits(8..10))
    else {
        return false;
    };
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    w[4] == b'-' && w[7] == b'-' && w[10] == b' ' && (1..=days).contains(&d)
}

/// How much of a done line is `x `, the dates and a priority in front of
/// it, as lsty reads them; 0 for a line that is not done.
fn done_stamps(line: &str) -> usize {
    if line.starts_with("x ") {
        stamps(line)
    } else {
        0
    }
}

/// The length of the done mark, priority and dates at the start of a line,
/// as lsty reads them: a done line has two dates, any other one.
fn stamps(line: &str) -> usize {
    let done = line.starts_with("x ");
    let mut at = if done { 2 } else { 0 };
    if has_priority(line.get(at..).unwrap_or_default()) {
        at += 4;
    }
    for _ in 0..(if done { 2 } else { 1 }) {
        if date_at(line, at) {
            at += 11;
        } else {
            break;
        }
    }
    at
}

/// The line numbers a command names.
pub(crate) fn targets(cmd: &Cmd) -> Vec<usize> {
    match cmd {
        Cmd::Append(n, _)
        | Cmd::Prepend(n, _)
        | Cmd::Replace(n, _)
        | Cmd::Pri(n, _)
        | Cmd::Del(n)
        | Cmd::DelTerm(n, _) => vec![*n],
        Cmd::Depri(n) | Cmd::Do(n) => n.clone(),
        Cmd::Add(_) | Cmd::Addm(_) | Cmd::Archive | Cmd::Deduplicate => {
            Vec::new()
        }
    }
}

/// The text of line `nr` before the command, with its line end.
fn before_raw<'a>(ctx: &'a Ctx<'_>, nr: usize) -> Option<&'a [u8]> {
    let lines = split(ctx.before.todo.as_ref());
    nr.checked_sub(1).and_then(|i| lines.get(i).copied())
}

fn before_line(ctx: &Ctx<'_>, nr: usize) -> Option<String> {
    before_raw(ctx, nr).map(text)
}

/// The texts a command gives.
fn texts(cmd: &Cmd) -> Vec<&str> {
    match cmd {
        Cmd::Add(t)
        | Cmd::Append(_, t)
        | Cmd::Prepend(_, t)
        | Cmd::Replace(_, t) => vec![t],
        Cmd::Addm(list) => list.iter().map(String::as_str).collect(),
        Cmd::Pri(..)
        | Cmd::Depri(_)
        | Cmd::Do(_)
        | Cmd::Del(_)
        | Cmd::DelTerm(..)
        | Cmd::Archive
        | Cmd::Deduplicate => Vec::new(),
    }
}

/// lsty failed and left both files as they were.
fn refused(ctx: &Ctx<'_>) -> bool {
    !ctx.lsty.ok && untouched(ctx)
}

/// lsty left both files as they were.
fn untouched(ctx: &Ctx<'_>) -> bool {
    ctx.lsty.files == *ctx.before
}

/// What todo.sh's `del N TERM` makes of a line: its five `sed` commands in
/// turn, with `term` taken literally.
fn sed_del(line: &str, term: &str) -> String {
    let line = sed_leading(line, term);
    let line = sed_trailing(&line, term);
    let line = sed_runs(&line, term, 1, 0);
    let line = sed_runs(&line, term, 0, 1);
    line.replace(term, "")
}

fn spaces(line: &str, from: usize) -> usize {
    line.get(from..)
        .map_or(0, |t| t.bytes().take_while(|b| *b == b' ').count())
}

/// `s/^\((.) \)\{0,1\} *TERM */\1/`
fn sed_leading(line: &str, term: &str) -> String {
    let groups: &[usize] = if has_bracket_priority(line) {
        &[4, 0]
    } else {
        &[0]
    };
    for &g in groups {
        let at = g + spaces(line, g);
        if line.get(at..).is_some_and(|t| t.starts_with(term)) {
            let end = at + term.len();
            let after = line.get(end + spaces(line, end)..).unwrap_or("");
            return format!("{}{after}", line.get(..g).unwrap_or(""));
        }
    }
    line.to_owned()
}

/// `s/ *TERM *$//`
fn sed_trailing(line: &str, term: &str) -> String {
    for (i, _) in line.char_indices() {
        let at = i + spaces(line, i);
        let tail = line.get(at..).unwrap_or("");
        if tail
            .strip_prefix(term)
            .is_some_and(|r| r.bytes().all(|b| b == b' '))
        {
            return line.get(..i).unwrap_or("").to_owned();
        }
    }
    line.to_owned()
}

/// `s/ \{before,\}TERM \{after,\}/ /g` with `before` and `after` the
/// least number of spaces on each side (`  *TERM *` and ` *TERM  *`).
fn sed_runs(line: &str, term: &str, before: usize, after: usize) -> String {
    let mut out = String::new();
    let mut at = 0;
    while at <= line.len() {
        let Some(found) = (at..=line.len())
            .filter(|i| line.is_char_boundary(*i))
            .find_map(|i| {
                let lead = spaces(line, i);
                let t = i + lead;
                let rest = line.get(t..)?.strip_prefix(term)?;
                let trail = rest.bytes().take_while(|b| *b == b' ').count();
                (lead >= before && trail >= after)
                    .then_some((i, t + term.len() + trail))
            })
        else {
            break;
        };
        out.push_str(line.get(at..found.0).unwrap_or(""));
        out.push(' ');
        at = found.1.max(found.0 + 1);
    }
    out.push_str(line.get(at..).unwrap_or(""));
    out
}

// ------------------------------------------------------ refused commands

/// lsty refuses to add an item that starts as done ("add it, then use
/// do"); todo.sh adds it as it is.
fn refused_new_done_item(ctx: &Ctx<'_>) -> Option<&'static str> {
    let new = matches!(ctx.cmd, Cmd::Add(_) | Cmd::Addm(_));
    let done = texts(ctx.cmd).iter().any(|t| t.starts_with("x "));
    (new && done && refused(ctx)).then_some("a new item cannot start as done")
}

/// lsty refuses a `due:` or `t:` value that is not a date; todo.sh keeps
/// the text.
fn refused_bad_date_tag(ctx: &Ctx<'_>) -> Option<&'static str> {
    let bad = |t: &&str| {
        t.split(' ').any(|w| {
            w.strip_prefix("due:")
                .or_else(|| w.strip_prefix("t:"))
                .is_some_and(|v| !DATES.contains(&v))
        })
    };
    (texts(ctx.cmd).iter().any(bad) && refused(ctx))
        .then_some("due: and t: must hold a date")
}

/// lsty refuses to give a done item another priority ("reopen it first");
/// todo.sh sets it, or replaces the line, and the item is open again.
fn refused_done_priority(ctx: &Ctx<'_>) -> Option<&'static str> {
    let (nr, new_priority) = match ctx.cmd {
        Cmd::Pri(n, _) => (*n, true),
        Cmd::Replace(n, t) => (*n, has_bracket_priority(t)),
        Cmd::Add(_)
        | Cmd::Addm(_)
        | Cmd::Append(..)
        | Cmd::Prepend(..)
        | Cmd::Depri(_)
        | Cmd::Do(_)
        | Cmd::Del(_)
        | Cmd::DelTerm(..)
        | Cmd::Archive
        | Cmd::Deduplicate => return None,
    };
    let done = before_line(ctx, nr).is_some_and(|l| l.starts_with("x "));
    (new_priority && done && refused(ctx))
        .then_some("a done item's priority is changed by reopening it")
}

/// lsty refuses to take the last word out of an item ("use del") and an
/// edit that turns the words left into a stamp; todo.sh does both.
fn refused_del_term(ctx: &Ctx<'_>) -> Option<&'static str> {
    let Cmd::DelTerm(nr, term) = ctx.cmd else {
        return None;
    };
    let line = before_line(ctx, *nr)?;
    lsty_line(&line, term).err().filter(|_| refused(ctx))
}

// -------------------------------------------------------------- success

/// A command with nothing to do succeeds in lsty (it says so and leaves
/// the files) and fails in todo.sh: a target that is a blank line ("already
/// deleted"), `do` on an item that is done, `depri` on an item without a
/// priority, `del N TERM` with a term the item lacks, `deduplicate` with
/// nothing to remove. The files are then settled by the other rules.
fn nothing_to_do(ctx: &Ctx<'_>) -> Option<&'static str> {
    if !ctx.lsty.ok || ctx.sh.ok {
        return None;
    }
    if matches!(ctx.cmd, Cmd::Deduplicate) {
        return (untouched(ctx) && !has_duplicates(ctx))
            .then_some("nothing to remove is not an error");
    }
    let nrs = targets(ctx.cmd);
    if nrs.len() == 1 && !untouched(ctx) {
        return None;
    }
    let idle = |nr: &usize| {
        let Some(line) = before_raw(ctx, *nr) else {
            return false;
        };
        let t = text(line);
        let lacks = |term: &str| sed_del(&t, term) == t;
        match ctx.cmd {
            Cmd::Do(_) => empty(line) || white(line) || t.starts_with("x "),
            Cmd::Pri(_, p) => {
                empty(line)
                    || white(line)
                    || (has_priority(&t)
                        && t.as_bytes().get(1)
                            == Some(&(*p as u8).to_ascii_uppercase()))
            }
            Cmd::Depri(_) => {
                empty(line) || white(line) || !has_bracket_priority(&t)
            }
            Cmd::DelTerm(_, term) => empty(line) || white(line) || lacks(term),
            Cmd::Append(..)
            | Cmd::Prepend(..)
            | Cmd::Replace(..)
            | Cmd::Del(_) => empty(line) || white(line),
            Cmd::Add(_) | Cmd::Addm(_) | Cmd::Archive | Cmd::Deduplicate => {
                false
            }
        }
    };
    nrs.iter()
        .any(idle)
        .then_some("a command with nothing to do is not an error")
}

/// Whether two lines of the todo file before the command are equal and
/// not empty.
fn has_duplicates(ctx: &Ctx<'_>) -> bool {
    let lines = split(ctx.before.todo.as_ref());
    lines.iter().enumerate().any(|(i, a)| {
        !empty(a) && lines.iter().skip(i + 1).any(|b| body(a) == body(b))
    })
}

// ---------------------------------------------------------------- files

/// A list is all or nothing in lsty: a target that cannot be changed
/// refuses the whole list and nothing is written; todo.sh changes the
/// others and then fails.
fn list_refused(ctx: &Ctx<'_>) -> Option<&'static str> {
    let nrs = targets(ctx.cmd);
    // the target lsty cannot change: a done item has no priority to drop
    let done = |nr: &usize| {
        before_line(ctx, *nr).is_some_and(|l| l.starts_with("x "))
    };
    let cannot = matches!(ctx.cmd, Cmd::Depri(_)) && nrs.iter().any(done);
    (nrs.len() > 1 && cannot && refused(ctx) && !ctx.sh.ok)
        .then_some("a list is all or nothing")
}

/// The lines of `bytes` without their line ends, sorted: a file's lines as
/// a multiset.
fn multiset(bytes: &[u8]) -> Vec<&[u8]> {
    let mut lines: Vec<&[u8]> =
        bytes.split_inclusive(|c| *c == b'\n').map(body).collect();
    lines.sort_unstable();
    lines
}

/// Whether lsty's todo file (`mine`) kept every line number: each target
/// that was an open item is now an empty line (a last one writes nothing),
/// and no line is missing but such a last one. That a non-target line did
/// not change is for the comparison with todo.sh's file that follows.
fn numbers_kept(ctx: &Ctx<'_>, mine: &[&[u8]]) -> bool {
    let before = split(ctx.before.todo.as_ref());
    let nrs = targets(ctx.cmd);
    let open = |nr: &usize| {
        before_raw(ctx, *nr)
            .is_some_and(|l| !empty(l) && !white(l) && !l.starts_with(b"x "))
    };
    mine.len() <= before.len()
        && (0..before.len()).all(|i| {
            let nr = i + 1;
            let blanked = nrs.contains(&nr) && open(&nr);
            let now = mine.get(i).copied().unwrap_or_default();
            !blanked || empty(now)
        })
        && (mine.len()..before.len()).all(|i| {
            let nr = i + 1;
            nrs.contains(&nr) && open(&nr)
        })
}

/// With `auto_archive`, `do` in lsty archives the items it completed and
/// leaves their lines blank so the numbers stay; todo.sh archives every
/// done line of the todo file and drops every empty line. The todo file of
/// todo.sh is lsty's without its empty lines and done lines, and its done
/// file is the old one followed by the same lines lsty archived and the
/// done lines it left, in the order todo.sh finds them.
fn auto_archive_scope(ctx: &Ctx<'_>) -> Option<&'static str> {
    if !(ctx.auto_archive && matches!(ctx.cmd, Cmd::Do(_))) {
        return None;
    }
    let mine = split(ctx.lsty_files.todo.as_ref());
    if !ctx.lsty.ok || !numbers_kept(ctx, &mine) {
        return None;
    }
    let live = |l: &&[u8]| !empty(l);
    let done = |l: &&[u8]| l.starts_with(b"x ");
    let kept: Vec<&[u8]> = mine
        .iter()
        .copied()
        .filter(live)
        .filter(|l| !done(l))
        .collect();
    let left: Vec<u8> = mine
        .iter()
        .copied()
        .filter(live)
        .filter(done)
        .flat_map(|l| [body(l), b"\n"].concat())
        .collect();
    let theirs = split(ctx.sh_files.todo.as_ref());
    let same_todo = kept.len() == theirs.len()
        && kept.iter().zip(&theirs).all(|(a, b)| body(a) == body(b));
    let old = ctx.before.done.as_deref().unwrap_or_default();
    let (a, b) = (
        ctx.lsty_files.done.as_deref().unwrap_or_default(),
        ctx.sh_files.done.as_deref().unwrap_or_default(),
    );
    // a done file that ends without a newline gets one before the new lines
    let tail = |bytes: &'_ [u8]| {
        let t = bytes.get(old.len()..).unwrap_or_default();
        if old.is_empty() || old.ends_with(b"\n") {
            t.to_vec()
        } else {
            t.strip_prefix(b"\n").unwrap_or(t).to_vec()
        }
    };
    let mut expected = tail(a);
    expected.extend_from_slice(&left);
    let same_done = a.starts_with(old)
        && b.starts_with(old)
        && multiset(&expected) == multiset(&tail(b));
    (same_todo && same_done).then_some(
        "auto-archive moves the items it completed and keeps the numbers",
    )
}

// ----------------------------------------------------------------- lines

/// `deduplicate` in todo.sh empties every line that repeats an earlier one,
/// lines of white space only among them; lsty takes those for blank lines,
/// not items, and leaves them.
fn white_space_duplicates(ctx: &Ctx<'_>) -> Option<&'static str> {
    if !matches!(ctx.cmd, Cmd::Deduplicate)
        || !ctx.lsty.ok
        || ctx.lsty_files.done != ctx.sh_files.done
    {
        return None;
    }
    let mut seen: Vec<Vec<u8>> = Vec::new();
    let mine = ctx.lsty_files.todo.as_deref().map(|bytes| {
        map_lines(bytes, |line| {
            if !white(line) {
                return None;
            }
            let repeat = seen.iter().any(|s| s == body(line));
            seen.push(body(line).to_vec());
            repeat.then(|| line.get(body(line).len()..).map(<[u8]>::to_vec))?
        })
    });
    (mine.as_deref() == ctx.sh_files.todo.as_deref())
        .then_some("a line of white space only is blank, not a duplicate")
}

/// A target line: what it was, and what each program made of it.
struct Line<'a> {
    before: &'a str,
    mine: &'a str,
    theirs: &'a str,
}

/// Whether the todo files differ only at lines of targets that a rule of
/// `LINES` explains, with as many lines each and the same done file.
fn explained_lines(ctx: &Ctx<'_>) -> bool {
    let (a, b) = (
        split(ctx.lsty_files.todo.as_ref()),
        split(ctx.sh_files.todo.as_ref()),
    );
    if a.len() != b.len() || ctx.lsty_files.done != ctx.sh_files.done {
        return false;
    }
    let nrs = targets(ctx.cmd);
    a.iter().zip(&b).enumerate().all(|(i, (x, y))| {
        if x == y {
            return true;
        }
        let nr = i + 1;
        let Some(before) = before_line(ctx, nr) else {
            return false;
        };
        let (mine, theirs) = (text(x), text(y));
        let line = Line {
            before: &before,
            mine: &mine,
            theirs: &theirs,
        };
        nrs.contains(&nr)
            && LINES.iter().any(|rule| rule(ctx, &line).is_some())
    })
}

/// The done mark stays first in lsty: `prepend` goes after `x DATE `, and
/// `replace` keeps the line done; todo.sh puts the text in front, so the
/// mark is no longer at the start and the item is no longer done.
fn done_mark_first(ctx: &Ctx<'_>, line: &Line<'_>) -> Option<&'static str> {
    let head = done_stamps(line.before);
    if head == 0 {
        return None;
    }
    let expected = match ctx.cmd {
        Cmd::Prepend(_, t) => {
            format!("{}{t} {}", &line.before[..head], &line.before[head..])
        }
        Cmd::Replace(_, t) => format!("{}{t}", &line.before[..head]),
        Cmd::Add(_)
        | Cmd::Addm(_)
        | Cmd::Append(..)
        | Cmd::Pri(..)
        | Cmd::Depri(_)
        | Cmd::Do(_)
        | Cmd::Del(_)
        | Cmd::DelTerm(..)
        | Cmd::Archive
        | Cmd::Deduplicate => return None,
    };
    (ctx.lsty.ok && lsty_text(line.mine, &expected))
        .then_some("a done item stays done and its done mark stays first")
}

/// The part of a line todo.sh reads as a priority and a date, which it
/// keeps in front of text it prepends or replaces: `(X) ` and then a
/// date of 2 to 4 year digits.
fn priority_and_date(line: &str) -> &str {
    let mut at = if has_bracket_priority(line) { 4 } else { 0 };
    let rest = line.get(at..).unwrap_or_default();
    let year = rest.bytes().take_while(u8::is_ascii_digit).count();
    let tail = rest.get(year..).unwrap_or_default();
    let is_date = (2..=4).contains(&year)
        && tail.len() >= 7
        && tail.as_bytes()[0] == b'-'
        && tail.as_bytes()[3] == b'-'
        && tail.as_bytes()[1..3].iter().all(u8::is_ascii_digit)
        && tail.as_bytes()[4..6].iter().all(u8::is_ascii_digit)
        && tail.as_bytes()[6] == b' ';
    if is_date {
        at += year + 7;
    }
    line.get(..at).unwrap_or_default()
}

/// Whether `mine` is the line `expected` as lsty writes an edited line:
/// without white space at the end.
fn lsty_text(mine: &str, expected: &str) -> bool {
    mine == expected.trim_end_matches([' ', '\t'])
}

/// A lowercase `(b)` is text in the todo.txt format, not a priority;
/// todo.sh takes any letter in brackets for one. So `pri` puts the new
/// letter in front of lsty's text and in place of it in todo.sh, `depri`
/// has nothing to remove in lsty, `prepend` and `replace` keep it in
/// todo.sh and put the text in front of it in lsty. And a replacement
/// that starts with a lowercase `(b)` has its letter made a capital in
/// lsty only.
fn lowercase_priority(ctx: &Ctx<'_>, line: &Line<'_>) -> Option<&'static str> {
    let reason = "a lowercase letter in brackets is text, not a priority";
    let old = is_lowercase_priority(line.before);
    let kept = priority_and_date(line.before);
    let rest = line.before.get(kept.len()..).unwrap_or_default();
    let ok = match ctx.cmd {
        Cmd::Pri(_, p) => {
            let p = p.to_ascii_uppercase();
            old && line.mine == format!("({p}) {}", line.before)
                && line.theirs
                    == format!("({p}) {}", line.before.get(4..).unwrap_or(""))
        }
        Cmd::Depri(_) => {
            old && line.mine == line.before
                && Some(line.theirs) == line.before.get(4..)
        }
        Cmd::Prepend(_, t) => {
            old && lsty_text(line.mine, &format!("{t} {}", line.before))
                && line.theirs == format!("{kept}{t} {rest}")
        }
        Cmd::Replace(_, t) => {
            let own = is_lowercase_priority(t);
            let fold = |s: &str| {
                let mut b = s.to_owned().into_bytes();
                if is_lowercase_priority(s) {
                    b[1] = b[1].to_ascii_uppercase();
                }
                String::from_utf8(b).unwrap_or_default()
            };
            let pri = if has_bracket_priority(line.mine) {
                4
            } else {
                0
            };
            let (new, text) = line.mine.split_at(pri);
            let date = kept.get(4..).unwrap_or_default();
            (own && line.mine == fold(line.theirs))
                || (old
                    && (fold(line.theirs) == format!("{new}{date}{text}")
                        || (pri == 0
                            && line.theirs == format!("{kept}{text}"))))
        }
        Cmd::Add(_)
        | Cmd::Addm(_)
        | Cmd::Append(..)
        | Cmd::Do(_)
        | Cmd::Del(_)
        | Cmd::DelTerm(..)
        | Cmd::Archive
        | Cmd::Deduplicate => false,
    };
    ok.then_some(reason)
}

/// A line of white space only is a blank line to lsty (an item that is
/// already deleted); todo.sh treats it as an item unless it is empty.
fn white_space_line(_: &Ctx<'_>, line: &Line<'_>) -> Option<&'static str> {
    let blank = !line.before.is_empty()
        && line.before.bytes().all(|c| matches!(c, b' ' | b'\t'));
    (blank && line.mine == line.before)
        .then_some("a line of white space only is blank, not an item")
}

/// What lsty's `del N TERM` makes of a line, if the term is a whole word of
/// the description (the done mark, priority and dates in front are not part
/// of it): the blanks at the start stay, a run of removed words with the
/// blanks around and between them becomes one space (nothing at an end),
/// nothing is left at the end. `None`: no such word, lsty writes nothing.
/// Exact over ASCII blanks (all the generator makes). The result may be a
/// line lsty refuses (see `lsty_line`).
fn lsty_del(line: &str, term: &str) -> Option<String> {
    let (front, desc) = line.split_at(stamps(line));
    let rest = desc.trim_start_matches([' ', '\t']);
    let lead = desc.get(..desc.len() - rest.len()).unwrap_or_default();
    let mut body = String::new();
    // a word was taken out since the last one kept
    let (mut removed, mut any) = (false, false);
    let mut tail = rest;
    while !tail.is_empty() {
        let word = tail.trim_start_matches([' ', '\t']);
        let gap = tail.get(..tail.len() - word.len()).unwrap_or_default();
        let len = word.find([' ', '\t']).unwrap_or(word.len());
        tail = word.get(len..).unwrap_or_default();
        let word = word.get(..len).unwrap_or_default();
        if word.is_empty() {
            continue;
        }
        if word == term {
            (removed, any) = (true, true);
            continue;
        }
        if !body.is_empty() {
            body.push_str(if removed { " " } else { gap });
        }
        removed = false;
        body.push_str(word);
    }
    let lead = if body.is_empty() { "" } else { lead };
    any.then(|| format!("{front}{lead}{body}"))
}

/// The line lsty leaves after `del N TERM`: the line itself when the term
/// is not a whole word of it, the reason when lsty refuses.
fn lsty_line(line: &str, term: &str) -> Result<String, &'static str> {
    let Some(l) = lsty_del(line, term) else {
        return Ok(line.to_owned());
    };
    if l.len() == stamps(line) {
        Err("an item cannot be left without a description")
    } else if stamps(&l) > stamps(line) {
        Err("an edit cannot make a stamp of the words left")
    } else {
        Ok(l)
    }
}

/// lsty takes a term out of the description only, and only where it is a
/// whole word; todo.sh takes it out of the whole line, inside words and
/// stamps too (`pay` out of `repay`, out of `+pay`). The line is what
/// lsty's model makes of it, and todo.sh's what its `sed` makes.
fn del_whole_words(ctx: &Ctx<'_>, line: &Line<'_>) -> Option<&'static str> {
    let Cmd::DelTerm(_, term) = ctx.cmd else {
        return None;
    };
    (ctx.lsty.ok
        && line.theirs == sed_del(line.before, term)
        && lsty_line(line.before, term).is_ok_and(|m| m == line.mine))
    .then_some(
        "a term is taken out of the description, where it is a whole word",
    )
}

/// lsty trims the white space at the end of a line it edits; todo.sh keeps
/// it.
fn trailing_white_space(
    ctx: &Ctx<'_>,
    line: &Line<'_>,
) -> Option<&'static str> {
    let edit = matches!(
        ctx.cmd,
        Cmd::Append(..) | Cmd::Prepend(..) | Cmd::Replace(..)
    );
    let trimmed = line.theirs.trim_end_matches([' ', '\t']);
    (edit && ctx.lsty.ok && trimmed != line.theirs && line.mine == trimmed)
        .then_some("white space at the end of an edited line is trimmed")
}

/// The model of lsty's `del N TERM` refuses what lsty refuses and keeps the
/// rest as lsty writes it.
pub(super) fn check_del_term() -> Result<(), String> {
    type Row = (&'static str, &'static str, Option<&'static str>);
    let rows: [Row; 6] = [
        ("(A) A", "A", None),
        ("a 2026-01-15 +a", "a", None),
        ("pay repay +pay pay x", "pay", Some("repay +pay x")),
        ("  pay a", "pay", Some("  a")),
        ("(A) repay", "pay", Some("(A) repay")),
        ("one two  pay  three", "pay", Some("one two three")),
    ];
    for (line, term, want) in rows {
        let got = lsty_line(line, term).ok();
        if got.as_deref() != want {
            return Err(format!(
                "{line:?} minus {term:?}: {got:?}, not {want:?}"
            ));
        }
    }
    Ok(())
}
