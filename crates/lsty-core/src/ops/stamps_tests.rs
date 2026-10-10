use super::{
    run::run,
    staged::Path,
    test_util::{folder, parts},
    tx::Tx,
};
use crate::errs::{Code, Nr, Source};

fn set_text(
    dir: &super::test_util::Dir,
    raw: &str,
) -> crate::errs::Result<()> {
    run(&dir.env(), "text", false, |tx: &mut Tx| {
        let item = tx
            .snapshot()
            .at(Source::Todo, Nr::new(1).expect("nr"))
            .expect("item")
            .item_id();
        tx.set(item, raw, Path::Text);
        let line = tx.line_ref(item).expect("line");
        tx.primary(&[line]);
        Ok(())
    })
    .map(drop)
}

const ID_HINT: &str =
    "an id is never changed; lsty untag 1 id:k3f9 removes it";
const REOPEN: &str = "use lsty reopen 1";
const DONE_MSG: &str = "1 is done; reopen it first";
const STAMP_MSG: &str = "1: a text edit can't change a stamp";

/// Name, the file, the text set, and the hint of the refusal; empty when
/// the edit is accepted. The message is the one of a stamp unless it is
/// `DONE_MSG`.
type StampRow = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
);

const STAMPS: &[StampRow] = &[
    (
        "12.13",
        "Buy\n",
        "x 2026-09-29 Buy",
        "use lsty do 1",
        STAMP_MSG,
    ),
    ("12.14", "x 2026-09-28 Fix\n", "Fix", REOPEN, STAMP_MSG),
    (
        "12.15",
        "x 2026-09-28 Fix\n",
        "x 2026-09-20 Fix",
        "the completion date is stamped",
        STAMP_MSG,
    ),
    (
        "12.16 changed",
        "2026-09-20 Buy\n",
        "2026-09-21 Buy",
        "the creation date is stamped",
        STAMP_MSG,
    ),
    (
        "12.16 removed",
        "2026-09-20 Buy\n",
        "Buy",
        "the creation date is stamped",
        STAMP_MSG,
    ),
    (
        "12.17",
        "Buy\n",
        "2026-09-20 Buy",
        "the creation date is stamped",
        STAMP_MSG,
    ),
    (
        "12.18 changed",
        "Buy id:k3f9\n",
        "Buy id:zzzz",
        ID_HINT,
        STAMP_MSG,
    ),
    ("12.18 removed", "Buy id:k3f9\n", "Buy", ID_HINT, STAMP_MSG),
    ("12.19", "(A) Buy id:k3f9\n", "(B) Buy milk id:k3f9", "", ""),
    (
        "12.32 other letter",
        "x 2026-09-28 Fix pri:B\n",
        "x 2026-09-28 Fix pri:A",
        REOPEN,
        DONE_MSG,
    ),
    (
        "12.32 none",
        "x 2026-09-28 Fix pri:B\n",
        "x 2026-09-28 Fix",
        REOPEN,
        DONE_MSG,
    ),
    (
        "12.32 text",
        "x 2026-09-28 Fix pri:B\n",
        "x 2026-09-28 Mend pri:B",
        "",
        "",
    ),
    ("blank accepted", "Buy\n", "", "", ""),
];

#[test]
fn text_edits() {
    for (name, todo, set, hint, msg) in STAMPS {
        let dir = folder(todo, None);
        let result = set_text(&dir, set);
        if hint.is_empty() {
            result.expect(name);
            assert_eq!(dir.todo(), format!("{set}\n"), "{name}");
            continue;
        }
        let err = result.unwrap_err();
        assert_eq!(parts(&err), (Code::WrongState, *msg, *hint), "{name}");
        assert_eq!(dir.todo(), *todo, "{name}: file");
    }
}

#[test]
fn a_stamp_error_names_the_item_and_its_state() {
    let dir = folder("Buy\n", None);
    let err = set_text(&dir, "x 2026-09-29 Buy").unwrap_err();
    assert_eq!(err.text("state"), Some("todo"));
    assert_eq!(err.item().map(|r| r.nr.get()), Some(1));
    let dir = folder("x 2026-09-28 Fix pri:B\n", None);
    let err = set_text(&dir, "x 2026-09-28 Fix").unwrap_err();
    assert_eq!(err.text("state"), Some("done"));
}

#[test]
fn the_action_path_is_not_checked() {
    let dir = folder("Buy\n", None);
    run(&dir.env(), "do", false, |tx| {
        let item = tx
            .snapshot()
            .at(Source::Todo, Nr::new(1).expect("nr"))
            .expect("item")
            .item_id();
        tx.set(item, "x 2026-09-29 Buy", Path::Action);
        Ok(())
    })
    .unwrap();
    assert_eq!(dir.todo(), "x 2026-09-29 Buy\n", "12.20");
}

#[test]
fn a_new_line_on_the_text_path_must_not_be_done() {
    let rows = [
        ("x 2026-09-29 Call", Some("a new line can't start as done")),
        ("Call", None),
    ];
    for (raw, refused) in rows {
        let dir = folder("Buy\n", None);
        let result = run(&dir.env(), "add", false, |tx| {
            tx.insert(Source::Todo, 1, raw, Path::Text);
            Ok(())
        });
        match refused {
            Some(message) => {
                let err = result.unwrap_err();
                assert_eq!(err.code(), Code::WrongState, "{raw}");
                assert_eq!(err.message(), message, "{raw}");
                assert_eq!(err.hint(), Some("add it open, then use lsty do"));
            }
            None => assert!(result.is_ok(), "{raw}"),
        }
    }
}

#[test]
fn the_stamp_word_refusal() {
    let dir = folder("Buy\n", None);
    let err = run(&dir.env(), "x", false, |tx| {
        let item = tx
            .snapshot()
            .at(Source::Todo, Nr::new(1).expect("nr"))
            .expect("item")
            .item_id();
        Err(tx.stamp_word_refusal(item))
    })
    .unwrap_err();
    assert_eq!(
        parts(&err),
        (
            Code::WrongState,
            STAMP_MSG,
            "the text would start with a stamp-like word; reword it first"
        )
    );
}
