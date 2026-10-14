use super::{
    Actor, AddParams, Blocks, IdChoice, Outcome, Pin, Place, Seen, Settle,
    Target, Toggle, Tracked, add,
    test_util::{Sequence, folder, parts, said, snapshot, strings},
};
use crate::{
    discover::FileState,
    errs::{Code, Error, Field, Nr, Source},
    todotxt::Priority,
};

/// What a case changes in the setup.
#[derive(Clone, Copy)]
struct Setup {
    agent: &'static str,
    include_creation: Option<bool>,
    default_priority: Option<char>,
}

fn run_with(
    todo: &str,
    setup: Setup,
    params: &AddParams,
) -> (String, Result<Outcome, Error>) {
    let mut dir = folder(todo, Some("x 2026-09-01 Old\n"));
    if let Some(on) = setup.include_creation {
        dir.config.include_creation = on;
    }
    dir.config.default_priority =
        setup.default_priority.and_then(|c| Priority::new(c as u8));
    let ids = Sequence::letters();
    let mut env = dir.env();
    env.ids = &ids;
    env.actor = Actor::named(setup.agent).expect("actor");
    let result = add(&env, params);
    (dir.todo(), result)
}

fn texts(texts: &[&str]) -> AddParams {
    AddParams {
        texts: strings(texts),
        ..AddParams::default()
    }
}

fn after(target: Target, texts_: &[&str]) -> AddParams {
    AddParams {
        place: Place::After(target),
        ..texts(texts_)
    }
}

fn before(nr: i64, texts_: &[&str]) -> AddParams {
    AddParams {
        place: Place::Before(Target::todo(nr)),
        ..texts(texts_)
    }
}

fn chosen(id: &str, t: &[&str]) -> AddParams {
    AddParams {
        id: IdChoice::Chosen(id.to_owned()),
        ..texts(t)
    }
}

fn random(t: &[&str]) -> AddParams {
    AddParams {
        id: IdChoice::Random,
        ..texts(t)
    }
}

fn with_date(date: bool, t: &[&str]) -> AddParams {
    AddParams {
        date: if date { Toggle::On } else { Toggle::Off },
        ..texts(t)
    }
}

fn with_pri(pri: Option<Priority>, t: &[&str]) -> AddParams {
    AddParams { pri, ..texts(t) }
}

fn top(t: &[&str]) -> AddParams {
    AddParams {
        place: Place::Top,
        ..texts(t)
    }
}

/// Todo file, setup, params, file after, messages, renumbered.
type Row = (
    &'static str,
    Setup,
    fn() -> AddParams,
    &'static str,
    &'static [&'static str],
    bool,
);

const NONE: Setup = Setup {
    agent: "",
    include_creation: None,
    default_priority: None,
};
const AGENT: Setup = Setup {
    agent: "agent1",
    ..NONE
};
const PRI_C: Setup = Setup {
    default_priority: Some('C'),
    ..NONE
};
const NO_CREATION: Setup = Setup {
    include_creation: Some(false),
    ..NONE
};

macro_rules! row {
    (
        $todo:expr, $setup:expr, $p:expr, $want:expr, $msgs:expr, $renum:expr
    ) => {
        (
            $todo,
            $setup,
            (|| $p) as fn() -> AddParams,
            $want,
            &$msgs,
            $renum,
        )
    };
}

