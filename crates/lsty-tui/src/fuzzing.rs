//! The property the fuzz target `hints` checks: a run of keys over a list,
//! the files sometimes changed behind its back, against what the hint of
//! each key said the key would do.

use std::{cmp::Ordering, collections::HashMap, fs, path::Path};

use lsty_core::{
    config::Config,
    errs::Source,
    keys::{Action, Matcher, Mode, Outcome, parse_keys},
};
use tempfile::TempDir;

use crate::{
    hints::dim,
    history::Again,
    insert::Draft,
    messages::{STICKY, Tone},
    model::Model,
    msg::Msg,
    press::{Answer, repeats_over, writes_line},
    rows::Row,
    test_util::{alone, deps, files, keys, reload, screen, started, workdir},
    track::Place,
};

/// The lines of a todo file; one may come more than once, as twins.
const LINES: [&str; 12] = [
    "A",
    "(A) B +p @c",
    "(C) C due:2026-09-30",
    "x 2026-09-01 D",
    "x 2026-09-01 E wontdo:true",
    "F started:2026-09-29T0800Z",
    "G started:2026-09-29T0700Z owner:agent2",
    "Blocker id:b",
    "H dep:b",
    "I t:2099-01-01",
    "Read https://x.org/a",
    "",
];

/// The done file, shown when the config shows archived tasks.
const DONE: &str = "x 2026-09-01 Old\nx 2026-09-01 Kept id:o\n";

/// `stop` has no default key; `+` and `-` step the priority, as they did when
/// the seeds were found.
const BINDINGS: &str = "\
[normal]\nstop = [\"<F9>\"]\n\
cycle_priority = [\"gp\", \"+\"]\ncycle_priority_back = [\"gP\", \"-\"]\n\
[visual]\nstop = [\"<F9>\"]\n\
cycle_priority = [\"gp\", \"+\"]\ncycle_priority_back = [\"gP\", \"-\"]\n";

/// The keys pressed: a key, and what is typed after it.
const KEYS: [(&str, &str); 38] = [
    ("x", ""),
    ("t", ""),
    ("T", ""),
    ("dd", ""),
    ("d", ""),
    ("<Del>", ""),
    ("-", ""),
    ("+", ""),
    ("gp", ""),
    ("<F9>", ""),
    (".", ""),
    ("u", ""),
    ("<C-r>", ""),
    ("J", ""),
    ("K", ""),
    ("yy", ""),
    ("y", ""),
    ("p", ""),
    ("P", ""),
    ("j", ""),
    ("k", ""),
    ("G", ""),
    ("gg", ""),
    ("v", ""),
    ("V", ""),
    ("<Esc>", ""),
    ("2", ""),
    ("3", ""),
    ("s", ""),
    ("gc", ""),
    ("/", "A<CR>"),
    ("/", "zzz<CR>"),
    ("<C-w>2", ""),
    ("<C-w>3", ""),
    ("<C-w>1", ""),
    ("gd", "2030-01-01<CR>"),
    ("gt", "k:v<CR>"),
    ("a", "Z<CR>"),
];

/// The keys pressed on a list whose file changed behind its back.
const STALE: [&str; 8] = ["x", "t", "T", "<Del>", "-", "+", "<F9>", "."];

/// A step byte at or above this changes the file behind the list's back.
const REWRITE: u8 = 0xF0;

/// A step byte in this range is a `:` command on one task, its low bits
/// picking the command (see `colon`). The range is none of the bytes the
/// seeds hold, so the seeds keep their keys.
const COLON: std::ops::RangeInclusive<u8> = 0xD1..=0xD6;

/// A step byte in this range is a rewrite as one at `REWRITE` is, but the
/// key pressed after it is an undo, or with bit 3 a redo, and no byte
/// follows but the line. The range is none of the bytes the seeds hold.
const AFTER: std::ops::RangeInclusive<u8> = 0xA0..=0xAF;

/// A list and the folder of its files.
struct Side {
    dir: TempDir,
    model: Model,
    /// Where each line of the file another writer changed went, while the
    /// list has not read it: none when it changed, went or got a twin.
    moved: Vec<Vec<usize>>,
    /// While its `<CR>` is checked, the lines the key that opened a field
    /// was to write on.
    opened: Vec<(usize, usize)>,
}

