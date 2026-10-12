use crate::connective::{
    Conn::{self, And, Not, Or},
    Term, clear_term, cycle, mark, required, step, terms,
};

/// The active names after each step of `names`, written as in the panel:
/// `a` for a name, in the order they are stepped.
fn walk(names: &str) -> Vec<Term> {
    let mut active = Vec::new();
    for name in names.split(' ') {
        step(&mut active, name.as_bytes());
    }
    active
}

fn of(items: &[(&str, Conn)]) -> Vec<Term> {
    items
        .iter()
        .map(|&(name, conn)| Term {
            name: name.into(),
            conn,
        })
        .collect()
}

fn text(active: &[Term]) -> String {
    let all: Vec<String> =
        terms("@", active).into_iter().map(|(_, t)| t).collect();
    all.join(" ")
}

#[test]
fn the_first_name_cycles_off_and_not_off() {
    let steps = [(None, Some(And)), (Some(And), Some(Not)), (Some(Not), None)];
    for (now, next) in steps {
        assert_eq!(cycle(true, now), next, "{now:?}");
    }
}

#[test]
fn a_later_name_cycles_or_and_not_off() {
    let steps = [
        (None, Some(Or)),
        (Some(Or), Some(And)),
        (Some(And), Some(Not)),
        (Some(Not), None),
    ];
    for (now, next) in steps {
        assert_eq!(cycle(false, now), next, "{now:?}");
    }
}

#[test]
fn stepping_adds_a_name_last_and_keeps_its_place_while_it_cycles() {
    let mut active = walk("a b c");
    assert_eq!(active, of(&[("a", And), ("b", Or), ("c", Or)]));
    step(&mut active, b"b");
    assert_eq!(active, of(&[("a", And), ("b", And), ("c", Or)]));
    step(&mut active, b"b");
    assert_eq!(active, of(&[("a", And), ("b", Not), ("c", Or)]));
    step(&mut active, b"d");
    assert_eq!(
        active[3],
        Term {
            name: "d".into(),
            conn: Or
        }
    );
}

#[test]
fn the_text_reads_as_the_filter_typed_by_hand() {
    let mut active = walk("a b c d");
    step(&mut active, b"c");
    step(&mut active, b"d");
    step(&mut active, b"d");
    assert_eq!(active, of(&[("a", And), ("b", Or), ("c", And), ("d", Not)]));
    assert_eq!(text(&active), "@a|@b @c -@d");
}

#[test]
fn an_alternative_after_a_not_widens_what_is_excluded() {
    let active = of(&[("a", Not), ("b", Or)]);
    assert_eq!(text(&active), "-@a|@b");
}

#[test]
fn a_chip_starts_where_its_first_name_is() {
    let active = of(&[("a", And), ("b", Or), ("c", And), ("d", Not)]);
    let at: Vec<usize> = terms("@", &active).iter().map(|t| t.0).collect();
    assert_eq!(at, [0, 2, 3]);
}

#[test]
fn the_sigil_stands_before_every_name() {
    let active = of(&[("a", And), ("b.c", Or)]);
    let all = terms("+", &active);
    assert_eq!(all, [(0, "+a|+b.c".to_owned())]);
}

#[test]
fn the_marks_follow_the_connectives() {
    let active = of(&[("a", And), ("b", Or), ("c", And), ("d", Not)]);
    let marks: Vec<_> = "abcdz".bytes().map(|n| mark(&active, &[n])).collect();
    assert_eq!(marks, [Some(">"), Some("|"), Some("&"), Some("-"), None]);
    let lone = of(&[("a", Not)]);
    assert_eq!(mark(&lone, b"a"), Some("-"));
}

/// A name stepped off from the end of its cycle: the active names
/// before and after, written as `a -b|c`, and the mark of the first.
#[test]
fn a_name_off_hands_its_connective_to_the_alternative_after_it() {
    let table = [
        (of(&[("a", Not), ("b", Or)]), "a", "-@b", "-"),
        (of(&[("a", Not), ("b", Or), ("c", Or)]), "a", "-@b|@c", "-"),
        (of(&[("a", And), ("b", Or), ("c", Or)]), "b", "@a -@c", ">"),
        (of(&[("a", Not), ("b", And)]), "a", "@b", ">"),
        (of(&[("a", And), ("b", Not)]), "a", "-@b", "-"),
        (
            of(&[("a", And), ("b", Or), ("c", And), ("d", Not)]),
            "b",
            "@a @c -@d",
            ">",
        ),
    ];
    for (before, gone, shown, first) in table {
        let mut active = before;
        // The name is a `-` entry when it goes, as the cycle ends there.
        while active.iter().any(|t| t.name == gone.as_bytes()) {
            step(&mut active, gone.as_bytes());
        }
        assert_eq!(text(&active), shown, "{shown}");
        let head = active[0].name.clone();
        assert_eq!(mark(&active, &head), Some(first), "{shown}");
    }
}

#[test]
fn a_term_clears_with_its_alternatives() {
    let mut active = of(&[("a", And), ("b", Or), ("c", And), ("d", Or)]);
    clear_term(&mut active, 0);
    assert_eq!(active, of(&[("c", And), ("d", Or)]));
    clear_term(&mut active, 1);
    assert_eq!(active, of(&[("c", And)]));
    clear_term(&mut active, 5);
    assert_eq!(active, of(&[("c", And)]));
}

#[test]
fn a_new_item_has_the_plain_terms_no_alternative_joins() {
    let active = of(&[
        ("a", And),
        ("b", Or),
        ("c", And),
        ("d", Not),
        ("e", And),
        ("f", Or),
        ("g", And),
    ]);
    let names: Vec<&[u8]> =
        required(&active).map(|t| t.name.as_slice()).collect();
    assert_eq!(names, [b"c".as_slice(), b"g"]);
    assert_eq!(required(&[]).count(), 0);
}