#[rustfmt::skip]
const ADDED: &[Row] = &[
    row!(
        "A\n",
        NONE,
        texts(&["Buy milk"]),
        "A\n2026-09-29 Buy milk\n",
        ["2 added: Buy milk"],
        false
    ), // 14.1
    row!(
        "",
        NONE,
        texts(&["(a) Call client +work"]),
        "(A) 2026-09-29 Call client +work\n",
        ["1 added: Call client"],
        false
    ), // 14.2
    row!(
        "",
        NONE,
        with_date(false, &["Buy"]),
        "Buy\n",
        ["1 added: Buy"],
        false
    ), // 14.3
    row!(
        "",
        NO_CREATION,
        with_date(true, &["Buy"]),
        "2026-09-29 Buy\n",
        ["1 added: Buy"],
        false
    ), // 14.4
    row!(
        "",
        PRI_C,
        texts(&["Buy"]),
        "(C) 2026-09-29 Buy\n",
        ["1 added: Buy"],
        false
    ), // 14.5
    row!(
        "",
        PRI_C,
        texts(&["(A) Buy"]),
        "(A) 2026-09-29 Buy\n",
        ["1 added: Buy"],
        false
    ), // 14.5
    row!(
        "",
        PRI_C,
        with_pri(Priority::new(b'B'), &["Buy"]),
        "(B) 2026-09-29 Buy\n",
        ["1 added: Buy"],
        false
    ), // 14.6
    row!(
        "",
        NONE,
        with_pri(Priority::new(b'B'), &["(B) Buy"]),
        "(B) 2026-09-29 Buy\n",
        ["1 added: Buy"],
        false
    ), // 14.7
    row!(
        "",
        NONE,
        texts(&["xylophone lesson"]),
        "2026-09-29 xylophone lesson\n",
        ["1 added: xylophone lesson"],
        false
    ), // 14.9
    row!(
        "",
        NONE,
        with_date(false, &["Dentist 2026-01-01"]),
        "Dentist 2026-01-01\n",
        ["1 added: Dentist 2026-01-01"],
        false
    ), // 14.11
    row!(
        "",
        NONE,
        with_date(false, &["2026-02-30 Dentist"]),
        "2026-02-30 Dentist\n",
        ["1 added: 2026-02-30 Dentist"],
        false
    ), // 14.11
    row!(
        "",
        NONE,
        texts(&["2026-01-01 Dentist"]),
        "2026-09-29 2026-01-01 Dentist\n",
        ["1 added: 2026-01-01 Dentist"],
        false
    ), // 14.12
    row!(
        "",
        NONE,
        texts(&["Pay rent due:fri t:+2d"]),
        "2026-09-29 Pay rent due:2026-10-02 t:2026-10-01\n",
        ["1 added: Pay rent"],
        false
    ), // 14.15
    row!(
        "",
        AGENT,
        texts(&["Buy"]),
        "2026-09-29 Buy id:aaaa\n",
        ["1 added as id:aaaa: Buy"],
        false
    ), // 14.17
    row!(
        "",
        NONE,
        random(&["Buy"]),
        "2026-09-29 Buy id:aaaa\n",
        ["1 added as id:aaaa: Buy"],
        false
    ), // 14.18
    row!(
        "",
        NONE,
        chosen("parser", &["Write parser"]),
        "2026-09-29 Write parser id:parser\n",
        ["1 added as id:parser: Write parser"],
        false
    ), // 14.19
    row!(
        "",
        NONE,
        texts(&["Write parser id:parser +spec01"]),
        "2026-09-29 Write parser id:parser +spec01\n",
        ["1 added as id:parser: Write parser"],
        false
    ), // 14.20
    row!(
        "",
        AGENT,
        texts(&["Write id:parser"]),
        "2026-09-29 Write id:parser\n",
        ["1 added as id:parser: Write"],
        false
    ), // 14.20
    row!(
        "",
        NONE,
        chosen("parser", &["Write id:parser"]),
        "2026-09-29 Write id:parser\n",
        ["1 added as id:parser: Write"],
        false
    ), // same id typed and chosen
    row!(
        "",
        NONE,
        texts(&["X started:2026-09-29T0900Z owner:agent1"]),
        "2026-09-29 X started:2026-09-29T0900Z owner:agent1\n",
        ["1 added: X"],
        false
    ), // 14.25
    row!(
        "",
        NONE,
        texts(&["B dep:zz"]),
        "2026-09-29 B dep:zz\n",
        ["1 added: B"],
        false
    ), // 14.27
    row!(
        "",
        NONE,
        texts(&["B due:fri due:soon"]),
        "2026-09-29 B due:2026-10-02 due:soon\n",
        ["1 added: B"],
        false
    ), // single-valued key twice
    row!(
        "",
        NONE,
        texts(&["B rec:1w rec:1x"]),
        "2026-09-29 B rec:1w rec:1x\n",
        ["1 added: B"],
        false
    ), // later rec as typed
    row!(
        "A\nB\n",
        NONE,
        top(&["N"]),
        "2026-09-29 N\nA\nB\n",
        ["1 added: N"],
        true
    ), // 14.28
    row!(
        "A\nB\nC\n",
        NONE,
        after(Target::todo(1), &["N"]),
        "A\n2026-09-29 N\nB\nC\n",
        ["2 added: N"],
        true
    ), // 14.29
    row!(
        "A\nB\n",
        NONE,
        after(Target::todo(2), &["N"]),
        "A\nB\n2026-09-29 N\n",
        ["3 added: N"],
        false
    ), // 14.30
    row!(
        "A\nB\n",
        NONE,
        before(1, &["N"]),
        "2026-09-29 N\nA\nB\n",
        ["1 added: N"],
        true
    ), // 14.31
    row!(
        "A\nB\n",
        NONE,
        before(2, &["N"]),
        "A\n2026-09-29 N\nB\n",
        ["2 added: N"],
        true
    ), // 14.31
    row!(
        "A\n\n\n",
        NONE,
        texts(&["N"]),
        "A\n\n\n2026-09-29 N\n",
        ["4 added: N"],
        false
    ), // 14.34
    row!(
        "A\nB",
        NONE,
        texts(&["N"]),
        "A\nB\n2026-09-29 N\n",
        ["3 added: N"],
        false
    ), // 14.35: an add ends the file with a newline
    row!(
        "",
        NONE,
        texts(&["L1", "L2"]),
        "2026-09-29 L1\n2026-09-29 L2\n",
        ["1 added: L1", "2 added: L2"],
        false
    ), // 14.36
    row!(
        "",
        NONE,
        random(&["L1", "L2"]),
        "2026-09-29 L1 id:aaaa\n2026-09-29 L2 id:bbbb\n",
        ["1 added as id:aaaa: L1", "2 added as id:bbbb: L2"],
        false
    ), // 14.38
    row!(
        "",
        NONE,
        random(&["L1", "L2 id:aaaa"]),
        "2026-09-29 L1 id:bbbb\n2026-09-29 L2 id:aaaa\n",
        ["1 added as id:bbbb: L1", "2 added as id:aaaa: L2"],
        false
    ), // later typed id
    row!(
        "",
        AGENT,
        texts(&["L1", "L2 id:aaaa"]),
        "2026-09-29 L1 id:bbbb\n2026-09-29 L2 id:aaaa\n",
        ["1 added as id:bbbb: L1", "2 added as id:aaaa: L2"],
        false
    ), // agent, later typed id
    row!(
        "A\nB\n",
        NONE,
        after(Target::todo(1), &["L1", "L2"]),
        "A\n2026-09-29 L1\n2026-09-29 L2\nB\n",
        ["2 added: L1", "3 added: L2"],
        true
    ), // 14.40
];