impl Side {
    fn new(todo: &[u8], config: &Config, width: u16) -> Self {
        let dir = workdir(todo);
        for (name, text) in [("keys.toml", BINDINGS), ("done.txt", DONE)] {
            fs::write(dir.path().join(name), text)
                .unwrap_or_else(|e| panic!("{e}"));
        }
        let mut model = started(deps(dir.path(), config.clone()));
        model.update(Msg::Resize(width, 24));
        let (moved, opened) = (Vec::new(), Vec::new());
        Self {
            dir,
            model,
            moved,
            opened,
        }
    }

    fn files(&self) -> [Vec<u8>; 2] {
        files(self.dir.path())
    }

    /// Presses `key` and checks it against its hint: a dim key changes
    /// nothing but its message (see [`Side::kept`]); a lit one that writes
    /// tasks either writes, or writes nothing and says why as an alert; and
    /// a key writes only on the lines the person saw (see [`Side::seen`]).
    /// A key its mode reads as two, one that acts or is dropped before its
    /// last stroke (`dd` in VISUAL: `d`, then a `d` of NORMAL), is checked
    /// as each in turn.
    fn checked(&mut self, key: &str, trail: &[String]) {
        let Some(at) = first_key(&self.model, key) else {
            return self.pressed(key, trail);
        };
        self.checked(&key[..at], trail);
        self.checked(&key[at..], trail);
    }

    /// [`Side::checked`] of a key its mode reads as one.
    fn pressed(&mut self, key: &str, trail: &[String]) {
        let asked = asked(&self.model, key);
        let writes = !self.opened.is_empty()
            || asked.is_some_and(|(a, _)| writes(&self.model, a));
        let aim = asked.map(|(a, _)| self.model.aimed(a));
        let history = matches!(aim, Some(Action::Undo | Action::Redo));
        let moves =
            matches!(aim, Some(Action::MoveTaskDown | Action::MoveTaskUp));
        let seen = self.seen(key);
        let sticky = self.model.rows.iter().any(|r| r.sticky);
        let before = self.kept();
        // The strokes before the last wait for it, and, like any key, let go
        // of the sticky tasks: a message names the rows left after them.
        let at = last_stroke(key).unwrap_or(0);
        keys(&mut self.model, &key[..at]);
        let was = shown(&self.model);
        keys(&mut self.model, &key[at..]);
        screen(&self.model, self.model.width, self.model.height);
        let m = &self.model;
        let name = format!(
            "{key} {asked:?} said {:?} after {trail:?}",
            m.message.text()
        );
        let wrote = self.files();
        let said = m.message.text();
        let way = aim.filter(|_| history);
        let press = Press {
            said,
            was: &was,
            seen: &seen,
            way,
            name: &name,
        };
        let written = self.named(&press, &before.0);
        assert!(
            history || kept_lines(&before.0, &wrote, &seen, &written, !moves),
            "wrote on lines not seen: {name}"
        );
        match asked {
            Some((_, answer)) if dim(answer) => {
                let mut kept = self.kept();
                // Like any key, a dim one lets go of the sticky tasks as the
                // timer would: the focus then leaves a panel that has
                // nothing to show for the list.
                let let_go = sticky && !m.rows.iter().any(|r| r.sticky);
                if let_go && kept.6 == Mode::Normal {
                    kept.6 = before.6;
                }
                assert_eq!(kept, before, "dim {name}");
            }
            Some(_) if writes && self.list() => {
                let alert = m.message.tone == Tone::Alert
                    && m.message.text().is_some();
                assert!(alert || wrote != before.0, "lit {name}");
            }
            _ => {}
        }
    }

