use super::{
    IdSource, OsIds, PriPair, Target, depri, id,
    ids::Taken,
    parse_one, parse_refs, pri,
    run::run,
    staged::Path,
    test_util::{Sequence, folder, parts, said},
    tx::Picked,
};
use crate::{
    errs::{Code, Error, Source},
    todotxt::{ID_ALPHABET, Id, Priority},
};

fn refs(arg: &str) -> Vec<Target> {
    parse_refs(arg).expect(arg)
}

fn id_with(
    dir: &super::test_util::Dir,
    ids: &Sequence,
    arg: &str,
    chosen: Option<&str>,
) -> Result<super::Outcome, Error> {
    let mut env = dir.env();
    env.ids = ids;
    id(&env, &refs(arg), chosen)
}

/// The ids the items of a result carry, in order.
fn item_ids(out: &super::Outcome) -> Vec<String> {
    out.items_after()
        .map(|i| {
            let id = out.after().id_of(i).unwrap_or_default();
            String::from_utf8_lossy(id).into_owned()
        })
        .collect()
}

fn holder_of(err: &Error) -> crate::errs::Ref {
    let Some(crate::errs::Field::Ref(holder)) = err.field("holder") else {
        panic!("holder is not a reference");
    };
    holder.clone()
}

fn abc() -> Sequence {
    Sequence::letters()
}

#[test]
fn parses_one_reference_each() {
    let nrs = refs("12");
    assert_eq!(nrs[0].line(), Some((Source::Todo, 12))); // 13.1
    let by_id = &refs("id:k3f9")[0];
    assert_eq!(by_id.id().map(Id::as_str), Some("k3f9"));
    let done = &refs("done:12")[0];
    assert_eq!(done.line(), Some((Source::Done, 12)));
    assert_eq!(refs("05")[0].line(), Some((Source::Todo, 5)));
}

#[test]
fn parses_lists_in_order() {
    let list = refs("3,5,8");
    let nrs: Vec<_> = list
        .iter()
        .filter_map(Target::line)
        .map(|(_, nr)| nr)
        .collect();
    assert_eq!(nrs, [3, 5, 8]);
    assert_eq!(refs("id:k3f9,id:a1b2").len(), 2);
    let mixed = refs("3,id:k3f9,done:2");
    assert_eq!(mixed.len(), 3);
    assert_eq!(mixed[1].id().map(Id::as_str), Some("k3f9"));
    assert_eq!(mixed[2].line(), Some((Source::Done, 2)));
}

#[test]
fn refuses_what_is_not_a_reference() {
    // The message names the element that is wrong.
    let bad = [
        ("3,,5", ""),
        ("3,", ""),
        ("-3", "-3"),
        ("abc", "abc"),
        ("ID:k3f9", "ID:k3f9"),
        ("done:", "done:"),
        ("id:", "id:"),
        ("3 5", "3 5"),
        ("", ""),
    ];
    for (arg, element) in bad {
        let err = parse_refs(arg).unwrap_err();
        let want = format!("\"{element}\" is not a reference");
        assert_eq!(
            parts(&err),
            (Code::Usage, want.as_str(), "use 12, id:k3f9 or done:12"),
            "{arg:?}"
        );
    }
    // 13.4: the second element is the bad one.
    let err = parse_refs("id:a,b").unwrap_err();
    assert_eq!(err.message(), "\"b\" is not a reference");
}

#[test]
fn refuses_an_invalid_id() {
    for arg in ["id:-x", "id:é", "id:a b"] {
        let err = parse_refs(arg).unwrap_err();
        assert_eq!(err.code(), Code::InvalidValue, "{arg}");
        assert_eq!(err.text("field"), Some("id"), "{arg}");
    }
}

#[test]
fn handles() {
    let handles: Vec<String> = ["12", "id:k3f9", "done:12"]
        .iter()
        .map(|a| refs(a)[0].handle())
        .collect();
    assert_eq!(handles, ["12", "k3f9", "done:12"]);
}

