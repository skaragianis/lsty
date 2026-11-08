//! The rule of the keys against the commands it names, on real lists.
//! Prompts open rather than write, `+` and `-` step each task from its own
//! priority, and the claim memory of `t` has frames of its own.

use std::{cell::Cell, fs, path::Path, rc::Rc, time::Duration};

use lsty_core::{
    config::{Config, Hidden},
    dates::Clock,
    errs::{self, Code, Source},
    keys::{Action, Mode},
    ops::{self, ClaimOpts, DoOpts, Outcome, PriPair, StartOpts, Target},
};
use tempfile::TempDir;

use crate::{
    actions::{Chosen, stepped},
    deps::Deps,
    hints::dim,
    messages::Tone,
    model::Model,
    press::{Answer, Command, Refusal, decide},
    test_util::{
        Board, NOW, alone, deps, files, instant, keys, press, started, workdir,
    },
};

/// The tasks, in the order of the rows. The first stays in the list, since
/// the second waits for it; the last two are archived.
const KINDS: [&str; 11] = [
    "Blocker id:blk",
    "Plain",
    "Waits dep:blk",
    "Mine started:2026-09-29T0800Z",
    "Theirs started:2026-09-29T0700Z owner:agent2",
    "(B) Ranked",
    "Daily rec:1d",
    "x 2026-09-01 Done",
    "x 2026-09-01 Skip wontdo:true",
    "x 2026-09-01 Old",
    "x 2026-09-01 Old skip wontdo:true",
];
const IN_DONE_FILE: usize = 9;

fn config() -> Config {
    Config {
        hidden_tasks: vec![Hidden::Later],
        ..alone()
    }
}

/// A list of the first kind and those of `kinds`, with the cursor on the
/// first of `kinds`.
fn list(kinds: &[usize]) -> (TempDir, Model) {
    let lines = |file: fn(usize) -> bool| -> String {
        let chosen = kinds.iter().copied().filter(|k| file(*k));
        let all = std::iter::once(0).chain(chosen.filter(|k| *k != 0));
        let lines = all.filter(|k| file(*k)).filter_map(|k| KINDS.get(k));
        lines.fold(String::new(), |out, line| out + line + "\n")
    };
    let dir = workdir(lines(|k| k < IN_DONE_FILE));
    let done = lines(|k| k >= IN_DONE_FILE);
    fs::write(dir.path().join("done.txt"), done)
        .unwrap_or_else(|e| panic!("{e}"));
    let mut model = started(deps(dir.path(), config()));
    if !kinds.contains(&0) {
        press(&mut model, "j");
    }
    (dir, model)
}

type Ran = errs::Result<(Outcome, Clock)>;

/// Runs `command` through the operations over `tasks` of `model`.
fn named(model: &Model, command: Command, tasks: &[Chosen]) -> Ran {
    let targets: Vec<Target> = tasks.iter().map(Chosen::target).collect();
    let claim = ClaimOpts::default();
    let wontdo = DoOpts {
        wontdo: true,
        ..DoOpts::default()
    };
    model.attempt(|env| match command {
        Command::Do => ops::finish(env, &targets, DoOpts::default()),
        Command::Wontdo => ops::finish(env, &targets, wontdo),
        Command::Start => ops::start(env, &targets, StartOpts::default()),
        Command::Stop => ops::stop(env, &targets, claim),
        Command::Reopen
            if matches!(targets[0].line(), Some((Source::Done, _))) =>
        {
            ops::restore(env, &targets, claim)
        }
        Command::Reopen => ops::reopen(env, &targets, claim),
        Command::Delete => ops::del(env, &targets, claim),
        Command::Step { forward } => {
            let last = model.deps.config.priority_range;
            let pairs: Vec<PriPair> = model
                .rows_of(targets.len())
                .iter()
                .filter_map(|row| model.snap.task(row.id))
                .zip(&targets)
                .map(|(task, target)| PriPair {
                    target: target.clone(),
                    priority: stepped(
                        model.snap.line(task).effective_priority(),
                        last,
                        forward,
                    ),
                })
                .collect();
            ops::pri(env, &pairs)
        }
    })
}

/// A press in the way the test does it, over `n` tasks.
#[derive(Clone, Copy, Debug)]
enum Way {
    Selection,
    Count,
}

fn begin(way: Way, kinds: &[usize]) -> (TempDir, Model, usize) {
    let (dir, mut model) = list(kinds);
    let n = kinds.len();
    let count = match way {
        Way::Selection => {
            press(&mut model, "v");
            for _ in 1..n {
                press(&mut model, "j");
            }
            1
        }
        Way::Count => n,
    };
    assert_eq!(model.rows_of(count).len(), n, "{kinds:?}");
    (dir, model, count)
}

/// Types the count of `way`, as a press does before its key.
fn type_count(model: &mut Model, way: Way, n: usize) {
    if matches!(way, Way::Count) && n > 1 {
        press(model, &n.to_string());
    }
}

/// Sends a write with its keys in NORMAL and VISUAL mode, the way `way` says,
/// after `type_count`.
fn send(
    model: &mut Model,
    (action, normal, visual): (Action, &str, &str),
    way: Way,
    n: usize,
) {
    if normal.is_empty() {
        model.act(action, if matches!(way, Way::Count) { n } else { 1 });
        return;
    }
    match way {
        Way::Selection => press(model, visual),
        Way::Count => press(model, normal),
    };
}