    /// The lines on disk a lit `key` may write on, as a file and a line:
    /// those of the rows it acts on, followed to where another writer moved
    /// them when the list has not read its change; for `<CR>`, those of the
    /// key that opened the field. None once a reading changed the lines
    /// under the keys typed or the selection.
    fn seen(&self, key: &str) -> Vec<(usize, usize)> {
        if !self.opened.is_empty() {
            return self.opened.clone();
        }
        let m = &self.model;
        let Some((action, _)) = asked(m, key) else {
            return Vec::new();
        };
        if m.keys_lost || !writes_line(m.aimed(action)) {
            return Vec::new();
        }
        let again = m.again.as_ref().filter(|_| action == Action::Repeat);
        let typed = m.typed_count();
        let count = again.map_or(typed, |a| repeats_over(a, typed));
        let rows = m.rows_of(count).iter().map(|r| (r.source, r.nr.index()));
        rows.flat_map(|(source, at)| match source {
            Source::Done => vec![(1, at)],
            Source::Todo if !self.moved.is_empty() => {
                let to = self.moved.get(at).cloned().unwrap_or_default();
                to.into_iter().map(|at| (0, at)).collect()
            }
            Source::Todo => vec![(0, at)],
        })
        .collect()
    }

    /// The lines a message names after the write (a dependent unblocked or
    /// relinked, by its place now); a delete names a row of the list as it
    /// was before the key, one the key acted on, when no one changed the file
    /// behind the list. Every task it names by its place is checked by
    /// `history_named`, an undo or redo by what it repeats.
    fn named(&self, press: &Press<'_>, before: &Files) -> Vec<(usize, usize)> {
        let (said, name) = (press.said.unwrap_or_default(), press.name);
        let body = press.way.and_then(|_| {
            let (head, body) = said.split_once(": ")?;
            ["undid", "redid"].contains(&head).then_some(body)
        });
        self.history_named(press, before, body.unwrap_or(said));
        if body.is_some() {
            return Vec::new();
        }
        let now = shown(&self.model);
        let gone = if self.moved.is_empty() {
            lines_of(&deleted(Some(said)), press.was, name)
        } else {
            Vec::new()
        };
        let seen = press.seen;
        assert!(
            seen.is_empty() || gone.iter().all(|line| seen.contains(line)),
            "named a row the key did not act on: {name}"
        );
        ["unblocked ", "relinked "]
            .map(|word| lines_of(&after(Some(said), word), &now, name))
            .concat()
    }

    /// Every task a message names by its place (see `named_places`) is a
    /// row, of the list shown now or of the one before the key, and, unless
    /// its clause says the task was already as asked or left alone (`changed
    /// since`), or a key that is no undo or redo wrote nothing, a line the
    /// write changed on disk. In such a clause of an ordinary press, other
    /// than `changed since`, the task is a row the key acted on. Not for a
    /// press of a file that changed behind the list, but the tasks it
    /// unblocked or relinked, which are rows of the list it left: a place of
    /// the list the key read cannot be rebuilt.
    fn history_named(&self, press: &Press<'_>, before: &Files, body: &str) {
        let stale = !self.moved.is_empty();
        let (now, after) = (shown(&self.model), self.files());
        let idle = press.way.is_none() && *before == after;
        for (n, said, listed) in named_places(body) {
            if stale && !listed {
                continue;
            }
            let lists = if stale {
                vec![&now]
            } else {
                vec![&now, press.was]
            };
            let rows: Vec<_> =
                lists.iter().filter_map(|r| r.get(&n)).collect();
            assert!(!rows.is_empty(), "no row {n}: {}", press.name);
            assert!(
                said != Said::Changed
                    || stale
                    || idle
                    || rows.iter().any(|at| touched(before, &after, **at)),
                "{n} unchanged: {}",
                press.name
            );
            let asked = said == Said::Already || (idle && said != Said::Left);
            let acted = |at| press.seen.contains(at);
            assert!(
                !asked
                    || listed
                    || press.way.is_some()
                    || press.seen.is_empty()
                    || press.was.get(&n).is_some_and(acted),
                "{n} not acted on: {}",
                press.name
            );
        }
    }

    /// Whether the list is shown, no field open.
    fn list(&self) -> bool {
        matches!(self.model.key_mode(), Mode::Normal | Mode::Visual)
    }

    /// What a dim key keeps: the files, the list it holds, the undo steps,
    /// whether the window of `t` is open, the selection, what `.` repeats
    /// and the mode.
    fn kept(&self) -> Kept {
        let m = &self.model;
        (
            self.files(),
            m.snap.serial(),
            (m.hist.undo.len(), m.hist.redo.len()),
            m.hist.stopped.is_some(),
            m.selected_rows()
                .map(|(a, b)| m.rows[a..=b].iter().map(Row::place).collect()),
            m.again.clone(),
            m.key_mode(),
        )
    }
}