#[test]
fn a_single_reference_refuses_a_list() {
    let err = parse_one("show", "id:a,id:b").unwrap_err();
    assert_eq!(err.message(), "show takes one reference");
}

fn resolved(dir: &super::test_util::Dir, arg: &str) -> Result<String, Error> {
    let env = dir.env();
    let out = pri(
        &env,
        &[PriPair {
            target: refs(arg).remove(0),
            priority: Priority::new(b'C'),
        }],
    )?;
    Ok(out
        .messages()
        .iter()
        .map(|m| String::from_utf8_lossy(m))
        .collect())
}

#[test]
fn resolves_ids() {
    // 13.7
    let dir = folder("A id:k3f9\n", None);
    assert_eq!(resolved(&dir, "id:k3f9").unwrap(), "k3f9 prioritized (C)");
    assert_eq!(dir.todo(), "(C) A id:k3f9\n");
    // 13.11
    let err = resolved(&dir, "id:zzzz").unwrap_err();
    assert_eq!(parts(&err).1, "no item id:zzzz");
    assert_eq!(err.text("id"), Some("zzzz"));
    assert!(err.field("nr").is_none());
}

#[test]
fn an_archived_id_is_read_only() {
    // 13.8
    let dir = folder("B\n", Some("x 2026-09-01 2026-08-01 A id:k3f9\n"));
    let err = resolved(&dir, "id:k3f9").unwrap_err();
    assert_eq!(err.code(), Code::WrongState);
}

#[test]
fn an_id_in_both_files_finds_the_todo_item() {
    // 13.9
    let dir = folder("A id:k\n", Some("x 2026-09-01 2026-08-01 A id:k\n"));
    let msgs = resolved(&dir, "id:k").unwrap();
    assert_eq!(dir.todo(), "(C) A id:k\n");
    assert!(msgs.contains("id:k is in todo.txt and done.txt"), "{msgs}");
}

#[test]
fn an_id_on_two_lines_is_ambiguous() {
    // 13.10
    let dir = folder("A id:k\nB id:k\n", None);
    let err = resolved(&dir, "id:k").unwrap_err();
    assert_eq!(err.code(), Code::Ambiguous);
    assert_eq!(err.code().exit(), 3);
}

#[test]
fn the_same_item_twice_counts_once() {
    // 13.12
    let dir = folder("A id:k3f9\n", None);
    let env = dir.env();
    let pairs: Vec<PriPair> = refs("1,id:k3f9")
        .into_iter()
        .map(|target| PriPair {
            target,
            priority: Priority::new(b'B'),
        })
        .collect();
    let out = pri(&env, &pairs).unwrap();
    assert_eq!(out.items().len(), 1);
    assert_eq!(out.messages().len(), 1);
    let dir = folder("(A) A id:k3f9\n", None);
    let targets = refs("1,id:k3f9");
    let out = depri(&dir.env(), &targets).unwrap();
    assert_eq!((out.items().len(), out.messages().len()), (1, 1));
}

#[test]
fn gives_a_random_id() {
    // 13.13
    let dir = folder("A\n", None);
    let out = id_with(&dir, &abc(), "1", None).unwrap();
    assert_eq!(dir.todo(), "A id:aaaa\n");
    assert_eq!(out.messages(), said(&["1 is id:aaaa"]));
    assert_eq!(item_ids(&out), ["aaaa"]);
    // 13.14
    let dir = folder("A id:aaaa\nB\n", None);
    let out = id_with(&dir, &abc(), "2", None).unwrap();
    assert_eq!(dir.todo(), "A id:aaaa\nB id:bbbb\n");
    assert_eq!(item_ids(&out), ["bbbb"]);
    // 13.15
    let dir = folder("B\n", Some("x 2026-09-01 2026-08-01 Z id:aaaa\n"));
    let out = id_with(&dir, &abc(), "1", None).unwrap();
    assert_eq!(dir.todo(), "B id:bbbb\n");
    assert_eq!(item_ids(&out), ["bbbb"]);
}

