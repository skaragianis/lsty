//! The rule of the keys against the commands it names, on real lists.
//! Prompts open rather than write, `+` and `-` step each task from its own
//! priority, and the claim memory of `t` has frames of its own.

use std::{fs, path::Path};

use lsty_core::{
    config::{Config, Hidden},
    dates::Clock,
    errs::{self, Code},
    keys::Action,
    ops::{self, ClaimOpts, DoOpts, Outcome, PriPair, StartOpts, Target},
};
use tempfile::TempDir;

use crate::{
    actions::{Chosen, stepped},
    messages::Tone,
    model::Model,
    press::{Answer, Command, Refusal, decide},
    test_util::{alone, deps, press, started, workdir},
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

fn files(dir: &Path) -> [Vec<u8>; 2] {
    ["todo.txt", "done.txt"]
        .map(|name| fs::read(dir.join(name)).unwrap_or_default())
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
        Command::Reopen => ops::reopen(env, &targets, claim),
        Command::Restore => ops::restore(env, &targets, claim),
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

/// Sends a write with its keys in NORMAL and VISUAL mode, the way `way` says.
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
        Way::Count if n == 1 => press(model, normal),
        Way::Count => press(model, &format!("{n}{normal}")),
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
    let answer = decide(key.0, &model.summary(count));
    let before = files(dir.path());
    send(&mut model, key, way, kinds.len());
    let after = files(dir.path());
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
        Answer::Refused(_) => pressed.refused(),
        Answer::CoreRefuses(Command::Stop) => {
            assert_eq!(pressed.after, pressed.before, "{}", pressed.name);
        }
        Answer::CoreRefuses(command) => pressed.refused_by(command),
        Answer::Resolved(command) => pressed.resolved(command),
    }
}

/// Checks that a prompt opens unless the rule refuses the tasks.
fn check_prompt(way: Way, kinds: &[usize]) {
    let name = format!("gd {way:?} {kinds:?}");
    let (dir, mut model, count) = begin(way, kinds);
    let answer = decide(Action::SetDue, &model.summary(count));
    let before = files(dir.path());
    match way {
        Way::Selection => press(&mut model, "gd"),
        Way::Count => press(&mut model, &format!("{}gd", kinds.len())),
    };
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
        check_prompt(Way::Selection, &kinds);
        if kinds.len() < 3 {
            check_prompt(Way::Count, &kinds);
        }
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
        (X, &[9, 10], Answer::Resolved(Command::Restore)),
        (X, &[1, 9], Answer::Refused(Refusal::Archived)),
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
        (Wontdo, &[8], Answer::Resolved(Command::Reopen)),
        (Wontdo, &[7], Answer::Refused(Refusal::NotWontdo)),
        (Wontdo, &[8, 7], Answer::Refused(Refusal::NotWontdo)),
        (Wontdo, &[10], Answer::Refused(Refusal::Closed)),
        (Wontdo, &[8, 10], Answer::Refused(Refusal::Closed)),
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
        let got = decide(action, &model.summary(count));
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
        decide(Action::ToggleComplete, &model.summary(1)),
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