/// The action a key runs and what its hint says, when it runs one.
type Hint = Option<(Action, Answer)>;

/// The bytes of the todo file and the done file.
type Files = [Vec<u8>; 2];

/// A press and what it said.
struct Press<'a> {
    said: Option<&'a str>,
    /// The places of the rows before the key.
    was: &'a HashMap<usize, (usize, usize)>,
    /// The lines the key acts on.
    seen: &'a [(usize, usize)],
    /// Undo or redo, when the key is one.
    way: Option<Action>,
    name: &'a str,
}

type Kept = (
    Files,
    u32,
    (usize, usize),
    bool,
    Option<Vec<Place>>,
    Option<Again>,
    Mode,
);

/// The action `key` runs now, as the list's own matcher reads it, and what
/// its hint says; `None` when it runs no action.
fn asked(model: &Model, key: &str) -> Hint {
    let Some(Outcome::Matched { action, .. }) = fed(model, key) else {
        return None;
    };
    Some((action, model.answers()(action)))
}

/// What the list's own matcher makes of `key` at its last stroke.
fn fed(model: &Model, key: &str) -> Option<Outcome> {
    let mut matcher = model.matchers.get(&model.key_mode())?.clone();
    let strokes = parse_keys(key, None).ok()?;
    strokes.into_iter().map(|s| matcher.feed(s).outcome).last()
}

/// The length of the first of the keys `key` is, when the matcher of the
/// mode stops waiting before its last stroke: it runs an action, or drops
/// the keys pending.
fn first_key(model: &Model, key: &str) -> Option<usize> {
    let all = parse_keys(key, None).ok()?;
    (1..key.len()).find(|&at| {
        let head = key.get(..at).and_then(|h| parse_keys(h, None).ok());
        head.is_some_and(|h| all.starts_with(&h))
            && fed(model, &key[..at]).is_some_and(|o| o != Outcome::Pending)
    })
}

/// Where the last of the strokes of `key` starts, when it has more.
fn last_stroke(key: &str) -> Option<usize> {
    let all = parse_keys(key, None).ok()?;
    (1..key.len()).rev().find(|&at| {
        let head = key.get(..at).and_then(|h| parse_keys(h, None).ok());
        head.is_some_and(|h| h.len() + 1 == all.len() && all.starts_with(&h))
    })
}

/// Whether `action`, lit now, changes the bytes of the files: undo, redo
/// and the keys that write on their lines, but `<Esc>` and `.` of `t`,
/// which cannot be sure, and a commit of a field with nothing typed.
fn writes(model: &Model, action: Action) -> bool {
    let again = model.again.as_ref().filter(|_| action == Action::Repeat);
    let unchanged = model.edit.as_ref().is_some_and(Draft::unchanged);
    let unsure = action == Action::Cancel
        || action == Action::Commit && unchanged
        || again.is_some_and(|a| a.action() == Action::ToggleStart);
    let history = matches!(action, Action::Undo | Action::Redo);
    (history || writes_line(action)) && !unsure
}

/// Changes the todo file in `dir` as another writer would: a line `arg`
/// picks gets more text, goes, or gets a twin, or a line comes first.
/// Tells where each line went: none when it changed, went or got a twin.
fn rewrite(dir: &Path, op: u8, arg: usize) -> Vec<Vec<usize>> {
    let path = dir.join("todo.txt");
    let text = fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let n = lines.len();
    let at = arg % n.max(1);
    let op = if at < n { op % 4 } else { 3 };
    match op {
        0 => lines[at].push_str(" +r"),
        1 => drop(lines.remove(at)),
        2 => lines.insert(at, lines[at].clone()),
        _ => lines.insert(0, LINES[arg % LINES.len()].into()),
    }
    let text = lines.iter().fold(String::new(), |all, l| all + l + "\n");
    fs::write(&path, text).unwrap_or_else(|e| panic!("{e}"));
    let went = |i: usize| match (op, i.cmp(&at)) {
        (0..=2, Ordering::Equal) => Vec::new(),
        (1, Ordering::Greater) => vec![i - 1],
        (2, Ordering::Greater) | (3, _) => vec![i + 1],
        _ => vec![i],
    };
    (0..n).map(went).collect()
}