#[test]
fn a_later_id_token_is_avoided() {
    let dir = folder("A id:x id:aaaa\nB\n", None);
    id_with(&dir, &abc(), "2", None).unwrap();
    assert_eq!(dir.todo(), "A id:x id:aaaa\nB id:bbbb\n");
}

#[test]
fn the_length_grows_after_sixteen_clashes() {
    // 13.16
    let mut ids = vec!["aaaa"; 16];
    ids.push("aaaab");
    let dir = folder("A\nB id:aaaa\n", None);
    id_with(&dir, &Sequence::list(ids), "1", None).unwrap();
    assert_eq!(dir.todo(), "A id:aaaab\nB id:aaaa\n");
}

#[test]
fn a_chosen_id() {
    // 13.17
    let dir = folder("A\n", None);
    let out = id_with(&dir, &abc(), "1", Some("parser")).unwrap();
    assert_eq!(dir.todo(), "A id:parser\n");
    assert_eq!(item_ids(&out), ["parser"]);
}

#[test]
fn an_item_with_an_id_keeps_it() {
    // 13.18
    let dir = folder("A id:parser\n", None);
    let out = id_with(&dir, &abc(), "1", Some("parser")).unwrap();
    assert_eq!(out.messages(), said(&["1 is id:parser (unchanged)"]));
    assert!(out.no_op());
    assert_eq!(item_ids(&out), ["parser"]);
    let out = id_with(&dir, &abc(), "1", None).unwrap();
    assert_eq!(out.messages(), said(&["1 is id:parser (unchanged)"]));
    assert_eq!(item_ids(&out), ["parser"]);
    // 13.19
    let err = id_with(&dir, &abc(), "1", Some("other")).unwrap_err();
    assert_eq!(
        parts(&err),
        (
            Code::WrongState,
            "1 already has id:parser",
            "an id is never changed"
        )
    );
}

#[test]
fn a_taken_id_names_its_holder() {
    // 13.20
    let dir = folder("A id:parser\nB\n", None);
    let err = id_with(&dir, &abc(), "2", Some("parser")).unwrap_err();
    assert_eq!(err.code(), Code::IdTaken);
    assert_eq!(err.text("taken"), Some("parser"));
    let holder = holder_of(&err);
    assert_eq!((holder.nr.get(), holder.source), (1, Source::Todo));
    assert_eq!(holder.id.as_deref(), Some("parser"));
    assert_eq!(dir.todo(), "A id:parser\nB\n");
    // 13.21
    let dir = folder("B\n", Some("x 2026-09-01 2026-08-01 A id:parser\n"));
    let err = id_with(&dir, &abc(), "1", Some("parser")).unwrap_err();
    assert_eq!(err.code(), Code::IdTaken);
    let holder = holder_of(&err);
    assert_eq!(holder.source, Source::Done);
    assert_eq!(holder.id.as_deref(), Some("parser"));
}

#[test]
fn a_bad_chosen_id_or_a_list() {
    let dir = folder("A\nB\n", None);
    for chosen in ["a,b", "-x"] {
        let err = id_with(&dir, &abc(), "1", Some(chosen)).unwrap_err();
        assert_eq!(err.code(), Code::InvalidValue, "{chosen}");
        assert_eq!(err.text("field"), Some("id"));
    }
    let err = id_with(&dir, &abc(), "1,2", Some("parser")).unwrap_err();
    assert_eq!(err.code(), Code::Usage); // 13.23
}

#[test]
fn several_targets_in_one_write() {
    // 13.24
    let dir = folder("A\nB\n", None);
    let out = id_with(&dir, &abc(), "1,2", None).unwrap();
    assert_eq!(dir.todo(), "A id:aaaa\nB id:bbbb\n");
    assert_eq!(item_ids(&out), ["aaaa", "bbbb"]);
    // 13.25
    let dir = folder("A\nB\n", None);
    let err = id_with(&dir, &abc(), "1,9", None).unwrap_err();
    assert_eq!(err.code(), Code::NoItem);
    assert_eq!(dir.todo(), "A\nB\n");
}