#[test]
fn adds_lines() {
    for (todo, setup, params, want, msgs, renumbered) in ADDED {
        let (file, result) = run_with(todo, *setup, &params());
        let label = format!("{want:?}");
        let out = result.unwrap_or_else(|e| panic!("{label}: {e}"));
        assert_eq!(file, *want);
        assert_eq!(out.messages(), said(msgs), "{label}");
        assert_eq!(out.renumbered(), *renumbered, "{label}");
        assert!(out.before().is_empty(), "{label}");
    }
}

#[test]
fn adds_after_an_id() {
    // 14.32
    let target = super::parse_one("add", "id:k3f9").unwrap();
    let params = after(target, &["N"]);
    let (file, result) = run_with("A id:k3f9\nB\n", NONE, &params);
    assert!(result.unwrap().renumbered());
    assert_eq!(file, "A id:k3f9\n2026-09-29 N\nB\n");
}

#[test]
fn reports_the_new_item_numbers() {
    // 14.28
    let (_, result) = run_with("A\nB\n", NONE, &top(&["N"]));
    let out = result.unwrap();
    let nrs: Vec<u32> = out.items_after().map(|i| i.nr().get()).collect();
    assert_eq!(nrs, [1]);
}

/// Todo file, setup, params, code, hint, `field`.
type Refusal = (
    &'static str,
    Setup,
    fn() -> AddParams,
    Code,
    &'static str,
    &'static str,
);

macro_rules! refusal {
    (
        $todo:expr, $setup:expr, $p:expr, $code:expr, $hint:expr, $field:expr
    ) => {
        (
            $todo,
            $setup,
            (|| $p) as fn() -> AddParams,
            $code,
            $hint,
            $field,
        )
    };
}

const DONE_HINT: &str = "add it, then use lsty do";
const BACKDATE: &str = r#"write "Dentist due:2026-01-01""#;