/// A press that has been sent, with the files before and after it.
struct Pressed<'a> {
    name: String,
    way: Way,
    kinds: &'a [usize],
    dir: TempDir,
    model: Model,
    before: [Vec<u8>; 2],
    after: [Vec<u8>; 2],
}

impl Pressed<'_> {
    /// `command` on a copy with the same clock, and the files it left.
    fn on_a_copy(&self, command: Command) -> (Ran, [Vec<u8>; 2]) {
        let (dir, twin, count) = begin(self.way, self.kinds);
        let tasks = twin.chosen(&twin.tasks_of(count));
        (named(&twin, command, &tasks), files(dir.path()))
    }

    fn refused(&self) {
        let name = &self.name;
        assert_eq!(self.after, self.before, "{name}: refused, yet it wrote");
        assert_eq!(self.model.tone, Tone::Alert, "{name}");
        assert_eq!(self.model.hist.undo.len(), 0, "{name}");
        assert_eq!(self.model.again, None, "{name}: took the repeat");
        let mode = match self.way {
            Way::Selection => Mode::Visual,
            Way::Count => Mode::Normal,
        };
        assert_eq!(self.model.key_mode(), mode, "{name}");
    }

    fn refused_by(&self, command: Command) {
        let name = &self.name;
        self.refused();
        let (Err(e), expected) = self.on_a_copy(command) else {
            panic!("{name}: {command:?} did not refuse");
        };
        assert_eq!(self.model.message.as_deref(), Some(e.message()), "{name}");
        assert_eq!(expected, self.before, "{name}: the command wrote");
    }

    /// The kinds whose line the press left as it was, by the files.
    fn passed_over(&self) -> usize {
        let left = self.kinds.iter().filter(|k| {
            let file = self.after.get(usize::from(**k >= IN_DONE_FILE));
            file.is_some_and(|file| {
                let line = KINDS.get(**k).map_or(&b""[..], |l| l.as_bytes());
                file.split(|b| *b == b'\n').any(|l| l == line)
            })
        });
        left.count()
    }

    fn resolved(&mut self, command: Command) {
        let name = format!("{}: {command:?}", self.name);
        let (result, expected) = self.on_a_copy(command);
        assert!(result.is_ok(), "{name} refused: {result:?}");
        assert_eq!(self.after, expected, "{name}");
        assert_ne!(self.after, self.before, "{name}: it changed nothing");
        self.written(command, self.passed_over());
    }

    /// What a write said of the tasks it passed over, and `u` takes it back.
    fn written(&mut self, command: Command, there: usize) {
        if let Some((_, already)) = command.words().filter(|_| there > 0) {
            let tail = format!("; {there} {already}");
            let said = self.model.message.as_deref().unwrap_or_default();
            assert!(said.ends_with(&tail), "{}: {said}", self.name);
            assert!(!said.contains(" is "), "{}: {said}", self.name);
        }
        press(&mut self.model, "u");
        assert_eq!(files(self.dir.path()), self.before, "{}", self.name);
    }
}

/// Checks one press against what the rule said of it.
fn check(key: (Action, &str, &str), way: Way, kinds: &[usize]) {
    let name = format!("{} {way:?} {kinds:?}", key.1);
    let (dir, mut model, count) = begin(way, kinds);
    let answer = if key.1.is_empty() {
        decide(key.0, &model, &model.summary(count))
    } else {
        type_count(&mut model, way, kinds.len());
        model.answers()(key.0)
    };
    let before = files(dir.path());
    send(&mut model, key, way, kinds.len());
    let after = files(dir.path());
    assert_eq!(dim(answer), after == before, "{name}: {answer:?}");
    let mut pressed = Pressed {
        name,
        way,
        kinds,
        dir,
        model,
        before,
        after,
    };
    match answer {
        Answer::Applies => panic!("{}: a key must decide", pressed.name),
        Answer::Refused(_) | Answer::CoreRefuses(Command::Stop) => {
            pressed.refused();
        }
        Answer::CoreRefuses(command) => pressed.refused_by(command),
        Answer::Resolved(command) => pressed.resolved(command),
    }
}

/// Checks that a prompt opens unless the rule refuses the tasks.
fn check_prompt((action, key): (Action, &str), way: Way, kinds: &[usize]) {
    let name = format!("{key} {way:?} {kinds:?}");
    let (dir, mut model, _count) = begin(way, kinds);
    type_count(&mut model, way, kinds.len());
    let answer = model.answers()(action);
    let before = files(dir.path());
    press(&mut model, key);
    let opened = model.line.is_some();
    assert_eq!(!opened, matches!(answer, Answer::Refused(_)), "{name}");
    assert_eq!(files(dir.path()), before, "{name}");
}

/// Each kind alone, each pair and each triple.
fn every_choice() -> Vec<Vec<usize>> {
    let n = KINDS.len();
    let mut out = Vec::new();
    for a in 0..n {
        out.push(vec![a]);
        for b in a + 1..n {
            out.push(vec![a, b]);
            for c in b + 1..n {
                out.push(vec![a, b, c]);
            }
        }
    }
    out
}

/// One key over every choice as a selection, and, since a count only finds
/// its rows another way, over the pairs as a count.
fn every_press(key: (Action, &str, &str)) {
    for kinds in every_choice() {
        check(key, Way::Selection, &kinds);
        if kinds.len() < 3 {
            check(key, Way::Count, &kinds);
        }
    }
}

#[test]
fn x_does_what_the_rule_says() {
    every_press((Action::ToggleComplete, "x", "x"));
}

