//! The property the fuzz target `hints` checks: a run of keys over a list,
//! the files sometimes changed behind its back, against what the hint of
//! each key said the key would do.

use std::{cmp::Ordering, fs, path::Path};

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
    messages::Tone,
    model::Model,
    msg::Msg,
    press::{Answer, repeats_over, writes_line},
    test_util::{alone, deps, files, keys, reload, screen, started, workdir},
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

/// `stop` has no default key.
const BINDINGS: &str =
    "[normal]\nstop = [\"<F9>\"]\n[visual]\nstop = [\"<F9>\"]\n";

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
        keys(&mut self.model, key);
        screen(&self.model, self.model.width, self.model.height);
        let m = &self.model;
        let name =
            format!("{key} {asked:?} said {:?} after {trail:?}", m.message);
        let wrote = self.files();
        let named = named(m.message.as_deref());
        assert!(
            history || kept_lines(&before.0, &wrote, &seen, &named, !moves),
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
                let alert = m.tone == Tone::Alert && m.message.is_some();
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
            m.selected_rows(),
            m.again.clone(),
            m.key_mode(),
        )
    }
}

/// The action a key runs and what its hint says, when it runs one.
type Hint = Option<(Action, Answer)>;

type Kept = (
    [Vec<u8>; 2],
    u32,
    (usize, usize),
    bool,
    Option<(usize, usize)>,
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
/// it adds may go anywhere. A `named` line, a dependent the press says it
/// unblocked or relinked, may change in its `dep:` but nowhere else.
fn kept_lines(
    before: &[Vec<u8>; 2],
    after: &[Vec<u8>; 2],
    seen: &[(usize, usize)],
    named: &[(usize, usize)],
    in_place: bool,
) -> bool {
    (0..2).all(|file| {
        let was: Vec<_> = before[file].split(|b| *b == b'\n').collect();
        let now: Vec<_> = after[file].split(|b| *b == b'\n').collect();
        let mut others = was
            .iter()
            .enumerate()
            .filter(|(i, _)| !seen.contains(&(file, *i)));
        let same = |i: usize, line: &[u8], now: &[u8]| {
            now == line
                || named.contains(&(file, i))
                    && but_deps(now) == but_deps(line)
        };
        if in_place && was.len() == now.len() {
            others.all(|(i, line)| same(i, line, now[i]))
        } else {
            let mut rest = now.iter();
            others.all(|(i, line)| rest.any(|l| same(i, line, l)))
        }
    })
}

/// The todo lines a message names as dependents it unblocked or relinked,
/// by their number before the press.
fn named(message: Option<&str>) -> Vec<(usize, usize)> {
    let message = message.unwrap_or_default();
    let said = ["unblocked ", "relinked "]
        .into_iter()
        .flat_map(|word| message.split(word).skip(1));
    let nrs =
        said.flat_map(|s| s.split(';').next().unwrap_or_default().split(", "));
    let at = |n: &str| n.trim().parse::<usize>().ok()?.checked_sub(1);
    nrs.filter_map(at).map(|i| (0, i)).collect()
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
/// archived tasks (2), the width (4) and the detail panel (8); the second
/// how many lines follow, one byte each. Each byte after them is a key,
/// or at `0xF0` and above a rewrite: the next byte picks the line, the
/// two after it the count and the key pressed then.
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
        if op >= REWRITE && side.list() {
            let mut next = || usize::from(bytes.next().unwrap_or(0));
            let (arg, count, key) =
                (next(), ["", "2", "3"][next() % 3], next());
            let key = STALE[key % STALE.len()];
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