fn done_anchor() -> AddParams {
    after(Target::at(Source::Done, 1), &["N"])
}

#[rustfmt::skip]
const REFUSED: &[Refusal] = &[
    refusal!(
        "",
        NONE,
        with_pri(Priority::new(b'B'), &["(A) Buy"]),
        Code::Usage,
        "",
        ""
    ), // 14.7
    refusal!(
        "",
        NONE,
        texts(&["x Buy"]),
        Code::InvalidValue,
        DONE_HINT,
        "text"
    ), // 14.8
    refusal!(
        "",
        NONE,
        texts(&["x 2026-09-29 Buy"]),
        Code::InvalidValue,
        DONE_HINT,
        "text"
    ), // 14.8
    refusal!(
        "",
        NONE,
        with_date(false, &["2026-01-01 Dentist"]),
        Code::InvalidValue,
        BACKDATE,
        "text"
    ), // 14.10
    refusal!(
        "",
        NONE,
        with_date(false, &["(A) 2026-01-01 Dentist"]),
        Code::InvalidValue,
        BACKDATE,
        "text"
    ), // 14.10
    refusal!(
        "",
        NONE,
        with_date(false, &["(A)  2026-01-01 Dentist"]),
        Code::InvalidValue,
        BACKDATE,
        "text"
    ), // two spaces
    refusal!(
        "",
        NONE,
        with_date(false, &[" \t2026-01-01 Dentist"]),
        Code::InvalidValue,
        BACKDATE,
        "text"
    ), // leading blanks are trimmed
    refusal!("", NONE, texts(&[""]), Code::Usage, "", ""), // 14.13
    refusal!("", NONE, texts(&["   "]), Code::Usage, "", ""), // 14.13
    refusal!(
        "",
        NONE,
        texts(&["a\nb"]),
        Code::InvalidValue,
        "use lsty addm for several tasks",
        "text"
    ), // 14.14
    refusal!(
        "",
        NONE,
        texts(&["a\rb"]),
        Code::InvalidValue,
        "use lsty addm for several tasks",
        "text"
    ), // 14.14
    refusal!(
        "",
        NONE,
        texts(&["Pay due:soon"]),
        Code::InvalidValue,
        "",
        "due"
    ), // 14.16
    refusal!(
        "",
        NONE,
        texts(&["Pay t:soon"]),
        Code::InvalidValue,
        "",
        "t"
    ), // 14.16
    refusal!(
        "",
        NONE,
        texts(&["Pay rec:1x"]),
        Code::InvalidValue,
        "",
        "rec"
    ), // 14.16
    refusal!("", NONE, chosen("a", &["X id:b"]), Code::Usage, "", ""), // 14.21
    refusal!(
        "A id:parser\n",
        NONE,
        chosen("parser", &["B"]),
        Code::IdTaken,
        "",
        ""
    ), // 14.22
    refusal!(
        "",
        NONE,
        chosen("a,b", &["B"]),
        Code::InvalidValue,
        "",
        "id"
    ), // 14.23
    refusal!(
        "",
        AGENT,
        texts(&["X started:2026-09-29T0900Z"]),
        Code::WrongState,
        "use lsty start",
        ""
    ), // 14.24
    refusal!(
        "",
        AGENT,
        texts(&["X ended:2026-09-29T0900Z"]),
        Code::WrongState,
        "ended: is written by lsty do",
        ""
    ),
    refusal!(
        "",
        AGENT,
        texts(&["X owner:agent1"]),
        Code::WrongState,
        "use lsty start",
        ""
    ), // 14.24
    refusal!(
        "A\n",
        NONE,
        after(Target::todo(9), &["N"]),
        Code::NoItem,
        "",
        ""
    ), // 14.33
    refusal!(
        "A\n\nB\n",
        NONE,
        after(Target::todo(2), &["N"]),
        Code::NoItem,
        "",
        ""
    ), // 14.33
    refusal!(
        "A\n",
        NONE,
        done_anchor(),
        Code::WrongState,
        "done:1 is archived; use restore",
        ""
    ), // 14.33
    refusal!(
        "",
        NONE,
        texts(&["L1", "x L2"]),
        Code::InvalidValue,
        DONE_HINT,
        "text"
    ), // 14.37
    refusal!(
        "",
        NONE,
        chosen("k", &["L1", "L2"]),
        Code::Usage,
        "write id:ID in each line",
        ""
    ), // 14.39
    refusal!(
        "",
        NONE,
        texts(&["X id:a id:b"]),
        Code::WrongState,
        "an item has one id",
        ""
    ), // two ids
    refusal!("", NONE, AddParams::default(), Code::Usage, "", ""), // no texts
];

