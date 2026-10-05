use super::{Pin, Settle, Spot};
use crate::errs::Nr;

fn bars(text: &str) -> Vec<&str> {
    if text.is_empty() {
        Vec::new()
    } else {
        text.split('|').collect()
    }
}

fn pin(at: usize, base: &str, plain: bool) -> Pin {
    Pin::new(
        Nr::new(u32::try_from(at + 1).unwrap()).unwrap(),
        base,
        if plain {
            Settle::Line
        } else {
            Settle::LineOrBlank
        },
    )
}

/// Name, the lines seen, the lines now, the index pinned, its base,
/// whether the pin is plain, where it points to begin with (live when
/// `None`), and where it points after.
type Row = (
    &'static str,
    &'static str,
    &'static str,
    usize,
    &'static str,
    bool,
    Option<Spot>,
    Spot,
);

const ROWS: &[Row] = &[
    (
        "a line that maps",
        "a|b|c",
        "x|a|b|c",
        1,
        "b",
        false,
        None,
        Spot::Live(2),
    ),
    (
        "a line found by its id",
        "a|b id:k1|c",
        "c|a|B id:k1",
        1,
        "b id:k1",
        false,
        None,
        Spot::Live(2),
    ),
    (
        "a line changed in place",
        "a|b|c",
        "a|B|c",
        1,
        "b",
        false,
        None,
        Spot::Live(1),
    ),
    (
        "the changed line pairs with the first new line",
        "a|b|c",
        "a|B|N|c",
        1,
        "b",
        false,
        None,
        Spot::Live(1),
    ),
    (
        "a deleted line leaves a blank",
        "a|b|c",
        "a||c",
        1,
        "b",
        false,
        None,
        Spot::Blank(1),
    ),
    (
        "a deleted line, compacted",
        "a|b|c",
        "a|c",
        1,
        "b",
        false,
        None,
        Spot::After(Some(0)),
    ),
    (
        "a deleted first line",
        "a|b",
        "b",
        0,
        "a",
        false,
        None,
        Spot::After(None),
    ),
    (
        "a plain pin never takes a blank",
        "a|b|c",
        "a||c",
        1,
        "b",
        true,
        None,
        Spot::After(Some(0)),
    ),
    (
        "a blank that stays",
        "a||c",
        "x|a||c",
        0,
        "b",
        false,
        Some(Spot::Blank(1)),
        Spot::Blank(2),
    ),
    (
        "a blank that is compacted",
        "a||c",
        "a|c",
        0,
        "b",
        false,
        Some(Spot::Blank(1)),
        Spot::After(Some(0)),
    ),
    (
        "the line a copy follows",
        "a|c",
        "x|a|c",
        0,
        "b",
        false,
        Some(Spot::After(Some(0))),
        Spot::After(Some(1)),
    ),
    (
        "the top stays the top",
        "a",
        "x|a",
        0,
        "b",
        false,
        Some(Spot::After(None)),
        Spot::After(None),
    ),
    (
        "the line it follows is gone",
        "z|a|c",
        "z|c",
        0,
        "b",
        false,
        Some(Spot::After(Some(1))),
        Spot::After(Some(0)),
    ),
    (
        "an id that left the todo file",
        "a|b id:k1",
        "a",
        1,
        "b id:k1",
        false,
        None,
        Spot::After(Some(0)),
    ),
    (
        "an id lost and another line takes its place",
        "a|b id:k1|c",
        "a|other id:k9|c",
        1,
        "b id:k1",
        false,
        None,
        Spot::After(Some(0)),
    ),
    (
        "a changed line after a compacted blank",
        "a||b|c",
        "a|B owner:x|c",
        2,
        "b",
        false,
        None,
        Spot::Live(1),
    ),
    (
        "a deleted line comes back by its id",
        "a|c",
        "a|c|b id:k1",
        0,
        "b id:k1",
        false,
        Some(Spot::After(Some(0))),
        Spot::Live(2),
    ),
    (
        "past the lines seen",
        "a",
        "a",
        3,
        "z",
        false,
        None,
        Spot::After(None),
    ),
];

#[test]
fn a_pin_follows_its_line_from_one_reading_to_the_next() {
    for (name, old, now, at, base, plain, start, want) in ROWS {
        let pin = pin(*at, base, *plain);
        let pin = match start {
            Some(spot) => pin.pointing(*spot),
            None => pin,
        };
        let got = pin.follow(&bars(old), &bars(now));
        assert_eq!(got.spot(), *want, "{name}");
        assert_eq!(got.base(), pin.base(), "{name}");
    }
}

#[test]
fn an_id_in_the_done_file_is_an_archived_item() {
    let pin = pin(0, "b id:k1", false);
    assert!(pin.archived(&["x b id:k1"]));
    assert!(!pin.archived(&["x b id:k2"]));
    assert!(
        !Pin::new(Nr::new(1).unwrap(), "b", Settle::LineOrBlank)
            .archived(&["x b id:k1"])
    );
}

const N: Option<usize> = None;

/// Name, the lines seen, the lines now, where each seen line went, the
/// index pinned, its base, whether the pin is plain, and where it
/// points after.
type Hunked = (
    &'static str,
    &'static str,
    &'static str,
    &'static [Option<usize>],
    usize,
    &'static str,
    bool,
    Spot,
);