#[test]
fn a_done_item_in_the_todo_file_may_get_an_id() {
    // 13.26
    let dir = folder("x 2026-09-28 A\n", None);
    let out = id_with(&dir, &abc(), "1", None).unwrap();
    assert_eq!(dir.todo(), "x 2026-09-28 A id:aaaa\n");
    assert_eq!(item_ids(&out), ["aaaa"]);
    // 13.27
    let dir = folder("B\n", Some("x 2026-09-01 2026-08-01 A\n"));
    let err = id_with(&dir, &abc(), "done:1", None).unwrap_err();
    assert_eq!(
        parts(&err),
        (
            Code::WrongState,
            "done:1 is read-only history",
            "done:1 is archived; use restore"
        )
    );
}

fn set_pri(dir: &super::test_util::Dir) -> Result<super::Outcome, Error> {
    let target = Target::todo(1);
    pri(
        &dir.env(),
        &[PriPair {
            target,
            priority: Priority::new(b'B'),
        }],
    )
}

#[test]
fn another_tools_id_is_kept() {
    // 13.28
    let dir = folder("A id:a,b\n", None);
    set_pri(&dir).unwrap();
    assert_eq!(dir.todo(), "(B) A id:a,b\n");
}

#[test]
fn a_second_id_is_refused() {
    // 13.29: the text path adds a second token.
    let dir = folder("A id:k3f9\n", None);
    let out = id_with(&dir, &abc(), "1", None).unwrap();
    assert!(out.no_op());
    let err = text_edit(&dir, b"A id:k3f9 id:x").unwrap_err();
    assert_eq!(
        parts(&err),
        (
            Code::WrongState,
            "1 already has id:k3f9",
            "an item has one id"
        )
    );
}

fn text_edit(
    dir: &super::test_util::Dir,
    raw: &[u8],
) -> Result<super::Outcome, Error> {
    run(&dir.env(), "edit", false, |tx| {
        let Picked::Item(item) = tx.target(&Target::todo(1))? else {
            return Ok(());
        };
        tx.set(item, raw, Path::Text);
        Ok(())
    })
}

#[test]
fn two_staged_lines_with_one_id() {
    // 13.30
    let dir = folder("A\nB\n", None);
    let err = run(&dir.env(), "test", false, |tx| {
        for nr in [1, 2] {
            let Picked::Item(item) = tx.target(&Target::todo(nr))? else {
                continue;
            };
            let raw =
                tx.line(item).map(|l| l.raw().to_vec()).unwrap_or_default();
            tx.set(item, [raw, b" id:same".to_vec()].concat(), Path::Action);
        }
        Ok(())
    })
    .unwrap_err();
    assert_eq!(err.code(), Code::IdTaken);
}

#[test]
fn an_added_line_may_not_take_an_id_that_exists() {
    let dir = folder("A id:one\n", None);
    let err = run(&dir.env(), "test", false, |tx| {
        tx.insert(Source::Todo, 1, b"B id:one", Path::Action);
        Ok(())
    })
    .unwrap_err();
    assert_eq!(err.code(), Code::IdTaken);
    assert_eq!(err.message(), "id:one is taken by 1");
}

#[test]
fn an_added_line_with_a_bad_id() {
    let dir = folder("A\n", None);
    let err = run(&dir.env(), "test", false, |tx| {
        tx.insert(Source::Todo, 1, b"B id:-x", Path::Action);
        Ok(())
    })
    .unwrap_err();
    assert_eq!(err.code(), Code::InvalidValue);
}

#[test]
fn the_default_source_draws_from_the_alphabet() {
    // 13.31
    let mut seen = [false; 32];
    for _ in 0..10_000 {
        let id = OsIds.next(4).unwrap();
        assert_eq!(id.len(), 4);
        for c in id.bytes() {
            let at = ID_ALPHABET
                .iter()
                .position(|&a| a == c)
                .expect("in alphabet");
            seen[at] = true;
        }
    }
    assert!(seen.iter().all(|&s| s));
}

