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
