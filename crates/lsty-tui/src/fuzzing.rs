//! The property the fuzz target `hints` checks: a run of keys over a list,
//! the files sometimes changed behind its back, against what the hint of
//! each key said the key would do.

use std::{fs, path::Path};

use lsty_core::{
    config::Config,
    errs::Source,
    keys::{Action, Matcher, Mode, Outcome, parse_keys},
    ops::LIST_CHANGED,
};
use tempfile::TempDir;

use crate::{
    hints::dim,
    history::Again,
    insert::Draft,
    messages::Tone,
    model::Model,
    msg::{Cmd, Msg},
    press::{Answer, repeats_over, writes_line},
    test_util::{
        alone, deps, files, hint_is, keys, reload, same_as, screen, started,
        workdir,
    },
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
    /// What the last checked press asked for.
    cmds: Vec<Cmd>,
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
        Self {
            dir,
            model,
            cmds: Vec::new(),
        }
    }

    fn files(&self) -> [Vec<u8>; 2] {
        files(self.dir.path())
    }

    fn screen(&self) -> Vec<String> {
        screen(&self.model, self.model.width, self.model.height)
    }

    /// Presses `key` and checks it against its hint: a dim key keeps the
    /// files, the mode, the selection and what `.` repeats; a lit one that
    /// writes tasks, and opens no field to, changes the files and refuses
    /// none. On a `stale` list, which reads the files as it presses, or when
    /// it says to look again, a dim key only writes nothing. Over keys or a
    /// selection whose lines a reading changed, a key that writes on its
    /// lines writes nothing, says to look again and ends the selection.
    /// Tells what the hint said.
    fn checked(&mut self, key: &str, stale: bool, trail: &[String]) -> Hint {
        let asked = asked(&self.model, key);
        let writes = asked.is_some_and(|(a, _)| writes(&self.model, a));
        let lost = self.model.keys_lost
            && asked.is_some_and(|(a, _)| writes_line(self.model.aimed(a)));
        let before = self.kept();
        self.cmds = keys(&mut self.model, key);
        self.screen();
        let said = &self.model.message;
        let looked = stale || said.as_deref() == Some(LIST_CHANGED);
        let name = format!("{key} {asked:?} said {said:?} after {trail:?}");
        match asked {
            Some((_, answer)) if dim(answer) && looked => {
                assert_eq!(self.files(), before.0, "dim {name}");
            }
            Some((_, answer)) if dim(answer) => {
                assert_eq!(self.kept(), before, "dim {name}");
            }
            Some(_) if lost => self.said_look_again(&before.0, &name),
            Some(_) if !stale && self.list() && writes => {
                assert_ne!(self.files(), before.0, "lit {name}");
                assert_ne!(self.model.tone, Tone::Alert, "lit {name}");
            }
            _ => {}
        }
        asked
    }

    /// The press wrote nothing, said to look again and ended the selection.
    fn said_look_again(&self, before: &[Vec<u8>; 2], name: &str) {
        let m = &self.model;
        let ended = (m.message.as_deref(), &self.files(), m.key_mode());
        let looks = (Some(LIST_CHANGED), before, Mode::Normal);
        assert_eq!(ended, looks, "lost {name}");
    }

    /// Whether the list is shown, no field open.
    fn list(&self) -> bool {
        matches!(self.model.key_mode(), Mode::Normal | Mode::Visual)
    }

    /// What a dim key keeps.
    fn kept(&self) -> Kept {
        let m = &self.model;
        (
            self.files(),
            m.key_mode(),
            m.selected_rows(),
            m.again.clone(),
        )
    }
}

/// The action a key runs and what its hint says, when it runs one.
type Hint = Option<(Action, Answer)>;

type Kept = ([Vec<u8>; 2], Mode, Option<(usize, usize)>, Option<Again>);

