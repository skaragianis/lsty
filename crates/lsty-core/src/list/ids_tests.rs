use super::Item;
use super::test_util::snapshot;
use crate::errs::{Code, Field, Nr, Ref, Source};

struct Row {
    name: &'static str,
    todo: &'static [&'static str],
    done: &'static [&'static str],
    id: &'static str,
    /// The item found: source and NR.
    found: Option<(Source, u32)>,
    code: Option<Code>,
    refs: &'static [u32],
    ref_source: Source,
    warnings: &'static [&'static str],
}

const BASE: Row = Row {
    name: "",
    todo: &[],
    done: &[],
    id: "k",
    found: None,
    code: None,
    refs: &[],
    ref_source: Source::Todo,
    warnings: &[],
};

const ROWS: &[Row] = &[
    Row {
        name: "08.15 both files",
        todo: &["A id:k"],
        done: &["x 2026-09-28 A id:k"],
        found: Some((Source::Todo, 1)),
        warnings: &[
            "id:k is in todo.txt and done.txt; using the todo.txt copy",
        ],
        ..BASE
    },
    Row {
        name: "08.16 two lines of the todo file",
        todo: &["A id:k", "B id:k"],
        code: Some(Code::Ambiguous),
        refs: &[1, 2],
        warnings: &["id:k is on 2 lines of todo.txt"],
        ..BASE
    },
    Row {
        name: "08.18 two lines of the done file",
        done: &["x 2026-09-28 A id:k", "x 2026-09-28 B id:k"],
        code: Some(Code::Ambiguous),
        refs: &[1, 2],
        ref_source: Source::Done,
        warnings: &["id:k is on 2 lines of done.txt"],
        ..BASE
    },
    Row {
        name: "two todo lines and a done copy are ambiguous",
        todo: &["A id:k", "B id:k"],
        done: &["x 2026-09-28 C id:k"],
        code: Some(Code::Ambiguous),
        refs: &[1, 2],
        warnings: &[
            "id:k is in todo.txt and done.txt; using the todo.txt copy",
            "id:k is on 2 lines of todo.txt",
        ],
        ..BASE
    },
    Row {
        name: "unknown id",
        todo: &["A id:k"],
        id: "zz",
        code: Some(Code::NoItem),
        ..BASE
    },
    Row {
        name: "an id is case sensitive",
        todo: &["A id:k"],
        id: "K",
        code: Some(Code::NoItem),
        ..BASE
    },
    Row {
        name: "one line",
        todo: &["A", "B id:k"],
        found: Some((Source::Todo, 2)),
        ..BASE
    },
    Row {
        name: "an id in the done file only",
        done: &["x 2026-09-28 A id:k"],
        found: Some((Source::Done, 1)),
        ..BASE
    },
];

fn refs_of(err: &crate::errs::Error) -> Vec<(u32, Source)> {
    err.fields()
        .filter_map(|(key, field)| match (key, field) {
            ("refs", Field::Refs(refs)) => Some(refs),
            _ => None,
        })
        .flatten()
        .map(|r| (r.nr.get(), r.source))
        .collect()
}

#[test]
fn by_id_and_warnings() {
    for row in ROWS {
        let snap = snapshot(row.todo, row.done);
        assert_eq!(snap.warnings(), row.warnings, "{}", row.name);
        match (snap.by_id(row.id), row.code) {
            (Ok(item), None) => {
                assert_eq!(
                    Some((item.source(), item.nr().get())),
                    row.found,
                    "{}",
                    row.name
                );
            }
            (Err(err), Some(code)) => {
                assert_eq!(err.code(), code, "{}", row.name);
                let want: Vec<(u32, Source)> =
                    row.refs.iter().map(|&n| (n, row.ref_source)).collect();
                assert_eq!(refs_of(&err), want, "{}", row.name);
            }
            (got, _) => panic!("{}: {:?}", row.name, got.map(Item::nr)),
        }
    }
}

#[test]
fn error_fields() {
    let snap = snapshot(&["A id:k", "B id:k"], &[]);
    let err = snap.by_id("k").expect_err("ambiguous");
    let fields: Vec<_> = err.fields().collect();
    assert_eq!(fields.len(), 2);
    assert!(fields.contains(&("id", &Field::Str("k".into()))));
    let want = [1, 2].map(|n| Ref {
        nr: Nr::new(n).expect("nr"),
        source: Source::Todo,
        id: Some("k".into()),
    });
    assert!(fields.contains(&("refs", &Field::Refs(want.to_vec()))));
    let err = snap.by_id("zz").expect_err("missing");
    let fields: Vec<_> = err.fields().collect();
    assert_eq!(fields, [("id", &Field::Str("zz".into()))]);
}

#[test]
fn error_messages() {
    let snap = snapshot(&["A id:k", "B id:k"], &[]);
    let err = snap.by_id("k").expect_err("ambiguous");
    assert_eq!(err.message(), "id:k is on 2 lines");
    let err = snap.by_id("zz").expect_err("missing");
    assert_eq!(err.message(), "no item id:zz");
}

#[test]
fn first_with_id_takes_the_first_line() {
    let snap = snapshot(&["A id:k", "B id:k"], &["x 2026-09-28 C id:k"]);
    let item = snap.first_with_id("k").expect("item");
    assert_eq!((item.source(), item.nr().get()), (Source::Todo, 1));
    assert!(snap.first_with_id("nope").is_none());
}

#[test]
fn warnings_for_items_that_carry_the_id() {
    let snap = snapshot(
        &["A id:k", "B id:k", "C id:m", "D id:n", "E id:n"],
        &["x 2026-09-28 F id:m"],
    );
    let pick = |nrs: &[usize]| {
        let items = nrs.iter().filter_map(|&i| snap.items().get(i));
        snap.warnings_for(items)
    };
    assert_eq!(pick(&[0]), ["id:k is on 2 lines of todo.txt"]);
    assert_eq!(
        pick(&[2, 3]),
        [
            "id:m is in todo.txt and done.txt; using the todo.txt copy",
            "id:n is on 2 lines of todo.txt",
        ]
    );
    assert_eq!(pick(&[]).len(), 0);
}

#[test]
fn warnings_come_in_a_fixed_order() {
    let snap = snapshot(
        &["A id:k", "B id:k", "C id:m"],
        &["x 2026-09-28 F id:m", "x 2026-09-28 G id:m"],
    );
    assert_eq!(
        snap.warnings(),
        [
            "id:m is in todo.txt and done.txt; using the todo.txt copy",
            "id:k is on 2 lines of todo.txt",
            "id:m is on 2 lines of done.txt",
        ]
    );
}

#[test]
fn ids_that_are_not_text_are_found_by_bytes() {
    let snap = super::Snapshot::build(
        super::test_util::found("/d"),
        crate::todotxt::File::parse(b"A id:\xff\xfe\nB dep:\xff\xfe\n"),
        crate::todotxt::File::default(),
        &super::test_util::params(),
    );
    let item = snap.by_id(b"\xff\xfe").expect("item");
    assert_eq!(item.nr().get(), 1);
    assert!(snap.items().get(1).is_some_and(Item::blocked));
}