const HUNKED: &[Hunked] = &[
    (
        "1:2, the line takes the first new one",
        "a|b|c",
        "a|X|Y|c",
        &[Some(0), N, Some(3)],
        1,
        "b",
        true,
        Spot::Live(1),
    ),
    (
        "2:2, the first line takes the first new one",
        "a|b1|b2|c",
        "a|X|Y|c",
        &[Some(0), N, N, Some(3)],
        1,
        "b1",
        true,
        Spot::Live(1),
    ),
    (
        "2:2, the second line takes the second new one",
        "a|b1|b2|c",
        "a|X|Y|c",
        &[Some(0), N, N, Some(3)],
        2,
        "b2",
        true,
        Spot::Live(2),
    ),
    (
        "2:1, the second line has no new one",
        "a|b1|b2|c",
        "a|X|c",
        &[Some(0), N, N, Some(2)],
        2,
        "b2",
        true,
        Spot::After(Some(0)),
    ),
    (
        "1:2 at the start of the file",
        "b|c",
        "X|Y|c",
        &[N, Some(2)],
        0,
        "b",
        true,
        Spot::Live(0),
    ),
    (
        "2:2 at the start, the second line",
        "b1|b2|c",
        "X|Y|c",
        &[N, N, Some(2)],
        1,
        "b2",
        true,
        Spot::Live(1),
    ),
    (
        "1:2 at the end of the file",
        "a|b",
        "a|X|Y",
        &[Some(0), N],
        1,
        "b",
        true,
        Spot::Live(1),
    ),
    (
        "2:2 at the end, the second line",
        "a|b1|b2",
        "a|X|Y",
        &[Some(0), N, N],
        2,
        "b2",
        true,
        Spot::Live(2),
    ),
    (
        "a blank new line is the blank the item left",
        "a|b|c",
        "a||c",
        &[Some(0), N, Some(2)],
        1,
        "b",
        false,
        Spot::Blank(1),
    ),
    (
        "a plain pin does not settle on a blank",
        "a|b|c",
        "a||c",
        &[Some(0), N, Some(2)],
        1,
        "b",
        true,
        Spot::After(Some(0)),
    ),
    (
        "a pin with an id does not take a new line",
        "a|b id:k|c",
        "a|X|c",
        &[Some(0), N, Some(2)],
        1,
        "b id:k",
        true,
        Spot::After(Some(0)),
    ),
    (
        "a pin with an id may settle on a blank",
        "a|b id:k|c",
        "a||c",
        &[Some(0), N, Some(2)],
        1,
        "b id:k",
        false,
        Spot::Blank(1),
    ),
    (
        "a blank among the seen lines is not counted in the rank",
        "a||b|c",
        "a|X|Y|c",
        &[Some(0), N, N, Some(3)],
        2,
        "b",
        true,
        Spot::Live(1),
    ),
    (
        "a line past the lines seen is after the top",
        "a|b|c",
        "a|b|c",
        &[Some(0), Some(1), Some(2)],
        5,
        "z",
        true,
        Spot::After(None),
    ),
];

#[test]
fn a_vanished_line_is_paired_with_the_lines_inserted_in_its_hunk() {
    for (name, old, now, map, at, base, plain, want) in HUNKED {
        let pin = pin(*at, base, *plain);
        let got = pin.follow_mapped(map, &bars(old), &bars(now));
        assert_eq!(got.spot(), *want, "{name}");
    }
}

/// The map, the index, and the first and last index of the run of
/// changed lines around it.
type Run = (&'static [Option<usize>], usize, (usize, usize));

const RUNS: &[Run] = &[
    (&[Some(0), N, Some(1)], 1, (1, 1)),
    (&[N, Some(0)], 0, (0, 0)),
    (&[N, N, Some(0)], 0, (0, 1)),
    (&[N, N, Some(0)], 1, (0, 1)),
    (&[Some(0), N, N], 1, (1, 2)),
    (&[Some(0), N, N], 2, (1, 2)),
    (&[N, N, N], 1, (0, 2)),
    (&[Some(0), N, N, Some(1)], 2, (1, 2)),
    (&[N, Some(0), N], 2, (2, 2)),
    (&[Some(0), Some(1)], 0, (0, 0)),
];

#[test]
fn a_hunk_is_the_run_of_changed_lines_around_a_line() {
    for (map, at, want) in RUNS {
        assert_eq!(super::pin::hunk(map, *at), *want, "{map:?} at {at}");
    }
}

#[test]
fn a_bound_is_the_place_of_a_followed_line() {
    let map = [Some(3), N, Some(7)];
    assert_eq!(super::pin::bound(&map, None), None);
    assert_eq!(super::pin::bound(&map, Some(0)), Some(3));
    assert_eq!(super::pin::bound(&map, Some(1)), None);
    assert_eq!(super::pin::bound(&map, Some(2)), Some(7));
    assert_eq!(super::pin::bound(&map, Some(3)), None);
}

#[test]
fn an_id_is_found_as_a_tag_and_not_as_part_of_a_word() {
    let lines = ["q xid:a", "y id:ab", "z id:a", "w id:a"];
    assert_eq!(super::pin::index_of_id(&lines, b"a"), Some(2));
    assert_eq!(super::pin::index_of_id(&lines, b"ab"), Some(1));
    assert_eq!(super::pin::index_of_id(&lines, b"b"), None);
    assert_eq!(super::pin::index_of_id(&["x"], b"a"), None);
}

#[test]
fn a_rebased_pin_keeps_its_place_and_has_the_new_base() {
    let rebased = pin(2, "old", false).rebased("new");
    assert_eq!(rebased.base(), b"new");
    assert_eq!(rebased.spot(), Spot::Live(2));
}