/// Another writer changes the file between the count and the key, and the
/// list reads it then when `op` says the watcher does, else only after the
/// key.
fn behind(
    side: &mut Side,
    (op, arg, count, key): (u8, usize, &str, &str),
    trail: &[String],
) {
    if side.model.matchers.values().any(Matcher::cancels) {
        keys(&mut side.model, "<Esc>");
    }
    keys(&mut side.model, count);
    let moved = rewrite(side.dir.path(), op, arg);
    if op & 4 == 0 {
        side.moved = moved;
    } else {
        reload(&mut side.model);
    }
    side.checked(key, trail);
    side.moved.clear();
    reload(&mut side.model);
}

/// Whether a press kept every line of `before` but the `seen` ones (a file
/// and a line each): all still there, in their order, and, `in_place`
/// when it moves no line and keeps their number, each where it was. Lines
/// it adds may go anywhere. A `written` line, a dependent the press says it
/// unblocked or relinked (by its line after the write), may change in its
/// `dep:` but nowhere else.
fn kept_lines(
    before: &[Vec<u8>; 2],
    after: &[Vec<u8>; 2],
    seen: &[(usize, usize)],
    written: &[(usize, usize)],
    in_place: bool,
) -> bool {
    (0..2).all(|file| {
        let was: Vec<_> = before[file].split(|b| *b == b'\n').collect();
        let now: Vec<_> = after[file].split(|b| *b == b'\n').collect();
        let mut others = was
            .iter()
            .enumerate()
            .filter(|(i, _)| !seen.contains(&(file, *i)));
        let same = |line: &[u8], (j, now): (usize, &[u8])| {
            now == line
                || written.contains(&(file, j))
                    && but_deps(now) == but_deps(line)
        };
        if in_place && was.len() == now.len() {
            others.all(|(i, line)| same(line, (i, now[i])))
        } else {
            let mut rest = now.iter().map(|l| &l[..]).enumerate();
            others.all(|(_, line)| rest.any(|l| same(line, l)))
        }
    })
}

/// The file and line of each row shown, by its place.
fn shown(model: &Model) -> HashMap<usize, (usize, usize)> {
    let rows = model.rows.iter().enumerate();
    rows.map(|(i, r)| {
        let file = usize::from(r.source == Source::Done);
        (model.place(i), (file, r.nr.index()))
    })
    .collect()
}

/// The lines of `rows` that the names in a message are the places of; a
/// place that is no row fails. A task with no row is named by its line
/// (`line N`, `done:N`), which no row may show: a row is named by its place.
fn lines_of(
    names: &[String],
    rows: &HashMap<usize, (usize, usize)>,
    name: &str,
) -> Vec<(usize, usize)> {
    let line = |n: &str| n.parse::<usize>().ok().map(|n| n.saturating_sub(1));
    let unshown = |file, n: &str| {
        let at = (file, line(n)?);
        assert!(!rows.values().any(|r| *r == at), "{n} is a row: {name}");
        Some(at)
    };
    let place = |n: &str| {
        let at =
            |n| rows.get(&n).unwrap_or_else(|| panic!("no row {n}: {name}"));
        n.parse().ok().map(|n| *at(n))
    };
    let lines = names.iter().filter_map(|n| match n.strip_prefix("line ") {
        Some(n) => unshown(0, n),
        None => match n.strip_prefix("done:") {
            Some(n) => unshown(1, n),
            None => place(n),
        },
    });
    lines.collect()
}

/// The names a message gives after `word`, up to the next `;`.
fn after(message: Option<&str>, word: &str) -> Vec<String> {
    let said = message.unwrap_or_default().split(word).skip(1);
    let names =
        said.flat_map(|s| s.split(';').next().unwrap_or_default().split(", "));
    names.map(|n| n.trim().to_owned()).collect()
}