#[test]
fn refuses_and_writes_nothing() {
    for (todo, setup, params, code, hint, field_name) in REFUSED {
        let (file, result) = run_with(todo, *setup, &params());
        let label = format!("{todo:?} {code:?} {hint}");
        let Err(err) = result else {
            panic!("{label}: accepted");
        };
        assert_eq!(err.code(), *code, "{label}: {}", err.message());
        assert_eq!(err.hint().unwrap_or_default(), *hint, "{label}");
        if !field_name.is_empty() {
            assert_eq!(err.text("field"), Some(*field_name), "{label}");
        }
        assert_eq!(file, *todo, "{label}");
    }
}

#[test]
fn an_id_that_cannot_be_appended_is_refused() {
    // The id would turn the only word into a date or a done mark.
    for word in ["2026-01-01", "x"] {
        let params = AddParams {
            date: Toggle::Off,
            ..chosen("k", &[word])
        };
        let (file, result) = run_with("", NONE, &params);
        let err = result.unwrap_err();
        let (code, message, hint) = parts(&err);
        assert_eq!(code, Code::InvalidValue, "{word}");
        assert_eq!(message, format!("can't give {word} the id:k"));
        assert_eq!(
            hint,
            "the text is only a prefix-like word; reword it first"
        );
        assert_eq!(err.text("field"), Some("id"));
        assert_eq!(file, "", "{word}");
    }
}

#[test]
fn a_taken_id_names_its_holder() {
    // 14.22
    let (_, result) =
        run_with("A id:parser\n", NONE, &chosen("parser", &["B"]));
    let err = result.unwrap_err();
    let Some(Field::Ref(holder)) = err.field("holder") else {
        panic!("holder is not a reference");
    };
    assert_eq!((holder.nr.get(), holder.source), (1, Source::Todo));
}

#[test]
fn a_blocked_item_cannot_start() {
    // 14.26
    let params = texts(&["B dep:a started:2026-09-29T0900Z"]);
    let (file, result) = run_with("A id:a\n", NONE, &params);
    let err = result.unwrap_err();
    assert_eq!(err.code(), Code::Blocked);
    let Some(Field::Refs(blockers)) = err.field("blockers") else {
        panic!("blockers are not references");
    };
    assert_eq!(blockers.len(), 1);
    assert_eq!(blockers[0].nr.get(), 1);
    assert_eq!(file, "A id:a\n");
}

#[test]
fn creates_the_file() {
    // 14.41
    let mut dir = folder("", None);
    std::fs::remove_file(&dir.found.todo).unwrap();
    dir.found.state = FileState::Missing;
    let mut env = dir.env();
    let params = texts(&["N"]);
    assert_eq!(add(&env, &params).unwrap_err().code(), Code::NoFile);
    env.create = true;
    let out = add(&env, &params).unwrap();
    assert_eq!(dir.todo(), "2026-09-29 N\n");
    assert_eq!(out.after().found().state, FileState::Exists);
}

#[test]
fn a_bad_text_late_in_the_list_adds_nothing() {
    // 14.37
    let (file, result) = run_with("A\n", NONE, &texts(&["L1", "x L2"]));
    assert!(result.is_err());
    assert_eq!(file, "A\n");
}

#[test]
fn two_texts_are_one_write() {
    // 14.36
    let (_, result) = run_with("", NONE, &texts(&["L1", "L2"]));
    let out = result.unwrap();
    let undo = out.undo().unwrap();
    assert_eq!(undo.files.len(), 1);
    assert_eq!(undo.files[0].source, Source::Todo);
    assert_eq!(undo.files[0].changes.len(), 2);
}

#[test]
fn several_random_ids_in_one_write_are_distinct() {
    let dir = folder("A id:bbbb\n", Some("x 2026-09-01 Z id:aaaa\n"));
    let ids = Sequence::list(["aaaa", "bbbb", "cccc", "cccc", "dddd"]);
    let mut env = dir.env();
    env.ids = &ids;
    add(&env, &random(&["L1", "L2"])).unwrap();
    assert_eq!(
        dir.todo(),
        "A id:bbbb\n2026-09-29 L1 id:cccc\n2026-09-29 L2 id:dddd\n"
    );
}