/// The action `key` runs now, as the list's own matcher reads it, and what
/// its hint says; `None` when it runs no action.
fn asked(model: &Model, key: &str) -> Hint {
    let mut matcher = model.matchers.get(&model.key_mode())?.clone();
    let strokes = parse_keys(key, None).ok()?;
    let fed = strokes.into_iter().map(|s| matcher.feed(s)).last()?;
    let Outcome::Matched { action, .. } = fed.outcome else {
        return None;
    };
    Some((action, model.answers()(action)))
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

/// The lines under `key` in NORMAL mode, when it is lit and writes on them
/// (`.` on those it repeats over), that the todo file holds once: one
/// changed, gone or twinned after that is one the key must look again for.
fn under(model: &Model, key: &str, todo: &[u8]) -> Vec<Vec<u8>> {
    let Some((action, answer)) = asked(model, key) else {
        return Vec::new();
    };
    let line_key = writes_line(model.aimed(action));
    if dim(answer) || !line_key || model.key_mode() != Mode::Normal {
        return Vec::new();
    }
    let again = model.again.as_ref().filter(|_| action == Action::Repeat);
    let count = again.map_or_else(
        || model.typed_count(),
        |a| repeats_over(a, model.typed_count()),
    );
    let rows = model.rows_of(count).iter();
    rows.filter_map(|row| model.snap.task(row.id))
        .map(|task| model.snap.line(task).raw().to_vec())
        .filter(|line| twins(todo, line) == 1)
        .collect()
}

/// How many lines of `todo` are `line`.
fn twins(todo: &[u8], line: &[u8]) -> usize {
    todo.split(|b| *b == b'\n').filter(|l| *l == line).count()
}

/// Changes the todo file in `dir` as another writer would: a line `arg`
/// picks gets more text, goes, or gets a twin, or a line comes first.
fn rewrite(dir: &Path, op: u8, arg: usize) {
    let path = dir.join("todo.txt");
    let text = fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let at = arg % lines.len().max(1);
    let there = at < lines.len();
    match op % 4 {
        0 if there => lines[at].push_str(" +r"),
        1 if there => drop(lines.remove(at)),
        2 if there => lines.insert(at, lines[at].clone()),
        _ => lines.insert(0, LINES[arg % LINES.len()].into()),
    }
    let text = lines.iter().fold(String::new(), |all, l| all + l + "\n");
    fs::write(&path, text).unwrap_or_else(|e| panic!("{e}"));
}

/// The list and its twin read the same rewrite: the twin at once, the list
/// when `op` says the watcher reads between `count` and `key`, else only
/// when `key` looks. The list either does what the twin did, or writes
/// nothing and says to look again, as it must when a line under a lit key
/// changed; then, the reading it owes taken, its next press does what the
/// twin did. A paste may also land beside another copy of its line than the
/// twin's (see [`beside_a_copy`]): then the two differ and, `false`, the
/// run ends.
fn behind(
    (main, twin): (&mut Side, &mut Side),
    (op, arg, count, key): (u8, usize, &str, &str),
    trail: &[String],
) -> bool {
    if main.model.matchers.values().any(Matcher::cancels) {
        keys(&mut main.model, "<Esc>");
        keys(&mut twin.model, "<Esc>");
    }
    keys(&mut main.model, count);
    let typed = count;
    // A digit after a key that matched nothing is no count.
    let count = if main.model.typed_count() == 0 {
        ""
    } else {
        count
    };
    // The watcher reads between the count and the key: the lines it
    // shows bind only a key typed before it.
    let read = op & 4 != 0;
    let under = if read && count.is_empty() {
        Vec::new()
    } else {
        under(&main.model, key, &main.files()[0])
    };
    rewrite(main.dir.path(), op, arg);
    rewrite(twin.dir.path(), op, arg);
    let moved_on = main.files();
    reload(&mut twin.model);
    keys(&mut twin.model, typed);
    if read {
        reload(&mut main.model);
    }
    let fresh = twin.checked(key, false, trail);
    let seen = pastes_beside(&main.model, key);
    let stale = main.checked(key, true, trail);
    let looked = main.model.message.as_deref() == Some(LIST_CHANGED)
        && main.files() == moved_on;
    // A key the watcher's reading drew dim says its own words.
    let dim_now = read && fresh.is_some_and(|(_, answer)| dim(answer));
    let lost = !dim_now && under.iter().any(|l| twins(&moved_on[0], l) != 1);
    assert!(looked || !lost, "a line under the key changed: {trail:?}");
    if looked {
        looked_again((main, twin), (fresh, stale), (count, key), trail);
    } else if main.model.message == twin.model.message
        && main.model.tone == twin.model.tone
        && seen.is_some_and(|s| {
            beside_a_copy(&s, &moved_on, &main.files(), &twin.files())
        })
    {
        return false;
    }
    // A key that did not look leaves the reading to the watcher.
    reload(&mut main.model);
    let (m, t) = (main.dir.path(), twin.dir.path());
    same_as((m, &main.model), (t, &twin.model), &format!("{trail:?}"));
    true
}

/// For a lit `p` or `P` (or a `.` of one), whether it pastes below, and the
/// line under the cursor that it pastes beside.
fn pastes_beside(model: &Model, key: &str) -> Option<(bool, Vec<u8>)> {
    let (action, _) = asked(model, key)?;
    let aim = model.aimed(action);
    let below = aim == Action::PasteBelow;
    if !below && aim != Action::PasteAbove {
        return None;
    }
    let row = model.rows.get(model.cursor);
    let row = row.filter(|r| r.source != Source::Done)?;
    let task = model.snap.task(row.id)?;
    Some((below, model.snap.line(task).raw().to_vec()))
}

/// A stale paste that went in beside another copy of the line under it
/// than the twin's paste, which read first and so had its cursor on the
/// copy the reading's rule picks: right, since a paste lands beside the
/// copy the person saw. Only when the list's todo file is `before` with
/// the same lines the twin pasted put in beside a copy of `seen` there,
/// `before` holds `seen` more than once, and the done files are alike: so
/// no line of `before` was written on, moved or dropped. Any copy of `seen`
/// passes, not only the one the diff pairs (left to `lineid`'s tests), and
/// the run ends there.
fn beside_a_copy(
    (below, seen): &(bool, Vec<u8>),
    before: &[Vec<u8>; 2],
    list: &[Vec<u8>; 2],
    twin: &[Vec<u8>; 2],
) -> bool {
    let [was, now, other] = [&before[0], &list[0], &twin[0]]
        .map(|t| t.split(|b| *b == b'\n').collect::<Vec<_>>());
    let n = other.len().saturating_sub(was.len());
    let put = |at: usize, block| [&was[..at], block, &was[at..]].concat();
    let mut blocks = (0..was.len())
        .filter(|&at| put(at, &other[at..at + n]) == other)
        .map(|at| &other[at..at + n]);
    let copies = || (0..was.len()).filter(|&k| was[k] == seen.as_slice());
    n > 0
        && now != other
        && list[1] == twin[1]
        && copies().count() > 1
        && blocks.any(|block| {
            copies().any(|k| put(k + usize::from(*below), block) == now)
        })
}

/// The list said to look again and wrote nothing: right only when its key
/// was lit, or the twin wrote, emptied or looked again too. Takes the
/// reading the list asked for, if its press had not read, and presses the
/// key again, now with the hint the twin had; when looking again ended a
/// selection, the twin ends it too, or looks again too, and presses again.
fn looked_again(
    (main, twin): (&mut Side, &mut Side),
    (fresh, stale): (Hint, Hint),
    (count, key): (&str, &str),
    trail: &[String],
) {
    let wrote = twin.files() != main.files();
    let was_dim = stale.is_some_and(|(_, answer)| dim(answer));
    let emptied = twin.model.rows.is_empty();
    let both = twin.model.message.as_deref() == Some(LIST_CHANGED);
    assert!(
        !was_dim || wrote || emptied || both,
        "looked again: {trail:?}"
    );
    for cmd in std::mem::take(&mut main.cmds) {
        if let Cmd::Load(load) = cmd {
            main.model.update(load.read());
        }
    }
    keys(&mut main.model, count);
    let mut fresh = fresh;
    if both {
        // The twin looked again too, its selection having lost its lines.
        keys(&mut twin.model, count);
        fresh = twin.checked(key, false, trail);
    } else if main.model.key_mode() != twin.model.key_mode() {
        // Looking again ended the selection, which the twin keeps.
        keys(&mut twin.model, "<Esc>");
        keys(&mut twin.model, count);
        fresh = twin.checked(key, false, trail);
    }
    let Some((action, answer)) = fresh else {
        panic!("{key} runs no action: {trail:?}");
    };
    let name = format!("hint after looking again: {trail:?}");
    hint_is(&main.model, action, Some(answer), &name);
    main.checked(key, false, trail);
}

/// Builds a todo file and a run of keys from `data` and presses them over
/// a list and its twin, each key checked against its hint, and sometimes
/// changes the file behind the list's back.
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
    let mut main = Side::new(todo.as_bytes(), &config, width);
    let mut twin = Side::new(todo.as_bytes(), &config, width);
    let mut trail = vec![format!("{todo:?} {setup}")];
    let mut bytes = steps.iter().copied();
    while let Some(op) = bytes.next() {
        if op >= REWRITE && main.list() {
            let mut next = || usize::from(bytes.next().unwrap_or(0));
            let (arg, count, key) =
                (next(), ["", "2", "3"][next() % 3], next());
            let key = STALE[key % STALE.len()];
            trail.push(format!("rewrite {op} {arg} {count}{key}"));
            if !behind((&mut main, &mut twin), (op, arg, count, key), &trail) {
                return;
            }
        } else {
            let (key, then) = KEYS[usize::from(op) % KEYS.len()];
            trail.push(format!("{key}{then}"));
            main.checked(key, false, &trail);
            keys(&mut twin.model, key);
            keys(&mut main.model, then);
            keys(&mut twin.model, then);
        }
    }
}