/// What a clause says of a task it names: changed, already as asked, or
/// left alone by a paste (`changed since`).
#[derive(Clone, Copy, PartialEq)]
enum Said {
    Changed,
    Already,
    Left,
}

/// The tasks the clauses of `body` name by their place: the first number of
/// a clause (`N`, `task N`; not a line, `line N`, or a count, `N task` or
/// `N tasks`), the tasks after `unblocked ` and `relinked ` (`listed`), and
/// the row after ` moved to `, ` moved back from ` and ` reopened as `. Each
/// with what its clause says of the task, and whether it is listed.
fn named_places(body: &str) -> Vec<(usize, Said, bool)> {
    let body = body.split(STICKY).next().unwrap_or(body);
    let first = |c: &str| {
        let words: Vec<_> = c.trim().split([' ', ',']).collect();
        let skip = usize::from(words[0] == "task");
        let next = words.get(skip + 1);
        let counted = next.is_some_and(|n| ["task", "tasks"].contains(n));
        words[skip].parse().ok().filter(|_| !counted)
    };
    let clause = |c: &str| {
        let said = if c.contains(" already ") {
            Said::Already
        } else if c.contains(" changed since") {
            Said::Left
        } else {
            Said::Changed
        };
        let at = |words: &[&str], listed| {
            let names: Vec<_> =
                words.iter().flat_map(|w| after(Some(c), w)).collect();
            let numbers: Vec<usize> =
                names.iter().filter_map(|n| n.parse().ok()).collect();
            let mut once = numbers.clone();
            once.sort_unstable();
            once.dedup();
            assert_eq!(once.len(), numbers.len(), "{c:?} names a task twice");
            numbers
                .into_iter()
                .map(|n| (n, said, listed))
                .collect::<Vec<_>>()
        };
        let mut named = at(&["unblocked ", "relinked "], true);
        let rows = [" moved to ", " moved back from ", " reopened as "];
        named.extend(at(&rows, false));
        named.extend(first(c).map(|n| (n, said, false)));
        named
    };
    body.split(';').flat_map(clause).collect()
}

/// Whether the line `at` (a file and a line) is not as it was.
fn touched(before: &Files, after: &Files, (file, at): (usize, usize)) -> bool {
    let line = |text: &Files| {
        text[file]
            .split(|b| *b == b'\n')
            .nth(at)
            .map(<[u8]>::to_vec)
    };
    line(before) != line(after)
}

/// The names of the parts of a message that say `N deleted`.
fn deleted(message: Option<&str>) -> Vec<String> {
    let parts = message.unwrap_or_default().split(';');
    parts
        .filter_map(|p| Some(p.trim().strip_suffix(" deleted")?.to_owned()))
        .collect()
}

/// A `:` command on one task, the byte after `op` picking the row: `do` or
/// `del` of a row by its place, by its `id:` or of the place after the last
/// row, which can be a line of the file. It must write on the row at that
/// place and no other line, and name that row; past the last row it is no
/// task and writes nothing.
fn colon(side: &mut Side, op: u8, arg: u8, trail: &mut Vec<String>) {
    let how = usize::from(op - COLON.start());
    trail.push(":".to_owned());
    side.checked(":", trail);
    if side
        .model
        .line
        .as_ref()
        .is_none_or(|l| !l.field.text.is_empty())
    {
        return;
    }
    // the list as it is when `<CR>` runs the command: `:` lets go of the
    // sticky tasks
    let rows = shown(&side.model);
    let (by_id, past) = (how / 2 == 1, how / 2 == 2);
    let place = if past {
        rows.len() + 1
    } else {
        1 + usize::from(arg) % rows.len().max(1)
    };
    let row = rows.get(&place).copied();
    let files = side.files();
    let id = row.and_then(|(file, at)| {
        let line = files[file].split(|b| *b == b'\n').nth(at)?;
        let tag =
            line.split(|b| *b == b' ').find(|w| w.starts_with(b"id:"))?;
        Some(String::from_utf8_lossy(tag).into_owned())
    });
    let target = id.filter(|_| by_id).unwrap_or_else(|| place.to_string());
    let text = format!("{} {target}", ["do", "del"][how % 2]);
    trail.push(format!("{text}<CR>"));
    keys(&mut side.model, &text);
    side.opened = row.into_iter().collect();
    side.checked("<CR>", trail);
    side.opened.clear();
    let said = side.model.message.text();
    let Some(row) = row else {
        let want = format!("no task {place}");
        assert_eq!(said, Some(want.as_str()), "{trail:?}");
        return;
    };
    // a place is named whatever the outcome; an `id:` may be refused as
    // ambiguous, so it is named when it wrote
    if !by_id || side.files() != files {
        // the row where it is now, else where it was
        let now = shown(&side.model);
        let at = now
            .iter()
            .find(|(_, r)| **r == row)
            .map_or(place, |(p, _)| *p);
        let said = said.unwrap_or_default();
        let named = [format!("{at} "), format!("task {at} ")];
        assert!(
            named.iter().any(|n| said.starts_with(n)),
            "{said:?} is not row {at}: {trail:?}"
        );
    }
}