#[test]
fn a_random_id_skips_a_chosen_one_typed_in_the_same_write() {
    let dir = folder("A\n", None);
    let ids = Sequence::list(["aaaa", "bbbb"]);
    let mut env = dir.env();
    env.ids = &ids;
    add(&env, &random(&["L1", "L2 id:aaaa"])).unwrap();
    assert_eq!(
        dir.todo(),
        "A\n2026-09-29 L1 id:bbbb\n2026-09-29 L2 id:aaaa\n"
    );
}

fn blocking(target: Blocks, t: &str) -> AddParams {
    AddParams {
        blocks: Some(target),
        ..texts(&[t])
    }
}

#[test]
fn an_item_that_blocks_gives_itself_an_id_and_a_dep_to_the_other() {
    let (file, result) = run_with(
        "A\nB\n",
        NONE,
        &blocking(Blocks::Item(Target::todo(2)), "N"),
    );
    assert_eq!(file, "A\nB dep:aaaa\n2026-09-29 N id:aaaa\n");
    let undo = result.unwrap().undo().unwrap().clone();
    assert_eq!(undo.files.len(), 1);
    assert_eq!(undo.files[0].changes.len(), 2);
}

#[test]
fn an_item_that_blocks_keeps_the_id_it_has() {
    let params = AddParams {
        place: Place::Before(Target::todo(2)),
        ..blocking(Blocks::Item(Target::todo(2)), "N id:zzzz")
    };
    let (file, _) = run_with("A\nB\n", NONE, &params);
    assert_eq!(file, "A\n2026-09-29 N id:zzzz\nB dep:zzzz\n");
}

#[test]
fn an_item_that_blocks_a_followed_line_edits_that_line() {
    let dir = folder("A\nB\n", None);
    let snap = snapshot(&dir, b"A\nB\n", b"");
    let tracked = Tracked::new(
        Pin::new(Nr::new(2).unwrap(), "B", Settle::Line),
        Seen::of(&snap),
    );
    let ids = Sequence::letters();
    let mut env = dir.env();
    env.ids = &ids;
    let params = AddParams {
        place: Place::Beside {
            anchor: tracked.clone(),
            below: false,
        },
        ..blocking(Blocks::Line(tracked), "N")
    };
    add(&env, &params).unwrap();
    assert_eq!(dir.todo(), "A\n2026-09-29 N id:aaaa\nB dep:aaaa\n");
}

#[test]
fn an_item_that_blocks_a_line_the_file_no_longer_has_is_refused() {
    let dir = folder("A\nD\n", None);
    let snap = snapshot(&dir, b"A\nB\nD\n", b"");
    let tracked = Tracked::new(
        Pin::new(Nr::new(2).unwrap(), "B", Settle::Line),
        Seen::of(&snap),
    );
    let ids = Sequence::letters();
    let mut env = dir.env();
    env.ids = &ids;
    let result = add(&env, &blocking(Blocks::Line(tracked), "N"));
    assert!(result.is_err());
    assert_eq!(dir.todo(), "A\nD\n");
}

#[test]
fn an_item_that_blocks_a_done_item_makes_it_wait() {
    let (file, _) = run_with(
        "x 2026-09-01 A\nB\n",
        NONE,
        &blocking(Blocks::Item(Target::todo(1)), "N"),
    );
    assert_eq!(file, "x 2026-09-01 A dep:aaaa\nB\n2026-09-29 N id:aaaa\n");
}

#[test]
fn an_item_that_blocks_is_refused_without_a_target_or_alone() {
    for params in [
        blocking(Blocks::Item(Target::todo(9)), "N"),
        blocking(Blocks::Item(Target::at(Source::Done, 1)), "N"),
        AddParams {
            if_absent: true,
            ..blocking(Blocks::Item(Target::todo(1)), "N")
        },
        AddParams {
            texts: strings(&["N", "M"]),
            ..blocking(Blocks::Item(Target::todo(1)), "N")
        },
    ] {
        let (file, result) = run_with("A\nB\n", NONE, &params);
        assert!(result.is_err());
        assert_eq!(file, "A\nB\n");
    }
}