#[test]
fn t_does_what_the_rule_says() {
    every_press((Action::ToggleStart, "t", "t"));
}

#[test]
fn wontdo_does_what_the_rule_says() {
    every_press((Action::Wontdo, "T", "T"));
}

#[test]
fn delete_does_what_the_rule_says() {
    every_press((Action::Delete, "dd", "d"));
}

#[test]
fn a_priority_step_does_what_the_rule_says() {
    every_press((Action::CyclePriority, "-", "-"));
    every_press((Action::CyclePriorityBack, "+", "+"));
}

/// `stop` has no key by default and is asked of `act`.
#[test]
fn stop_does_what_the_rule_says() {
    every_press((Action::Stop, "", ""));
}

#[test]
fn a_prompt_opens_as_the_rule_says() {
    for kinds in every_choice() {
        for key in [
            (Action::SetDue, "gd"),
            (Action::AddTag, "gt"),
            (Action::RemoveTag, "gT"),
        ] {
            check_prompt(key, Way::Selection, &kinds);
            if kinds.len() < 3 {
                check_prompt(key, Way::Count, &kinds);
            }
        }
    }
}

/// A model of `todo` and `done` with the detail panel on, whose clipboard is
/// `board`, and how many times its opener ran. `<C-x>` discards a draft, which
/// no default key does.
fn watched(
    (todo, done): (&str, &str),
    board: &Board,
) -> (TempDir, Model, Rc<Cell<usize>>) {
    let dir = workdir(todo);
    fs::write(dir.path().join("done.txt"), done)
        .unwrap_or_else(|e| panic!("{e}"));
    fs::write(
        dir.path().join("keys.toml"),
        "[insert]\ncancel = [\"<C-x>\"]\n",
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let opened = Rc::new(Cell::new(0));
    let runs = Rc::clone(&opened);
    let deps = Deps {
        clipboard: Box::new(board.clone()),
        opener: Box::new(move |_| {
            runs.set(runs.get() + 1);
            Ok(())
        }),
        ..deps(
            dir.path(),
            Config {
                show_detail: true,
                show_sidebar: true,
                ..config()
            },
        )
    };
    (dir, started(deps), opened)
}

/// All a key that is lit can do: write, fill the clip, open a field or run
/// the opener.
type Effect = ([Vec<u8>; 2], String, bool, usize);

fn effect(dir: &Path, model: &Model, opened: &Cell<usize>) -> Effect {
    let field = model.edit.is_some() || model.line.is_some();
    let clip = format!("{:?}", model.clip);
    (files(dir), clip, field, opened.get())
}

const TODO: &str = "A\n";
const TWO: &str = "A\nB\n";
const THREE: &str = "A\nB\nC\n";
const FOUR: &str = "A\nB\nC\nD\n";
const TWINS: &str = "A\nA\n";
const MIXED: &str = "A\nB\nx 2026-09-01 D\nC\n";
const OLD: &str = "x 2026-09-01 Old\n";
const OLD_ID: &str = "x 2026-09-01 Old id:o\n";
const DONE: &str = "x 2026-09-01 Done\n";
const LINK: &str = "Read https://x.org/a\n";
const LINKS: &str = "Read https://x.org/a https://x.org/b\n";
const OLD_LINK: &str = "x 2026-09-01 Read https://x.org/a\n";
const STUCK: &str = "Plain\nBlocker id:b\nStuck dep:b\n";
const LATER: &str = "A t:2099-01-01\n";
const CLAIMED: &str =
    "Blocker id:b\nStuck dep:b started:2026-09-29T0800Z owner:agent2\n";
const CLAIMED_TWICE: &str = "Blocker id:b\n\
    Stuck dep:b started:2026-09-29T0800Z owner:agent2\n\
    Too dep:b started:2026-09-29T0800Z\n";
/// Two claims a sort shows the other way round from the file.
const CLAIMED_RANKED: &str = "Blocker id:b\n\
    (B) Stuck dep:b started:2026-09-29T0800Z owner:agent2\n\
    (A) Too dep:b started:2026-09-29T0800Z\n";
/// Hides what `done` holds and any done task of `todo`.
const HIDE_DONE: &str = ":set hidden_tasks=done<CR>";

/// A state of a key: its files, what is typed first, whether its hint is dim.
type Case = (&'static str, &'static str, &'static str, bool);

/// A key and its action, with its states.
type KeyCases = (Action, &'static str, &'static [Case]);

/// Each key, in NORMAL or VISUAL as the first keys leave it and again in the
/// detail panel where it has that key: the hint is dim and the key has no
/// effect, or lit and it has one.
#[test]
#[expect(clippy::too_many_lines, reason = "a table")]
fn the_keys_that_write_no_task_do_what_their_hint_says() {
    use Action::{
        ChangeDescription as Cc, EditAppend, EditDescription, EditLineStart,
        InsertDependent as Go, InsertPrerequisite as Prerequisite,
        MoveTaskDown as Down, MoveTaskUp as Up, OpenLink, Redo, Repeat, Undo,
        Yank,
    };
    const NEEDS_A_TASK: &[Case] = &[
        ("", "", "", true),
        ("", "", "2", true),
        (TODO, "", "/zzz<CR>", true),
        (TODO, "", "/zzz<CR>2", true),
        (LATER, "", "", true),
        (DONE, OLD, HIDE_DONE, true),
        (TODO, "", "dd", true),
    ];
    const OPENS_ANYWAY: &[Case] = &[
        ("", "", "", false),
        ("", "", "2", false),
        (TODO, "", "/zzz<CR>", false),
        (LATER, "", "", false),
        (DONE, OLD, HIDE_DONE, false),
    ];
    const FIELDS: &[Case] = &[
        ("", "", "", true),
        ("", "", "2", true),
        (TODO, "", "/zzz<CR>", true),
        (LATER, "", "", true),
        (DONE, OLD, HIDE_DONE, true),
        (TODO, OLD, "2", false),
        (TODO, "", "", false),
        (DONE, "", "", false),
        ("", OLD, "", true),
        (TODO, OLD, "j", true),
    ];
    const MOVES_DOWN: &[Case] = &[
        (TWO, "", "9", false),
        (TODO, "", "9", true),
        (TWO, OLD, "j9", true),
        (THREE, "", "vj9", false),
        (TWO, "", "vj9", true),
        ("", "", "", true),
        (TWO, "", "", false),
        (TODO, "", "", true),
        (TODO, OLD, "", true),
        ("", OLD, "", true),
        (TWO, OLD, "j", true),
        (TWO, "", "s", true),
        ("x 2026-09-01 A\nB\n", "", "", false),
        (THREE, "", "vj", false),
        (TWO, "", "vj", true),
        (TODO, OLD, "vj", true),
        (THREE, "", "svj", true),
        (TWINS, "", "", true),
        ("A\nA\nB\n", "", "2", false),
        ("A\nA\nA\n", "", "vj", true),
        ("A\nB\nA\n", "", "/A<CR>", false),
    ];
    const MOVES_UP: &[Case] = &[
        (TWO, "", "j9", false),
        (TWO, "", "9", true),
        (THREE, "", "jvj9", false),
        ("", "", "", true),
        (TWO, "", "", true),
        (TWO, "", "j", false),
        (TWO, OLD, "jj", true),
        (TWO, "", "sj", true),
        (THREE, "", "jvj", false),
        (THREE, "", "vj", true),
        (TODO, OLD, "vj", true),
        (TWINS, "", "j", true),
    ];
    let rows: [KeyCases; 27] = [
        (Action::ToggleComplete, "x", NEEDS_A_TASK),
        (Action::ToggleStart, "t", NEEDS_A_TASK),
        (Action::Wontdo, "T", NEEDS_A_TASK),
        (Action::Delete, "dd", NEEDS_A_TASK),
        (Action::CyclePriority, "gp", NEEDS_A_TASK),
        (Action::SetDue, "gd", NEEDS_A_TASK),
        (Action::AddTag, "gt", NEEDS_A_TASK),
        (Action::RemoveTag, "gT", NEEDS_A_TASK),
        (Action::InsertBelow, "o", OPENS_ANYWAY),
        (Action::InsertAbove, "O", OPENS_ANYWAY),
        (Go, "go", OPENS_ANYWAY),
        (Prerequisite, "gO", OPENS_ANYWAY),
        (
            Undo,
            "u",
            &[
                (TODO, "", "", true),
                (TODO, "", "x", false),
                (TODO, "", "x3", false),
                (TODO, "", "3", true),
                (TODO, "", "xu", true),
                (TODO, "", "aZ<C-x>", false),
            ],
        ),
        (
            Redo,
            "<C-r>",
            &[
                (TODO, "", "", true),
                (TODO, "", "x", true),
                (TODO, "", "xu", false),
                (TODO, "", "xu3", false),
            ],
        ),
        (
            Repeat,
            ".",
            &[
                (TODO, "", "", true),
                (TWO, "", "x", false),
                (STUCK, "", "xjj", true),
                (TWO, "", "xk", false),
                (TODO, OLD, "xj", false),
                (TODO, OLD, "Tj", true),
                (TWO, "", "gtk:v<CR>j", false),
                (TODO, OLD, "gtk:v<CR>j", true),
                (TWO, "", "gd2030-01-01<CR>j", false),
                (TODO, OLD, "gd2030-01-01<CR>j", true),
                (THREE, "", "J", false),
                (TWO, "", "J", true),
                ("A\nB\nA\nB\n", "", "J", true),
                ("A\nB\nA\nB\n", "", "J2", false),
                ("A\nA\nB\nA\nA\nA\n", "", "vjJ<Esc>j", true),
                (TODO, "", "yyp", false),
                (MIXED, "", "2TGk", false),
                (MIXED, "", "vjTGk", false),
                (TWO, OLD, "2gtk:v<CR>j", true),
                (TWO, OLD, "vjgtk:v<CR>j", true),
                (FOUR, "", "vjJ<Esc>gg", false),
                (THREE, "", "vjJ<Esc>Gk", true),
                (FOUR, "", "3Jgg", false),
                (THREE, "", "5Jgg", false),
                (THREE, OLD, "3Jgg", false),
                (TWO, OLD, "3J", true),
                (TODO, "", "2", true),
                (MIXED, "", "TGk2", false),
                (MIXED, "", "2TGk1", true),
                (THREE, "", "vjJ<Esc>Gk1", false),
                (FOUR, "", "vjJ<Esc>gg9", true),
                (TWO, "", "Jgg2", false),
                (TWO, "", "J2", true),
                (TWO, OLD, "gtk:v<CR>j2", true),
                (TWO, OLD, "gtk:v<CR>j1", false),
                (TODO, "", "yyp2", false),
                (TODO, "", "dd", true),
                (TODO, "", "dd2", true),
                (TODO, "", "x/zzz<CR>", true),
                (TODO, "", "x/zzz<CR>2", true),
                (TODO, "", "t/zzz<CR>", true),
                (TODO, "", "T/zzz<CR>", true),
                (TODO, "", "gp/zzz<CR>", true),
                (TODO, "", "gtk:v<CR>/zzz<CR>", true),
                (TODO, "", "gd2030-01-01<CR>/zzz<CR>", true),
                (TODO, "", "aZ<CR>/zzz<CR>", true),
                (TODO, "", "yyp/zzz<CR>", false),
            ],
        ),
        (Down, "J", MOVES_DOWN),
        (Up, "K", MOVES_UP),
        (EditDescription, "i", FIELDS),
        (EditAppend, "a", FIELDS),
        (EditLineStart, "I", FIELDS),
        (Cc, "cc", FIELDS),
        (
            Prerequisite,
            "gO",
            &[
                (TODO, "", "", false),
                (DONE, "", "", false),
                ("", OLD, "", true),
                (TODO, OLD, "j", true),
                (TODO, OLD, "2", false),
            ],
        ),
        (
            Go,
            "go",
            &[
                (TODO, "", "", false),
                ("", OLD_ID, "", false),
                (TODO, OLD, "2", false),
                ("", OLD, "", true),
                (TODO, OLD, "j", true),
            ],
        ),
        (
            OpenLink,
            "gx",
            &[
                ("", "", "", true),
                (TODO, "", "", true),
                (LINK, "", "", false),
                (LINK, "", "1", false),
                (LINK, "", "2", true),
                (LINKS, "", "2", false),
                (LINKS, "", "3", true),
                (LINK, "", "vj2", true),
                (LINK, "", "<C-w>23", false),
                (TODO, "", "<C-w>23", true),
                ("", OLD_LINK, "", false),
                ("", OLD, "", true),
            ],
        ),
        (
            Yank,
            "yy",
            &[
                ("", "", "", true),
                ("", "", "2", true),
                (TODO, "", "", false),
                (TODO, OLD, "2", false),
                ("", OLD, "", false),
            ],
        ),
        (
            Yank,
            "y",
            &[(TODO, "", "v", false), (TWO, OLD, "vjj", false)],
        ),
        (
            OpenLink,
            "<CR>",
            &[
                (LINK, "", "<C-w>2", false),
                (LINK, "", "<C-w>23", false),
                (TODO, "", "<C-w>2", true),
                ("", "", "<C-w>2", true),
            ],
        ),
    ];
    let mut detail = 0;
    for (action, key, cases) in rows {
        for case in cases {
            agrees(action, key, case, false);
            detail += usize::from(agrees(action, key, case, true));
        }
    }
    assert!(detail > 40, "{detail}");
}

/// Moves to the detail panel, and tells whether `key` could be asked there:
/// it opens from NORMAL only, on a task, and has not every key.
fn into_detail(model: &mut Model, action: Action, key: &str) -> bool {
    let bound = model.deps.keys.keys(Mode::Detail, action);
    let counted = model.typed_count() > 0;
    if counted || model.mode() != Mode::Normal || !bound.contains(&key) {
        return false;
    }
    keys(model, "<C-w>2");
    model.key_mode() == Mode::Detail
}

/// One key in one state against its hint, and whether it could be asked.
fn agrees(action: Action, key: &str, case: &Case, in_detail: bool) -> bool {
    let &(todo, done, first, want) = case;
    let name = format!("{key} on {todo:?} {done:?} after {first} {in_detail}");
    let board = Board::default();
    let (dir, mut model, opened) = watched((todo, done), &board);
    keys(&mut model, first);
    if in_detail && !into_detail(&mut model, action, key) {
        return false;
    }
    let answer = model.answers()(action);
    assert_eq!(dim(answer), want, "{name}: {answer:?}");
    let before = effect(dir.path(), &model, &opened);
    keys(&mut model, key);
    let after = effect(dir.path(), &model, &opened);
    assert_eq!(after == before, want, "{name}: {answer:?}");
    true
}

/// `p` and `P` are always lit: only the clipboard, which a hint does not read,
/// knows whether they paste. A press pastes or says there is nothing to.
#[test]
fn a_paste_is_always_lit() {
    type Paste =
        (&'static str, &'static str, &'static str, &'static str, bool);
    const BLANK: &str = "  ";
    const FOREIGN: &str = "Foreign";
    let rows: [Paste; 16] = [
        ("", TODO, "", "", false),
        ("", TODO, "", "2", false),
        ("", "", "", "", false),
        (BLANK, TODO, "", "", false),
        (FOREIGN, TODO, "", "", true),
        (FOREIGN, TODO, "", "2", true),
        (FOREIGN, "", "", "", true),
        (FOREIGN, TODO, "", "/zzz<CR>", true),
        (FOREIGN, LATER, "", "", true),
        (FOREIGN, DONE, OLD, HIDE_DONE, true),
        ("", TODO, "", "/zzz<CR>", false),
        (BLANK, TODO, "", "yy", false),
        ("", TODO, "", "yy", true),
        ("", TODO, OLD, "yyj", true),
        ("", TODO, "", "yydd", true),
        ("", "", OLD, "yy", true),
    ];
    for (action, key) in [(Action::PasteBelow, "p"), (Action::PasteAbove, "P")]
    {
        for row in rows {
            pastes_or_says(action, key, row, false);
            pastes_or_says(action, key, row, true);
        }
    }
}

/// One press of `p` or `P`. A yank leaves its text on the clipboard, so the
/// text is set after the first keys unless it is empty.
fn pastes_or_says(
    action: Action,
    key: &str,
    (held, todo, done, first, pastes): (&str, &str, &str, &str, bool),
    in_detail: bool,
) {
    let name =
        format!("{key} on {held:?} {todo:?} {done:?} {first} {in_detail}");
    let board = Board::default();
    let (dir, mut model, _opened) = watched((todo, done), &board);
    keys(&mut model, first);
    if !held.is_empty() {
        board.set(held);
    }
    if in_detail && !into_detail(&mut model, action, key) {
        return;
    }
    assert!(!dim(model.answers()(action)), "{name}");
    let before = files(dir.path());
    keys(&mut model, key);
    assert_eq!(files(dir.path()) != before, pastes, "{name}");
    if !pastes {
        let said = model.message.as_deref();
        assert_eq!(said, Some("nothing to paste"), "{name}");
    }
}

/// A `.` of `p` and `P` is as lit as they are, with a yank and without.
#[test]
fn a_repeat_of_a_paste_is_always_lit() {
    for key in ["yyp", "yyP", "p", "P"] {
        let board = Board::default();
        board.set("Foreign");
        let (dir, mut model, _opened) = watched((TODO, ""), &board);
        keys(&mut model, key);
        assert!(!dim(model.answers()(Action::Repeat)), "{key}");
        board.set("Foreign");
        let before = files(dir.path());
        keys(&mut model, ".");
        assert_ne!(files(dir.path()), before, "foreign text is pasted");

        board.set("  ");
        assert!(!dim(model.answers()(Action::Repeat)), "{key}");
        let before = files(dir.path());
        keys(&mut model, ".");
        assert_eq!(files(dir.path()), before, "blank text is not");
        assert_eq!(model.message.as_deref(), Some("nothing to paste"));
    }
}

/// What each key says when the rule refuses it, and in which style. Now that
/// they ask the rule, a refusal must still reach the status line as before.
#[test]
fn a_refused_key_says_what_it_always_said() {
    const ONE: &str = "Old is archived — x reopens it";
    const NO_ID: &str = "Old is archived and has no id — x reopens it first";
    const SOME: &str = "some are archived — nothing changed";
    const THREE: &str = "A\nB\nC\n";
    const SORTED: &str = "sorted by priority — press s for manual order";
    let rows = [
        ("", OLD, "", "a", ONE, true),
        ("", OLD, "", "i", ONE, true),
        ("", OLD, "", "I", ONE, true),
        ("", OLD, "", "cc", ONE, true),
        ("", OLD, "", "gO", ONE, true),
        ("", OLD, "", "J", ONE, true),
        ("", OLD, "", "K", ONE, true),
        ("", OLD, "v", "J", ONE, true),
        ("", OLD, "", "go", NO_ID, true),
        (TODO, OLD, "vj", "J", SOME, true),
        (TODO, OLD, "vj", "K", SOME, true),
        (TODO, OLD, "vj", "d", SOME, true),
        (TODO, OLD, "vj", "gd", SOME, true),
        (TODO, OLD, "", "2dd", SOME, true),
        (TODO, OLD, "-", "2.", SOME, true),
        (TODO, OLD, "gtk:v<CR>", "2.", SOME, true),
        (THREE, OLD, "vjJ<Esc>gg", "4.", SOME, true),
        (TWO, "", "s", "J", SORTED, false),
        ("", OLD, "s", "K", SORTED, false),
        (TODO, "", "", "u", "nothing to undo", false),
        (TODO, "", "", "<C-r>", "nothing to redo", false),
        (TODO, "", "", "p", "nothing to paste", false),
        (TODO, "", "", "gx", "no link on this task", false),
        (TODO, OLD, "gtk:v<CR>j", ".", ONE, true),
    ];
    for (todo, done, first, key, said, alert) in rows {
        let name = format!("{key} on {todo:?} {done:?} after {first}");
        let (dir, mut model, _opened) =
            watched((todo, done), &Board::default());
        keys(&mut model, first);
        let before = files(dir.path());
        keys(&mut model, key);
        assert_eq!(model.message.as_deref(), Some(said), "{name}");
        assert_eq!(model.tone == Tone::Alert, alert, "{name}");
        assert_eq!(files(dir.path()), before, "{name}");
    }
}

/// A key with nothing to work on says nothing at all.
#[test]
fn a_key_with_nothing_to_work_on_says_nothing() {
    for key in [
        "yy", "J", "K", "gx", ".", "x", "t", "T", "dd", "a", "i", "I", "cc",
        "gp", "gP", "gd", "gt", "gT",
    ] {
        let (_dir, mut model, _opened) = watched(("", ""), &Board::default());
        keys(&mut model, key);
        assert_eq!(model.message, None, "{key}");
    }
    for key in ["J", "K"] {
        let (_dir, mut model, _opened) =
            watched((TODO, ""), &Board::default());
        keys(&mut model, key);
        assert_eq!(model.message, None, "{key}");
    }
}

type Rule = (Action, &'static [usize], Answer);

/// The rule, as the kinds of `KINDS` name the tasks.
#[test]
fn the_rule() {
    use Action::{
        CyclePriority as Step, Delete, SetDue, Stop, ToggleComplete as X,
        ToggleStart as Toggle, Wontdo, Yank,
    };
    const STEP: Command = Command::Step { forward: true };
    let rows: [Rule; 40] = [
        (X, &[1], Answer::Resolved(Command::Do)),
        (X, &[1, 7], Answer::Resolved(Command::Do)),
        (X, &[1, 3], Answer::Resolved(Command::Do)),
        (X, &[7, 8], Answer::Resolved(Command::Reopen)),
        (X, &[9, 10], Answer::Resolved(Command::Reopen)),
        (X, &[1, 9], Answer::Resolved(Command::Do)),
        (X, &[3, 9], Answer::Resolved(Command::Do)),
        (X, &[7, 9], Answer::Refused(Refusal::Closed)),
        (X, &[2], Answer::CoreRefuses(Command::Do)),
        (X, &[1, 2, 7], Answer::CoreRefuses(Command::Do)),
        (Toggle, &[1], Answer::Resolved(Command::Start)),
        (Toggle, &[1, 3], Answer::Resolved(Command::Start)),
        (Toggle, &[1, 4], Answer::Resolved(Command::Start)),
        (Toggle, &[3], Answer::Resolved(Command::Stop)),
        (Toggle, &[3, 4], Answer::Resolved(Command::Stop)),
        (Toggle, &[3, 7], Answer::Resolved(Command::Stop)),
        (Toggle, &[7, 8], Answer::Refused(Refusal::AllDone)),
        (Toggle, &[1, 7], Answer::Refused(Refusal::SomeDone)),
        (Toggle, &[1, 2], Answer::CoreRefuses(Command::Start)),
        (Toggle, &[9], Answer::Refused(Refusal::Closed)),
        (Toggle, &[1, 9], Answer::Refused(Refusal::Archived)),
        (Toggle, &[7, 9], Answer::Refused(Refusal::Closed)),
        (Wontdo, &[1], Answer::Resolved(Command::Wontdo)),
        (Wontdo, &[2], Answer::Resolved(Command::Wontdo)),
        (Wontdo, &[1, 7], Answer::Resolved(Command::Wontdo)),
        (Wontdo, &[8], Answer::Refused(Refusal::AllDone)),
        (Wontdo, &[10], Answer::Refused(Refusal::Closed)),
        (Wontdo, &[8, 10], Answer::Refused(Refusal::Closed)),
        (Wontdo, &[1, 10], Answer::Resolved(Command::Wontdo)),
        (Stop, &[1], Answer::CoreRefuses(Command::Stop)),
        (Stop, &[3, 7], Answer::Resolved(Command::Stop)),
        (Delete, &[1, 7], Answer::Resolved(Command::Delete)),
        (Delete, &[1, 9], Answer::Refused(Refusal::Archived)),
        (Step, &[1, 3], Answer::Resolved(STEP)),
        (Step, &[7], Answer::CoreRefuses(STEP)),
        (Step, &[9], Answer::Refused(Refusal::Closed)),
        (SetDue, &[1, 7], Answer::Applies),
        (SetDue, &[9], Answer::Refused(Refusal::Closed)),
        (SetDue, &[1, 9], Answer::Refused(Refusal::Archived)),
        (Yank, &[9], Answer::Applies),
    ];
    for (action, kinds, want) in rows {
        let (_dir, model, count) = begin(Way::Count, kinds);
        let got = decide(action, &model, &model.summary(count));
        assert_eq!(got, want, "{action:?} {kinds:?}");
    }
}

/// A kind, a command, how it refuses and how it starts what it says.
type Fact = (usize, Command, Option<Code>, &'static str);

/// What the rule relies on, each as the command does it.
#[test]
fn what_the_commands_do() {
    const OK: Option<Code> = None;
    let rows: [Fact; 12] = [
        (7, Command::Do, OK, "2 is already done"),
        (2, Command::Do, Some(Code::Blocked), ""),
        (8, Command::Wontdo, OK, "2 is already won't do"),
        (
            7,
            Command::Wontdo,
            OK,
            "2 is already done; use lsty reopen 2",
        ),
        (2, Command::Wontdo, OK, "2 won't do: Waits"),
        (3, Command::Start, OK, "2 is already underway"),
        (7, Command::Start, Some(Code::WrongState), ""),
        (2, Command::Start, Some(Code::Blocked), ""),
        (1, Command::Stop, OK, "2 is not underway"),
        (7, Command::Stop, OK, "2 is not underway"),
        (1, Command::Reopen, OK, "2 is already open"),
        (
            7,
            Command::Step { forward: true },
            Some(Code::WrongState),
            "",
        ),
    ];
    for (kind, command, code, says) in rows {
        let (_dir, model) = list(&[kind]);
        let tasks = model.chosen(&model.tasks_of(1));
        match named(&model, command, &tasks) {
            Ok((outcome, _)) => {
                assert_eq!(code, None, "{kind} {command:?}");
                let said = errs::lossy(
                    outcome.messages().first().map_or(&[][..], |m| m),
                );
                assert!(said.starts_with(says), "{command:?}: {said}");
            }
            Err(e) => assert_eq!(Some(e.code()), code, "{kind} {command:?}"),
        }
    }
}

/// One task the command refuses keeps it from writing any.
#[test]
fn a_command_refused_on_one_task_writes_none() {
    let (dir, model) = list(&[1, 2]);
    let before = files(dir.path());
    let tasks = model.chosen(&model.tasks_of(2));
    assert!(named(&model, Command::Do, &tasks).is_err());
    assert_eq!(files(dir.path()), before);
}

/// A blocker in the same press does not free the task waiting for it: the
/// commands read the list as it was.
#[test]
fn a_blocker_in_the_press_still_blocks() {
    let (dir, mut model) = list(&[0, 2]);
    let before = files(dir.path());
    press(&mut model, "2x");
    assert_eq!(model.tone, Tone::Alert);
    assert_eq!(files(dir.path()), before);
}

/// A completion that passes a task over and fails keeps the cursor.
#[test]
fn a_failed_completion_keeps_the_cursor() {
    let (dir, mut model) = list(&[1, 7]);
    fs::write(dir.path().join("todo.txt"), "Changed\n")
        .unwrap_or_else(|e| panic!("{e}"));
    let cursor = model.cursor;
    press(&mut model, "2x");
    assert_eq!(model.tone, Tone::Alert);
    assert_eq!(model.cursor, cursor);
}

/// `t` over a task that an agent holds takes it over, and says so.
#[test]
fn t_takes_over_an_agents_claim_and_says_so() {
    let (dir, mut model) = list(&[1, 2 + 2]);
    press(&mut model, "2t");
    let todo = fs::read_to_string(dir.path().join("todo.txt"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(!todo.contains("owner:agent2"), "{todo}");
    assert_eq!(
        model.message.as_deref(),
        Some("2 started; 3 was claimed by agent2; now yours")
    );
}

/// `t` over blocked tasks in the 2 seconds after it stopped exactly them: the
/// hint is lit and the key puts the claims back, and from when the window
/// ends, or for any other tasks, dim and it writes nothing. A digit keeps the
/// window open.
#[test]
fn a_t_is_lit_while_it_can_put_a_claim_back() {
    const OPEN: u64 = 1_999;
    const SHUT: u64 = 2_000;
    let rows = [
        (CLAIMED, "jt", 0, true),
        (CLAIMED, "jt", OPEN, true),
        (CLAIMED, "jt", SHUT, false),
        (CLAIMED, "jt1", OPEN, true),
        (CLAIMED, "jt1", SHUT, false),
        (CLAIMED, "jt2", OPEN, true),
        (CLAIMED, "jt2", SHUT, false),
        (CLAIMED, "jtkj", 0, false),
        (CLAIMED, "jt2<Esc>", 0, false),
        (CLAIMED_TWICE, "jt2", OPEN, false),
        (CLAIMED_TWICE, "j2t", OPEN, false),
        (CLAIMED_TWICE, "j2t2", OPEN, true),
        (CLAIMED_TWICE, "j2t2", SHUT, false),
        (CLAIMED_RANKED, "sgg2t2", OPEN, true),
    ];
    for (todo, first, after, back) in rows {
        let name = format!("t on {todo:?} after {first}, {after}ms");
        let (dir, mut model, _opened) = watched((todo, ""), &Board::default());
        keys(&mut model, first);
        let then = instant(NOW)
            .checked_add(Duration::from_millis(after))
            .unwrap_or_else(|e| panic!("{e}"));
        model.deps.now = Box::new(move || then);
        let answer = model.answers()(Action::ToggleStart);
        assert_eq!(dim(answer), !back, "{name}: {answer:?}");
        let before = files(dir.path());
        keys(&mut model, "t");
        assert_eq!(files(dir.path()) != before, back, "{name}");
        let said = model.message.as_deref();
        if back {
            assert_eq!(said, Some("put back as it was"), "{name}");
        } else {
            assert!(said.is_some_and(|s| s.contains("blocked")), "{name}");
        }
    }
}

/// `.` of `t`: the press itself ends the window, so the hint read before it
/// is lit where the key then says why not. Lit is the side a hint may err on.
#[test]
fn a_repeat_of_t_in_the_window_is_lit_and_says_why_not() {
    let (dir, mut model, _opened) = watched((CLAIMED, ""), &Board::default());
    keys(&mut model, "jt");
    assert!(!dim(model.answers()(Action::Repeat)));
    let before = files(dir.path());
    keys(&mut model, ".");
    assert_eq!(files(dir.path()), before);
    let said = model.message.as_deref().unwrap_or_default();
    assert!(said.contains("blocked"), "{said}");
    assert!(dim(model.answers()(Action::Repeat)));
}

/// A doing task whose blocker came later: `x` is refused, the others are not.
#[test]
fn a_doing_task_that_is_blocked() {
    const STUCK: &str = "Stuck dep:blk started:2026-09-29T0800Z";
    let rows: [(&str, &str, bool); 4] = [
        ("jx", "task 2 is blocked by 1", false),
        ("jT", "2 won't do: Stuck", true),
        ("jt", "2 stopped", true),
        ("j2t", "1 started; 1 already underway", true),
    ];
    let dir = workdir(format!("Blocker id:blk\n{STUCK}\n"));
    let mut model = started(deps(dir.path(), config()));
    press(&mut model, "j");
    assert_eq!(
        decide(Action::ToggleComplete, &model, &model.summary(1)),
        Answer::CoreRefuses(Command::Do)
    );
    for (typed, says, writes) in rows {
        let dir = workdir(format!("Blocker id:blk\n{STUCK}\nPlain\n"));
        let before = files(dir.path());
        let mut model = started(deps(dir.path(), config()));
        press(&mut model, typed);
        let said = model.message.clone().unwrap_or_default();
        assert!(said.starts_with(says), "{typed}: {said}");
        assert_eq!(files(dir.path()) != before, writes, "{typed}");
    }
}