/// The words of a line but its `dep:` tags.
fn but_deps(line: &[u8]) -> Vec<&[u8]> {
    line.split(|b| *b == b' ')
        .filter(|w| !w.starts_with(b"dep:"))
        .collect()
}

/// Builds a todo file and a run of keys from `data` and presses them over
/// a list, each key checked against its hint, and sometimes changes the
/// file behind the list's back.
///
/// The first byte picks the config, one bit each: the sidebar (1), the
/// archived tasks (2), the width (4) and the detail panel (8). The second,
/// modulo 7 and capped at the bytes left, is how many bytes follow it, each
/// picking a line of the file. Each byte after those is a key; one at `0xF0`
/// and above is, while the list is shown, a rewrite instead, its low bits
/// picking which (see `rewrite`, `behind`): the next byte picks the
/// line, the two after it the count and the key pressed then. One in
/// `AFTER` is the same with an undo or a redo pressed then, and one byte
/// after it. One in `COLON` is, while the list is shown, a `:` command
/// instead (see `colon`): the next byte picks its row.
///
/// # Panics
///
/// When a key does not do what its hint said.
pub fn hints_property(data: &[u8]) {
    let [setup, lines, rest @ ..] = data else {
        return;
    };
    let lines = usize::from(*lines % 7).min(rest.len());
    let (picked, steps) = rest.split_at(lines);
    let todo = picked.iter().fold(String::new(), |all, b| {
        all + LINES[usize::from(*b) % LINES.len()] + "\n"
    });
    let config = Config {
        show_sidebar: setup & 1 != 0,
        show_detail: setup & 8 != 0,
        hidden_tasks: if setup & 2 == 0 {
            alone().hidden_tasks
        } else {
            Vec::new()
        },
        ..alone()
    };
    let width = if setup & 4 == 0 { 60 } else { 120 };
    let mut side = Side::new(todo.as_bytes(), &config, width);
    let mut trail = vec![format!("{todo:?} {setup}")];
    let mut bytes = steps.iter().copied();
    while let Some(op) = bytes.next() {
        if COLON.contains(&op) && side.list() {
            let arg = bytes.next().unwrap_or(0);
            colon(&mut side, op, arg, &mut trail);
        } else if (op >= REWRITE || AFTER.contains(&op)) && side.list() {
            let mut next = || usize::from(bytes.next().unwrap_or(0));
            let arg = next();
            let (count, key) = if op >= REWRITE {
                (["", "2", "3"][next() % 3], STALE[next() % STALE.len()])
            } else {
                ("", ["u", "<C-r>"][usize::from(op & 8 != 0)])
            };
            trail.push(format!("rewrite {op} {arg} {count}{key}"));
            behind(&mut side, (op, arg, count, key), &trail);
        } else {
            let (key, then) = KEYS[usize::from(op) % KEYS.len()];
            trail.push(format!("{key}{then}"));
            let opens = side.seen(key);
            side.checked(key, &trail);
            match then.strip_suffix("<CR>") {
                Some(text) if !opens.is_empty() && !side.list() => {
                    keys(&mut side.model, text);
                    side.opened = opens;
                    side.checked("<CR>", &trail);
                    side.opened.clear();
                }
                _ => {
                    keys(&mut side.model, then);
                }
            }
        }
    }
}