#[test]
fn the_same_item_twice_gets_one_id() {
    let dir = folder("A\n", None);
    let out = id_with(&dir, &abc(), "1,1", None).unwrap();
    assert_eq!(dir.todo(), "A id:aaaa\n");
    assert_eq!(out.messages(), said(&["1 is id:aaaa"]));
    assert_eq!(item_ids(&out), ["aaaa"]);
}

#[test]
fn two_ids_gained_at_once_are_refused() {
    let dir = folder("A\n", None);
    let err = text_edit(&dir, b"A id:a id:b").unwrap_err();
    assert_eq!(
        parts(&err),
        (Code::WrongState, "1 already has id:a", "an item has one id")
    );
}

#[test]
fn an_exhausted_list_source_fails() {
    let dir = folder("A id:aaaa\nB\n", None);
    let err = id_with(&dir, &Sequence::list(["aaaa"]), "2", None).unwrap_err();
    assert_eq!(err.code(), Code::Io);
}

/// Draws one id inside a transaction that first edits the todo item 1 to
/// `edited`, with a source that gives `ids` in turn.
fn draw_after_edit(
    todo: &str,
    done: Option<&str>,
    edited: &str,
    ids: &[&str],
) -> Vec<String> {
    let dir = folder(todo, done);
    let seq = Sequence::list(ids.iter().copied());
    let mut drawn = Vec::new();
    run(&dir.env(), "test", false, |tx| {
        let item = tx.snapshot().items().first().unwrap().item_id();
        tx.set(item, edited, Path::Action);
        let mut taken = Taken::new([]);
        for _ in 0..2 {
            let id = taken.draw(tx.snapshot(), || tx.edited_ids(), &seq)?;
            drawn.push(id.to_string());
        }
        Ok(())
    })
    .unwrap();
    drawn
}

#[test]
fn a_drawn_id_skips_an_id_staged_on_a_changed_line() {
    let drawn = draw_after_edit(
        "A\nB\n",
        None,
        "A id:aaaa",
        &["aaaa", "bbbb", "cccc"],
    );
    assert_eq!(drawn, ["bbbb", "cccc"]);
}

#[test]
fn a_drawn_id_skips_ids_of_either_file_and_later_tokens() {
    let drawn = draw_after_edit(
        "A\nB id:bbbb id:cccc\n",
        Some("x 2026-09-01 Z id:aaaa\n"),
        "A",
        &["aaaa", "bbbb", "cccc", "dddd", "eeee"],
    );
    assert_eq!(drawn, ["dddd", "eeee"]);
}

#[test]
fn an_id_drawn_once_is_not_drawn_again_in_the_same_write() {
    let drawn =
        draw_after_edit("A\n", None, "A", &["aaaa", "aaaa", "aaaa", "bbbb"]);
    assert_eq!(drawn, ["aaaa", "bbbb"]);
}

#[test]
fn a_held_id_is_not_drawn() {
    let dir = folder("A\n", None);
    let seq = Sequence::list(["aaaa", "bbbb"]);
    let mut drawn = String::new();
    run(&dir.env(), "test", false, |tx| {
        let held: [&[u8]; 1] = [b"aaaa"];
        drawn = Taken::new(held)
            .draw(tx.snapshot(), Vec::new, &seq)?
            .to_string();
        Ok(())
    })
    .unwrap();
    assert_eq!(drawn, "bbbb");
}

#[test]
fn a_source_giving_an_unusable_id_is_an_error() {
    let dir = folder("A\n", None);
    let seq = Sequence::list(["a b"]);
    let err = run(&dir.env(), "test", false, |tx| {
        Taken::new([]).draw(tx.snapshot(), Vec::new, &seq).map(drop)
    })
    .unwrap_err();
    assert_eq!(err.code(), Code::Io);
    assert_eq!(
        err.message(),
        "the source of ids gave an id that can't be used"
    );
}
