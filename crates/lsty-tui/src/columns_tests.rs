use crate::columns::{Layout, Natural};

/// Natural widths, available cells, and the expected tags, date, owner,
/// duration and text widths.
type Row = (Natural, usize, [usize; 5]);

const AGENT: Natural = Natural {
    text: 200,
    tags: 24,
    date: 14,
    owner: 16,
    took: 8,
    priority: 3,
};
/// A claim that is its duration alone: there is no owner to narrow.
const HUMAN: Natural = Natural {
    owner: 0,
    took: 3,
    ..AGENT
};

/// The layout of `natural` after a gutter of 2, in `avail` cells.
fn tags_date_owner_took_text(natural: Natural, avail: usize) -> [usize; 5] {
    let got = Layout::new(natural, 2, avail);
    [got.tags, got.date, got.owner, got.took, got.text]
}

#[test]
fn the_columns_give_way_in_the_order_of_the_steps() {
    // The text is owed 60% of the cells after the number, mark and
    // priority (9 of them), rounded up.
    const ROWS: [Row; 18] = [
        // the full columns leave the text its share: nothing changes
        (AGENT, 200, [24, 14, 16, 8, 121]),
        (AGENT, 184, [24, 14, 16, 8, 105]),
        // 1 the owner narrows, by the cells missing only
        (AGENT, 183, [24, 14, 15, 8, 105]),
        (AGENT, 153, [24, 14, 3, 8, 87]),
        // 2 the tags narrow
        (AGENT, 152, [24, 14, 3, 8, 86]),
        (AGENT, 151, [23, 14, 3, 8, 86]),
        (AGENT, 113, [8, 14, 3, 8, 63]),
        // 3 the owner goes, the tags start again from their full width
        (AGENT, 112, [8, 14, 3, 8, 62]),
        (AGENT, 111, [12, 14, 0, 8, 62]),
        (AGENT, 100, [8, 14, 0, 8, 55]),
        // 4 the tags go
        (AGENT, 99, [8, 14, 0, 8, 54]),
        (AGENT, 98, [0, 14, 0, 8, 63]),
        // 5 the duration goes
        (AGENT, 74, [0, 14, 0, 8, 39]),
        (AGENT, 73, [0, 14, 0, 0, 48]),
        // 6 the date goes
        (AGENT, 49, [0, 14, 0, 0, 24]),
        (AGENT, 48, [0, 0, 0, 0, 39]),
        // then the text has all there is, however wide it is
        (AGENT, 12, [0, 0, 0, 0, 3]),
        // a short owner is not widened: there is nothing missing
        (Natural { owner: 2, ..AGENT }, 120, [12, 14, 2, 8, 67]),
    ];
    for (natural, avail, want) in ROWS {
        let got = tags_date_owner_took_text(natural, avail);
        assert_eq!(got, want, "available {avail}");
    }
}

#[test]
fn a_text_that_fits_beside_the_full_columns_changes_nothing() {
    // 20 cells of text: far under 60% of the list, and beside the columns
    let short = Natural { text: 20, ..AGENT };
    for (avail, want) in [
        (100, [24, 14, 16, 8, 21]),
        (90, [24, 14, 7, 8, 20]),
        (60, [0, 14, 0, 8, 25]),
    ] {
        let got = tags_date_owner_took_text(short, avail);
        assert_eq!(got, want, "available {avail}");
    }
    // no text at all: the columns stay full in a room that holds them, and
    // fit the row in one that does not (the duration and date never cut)
    let empty = Natural { text: 0, ..AGENT };
    for (avail, want) in [
        (120, [24, 14, 16, 8, 41]),
        (40, [0, 14, 0, 8, 5]),
        (30, [0, 14, 0, 0, 5]),
    ] {
        let got = tags_date_owner_took_text(empty, avail);
        assert_eq!(got, want, "available {avail}");
    }
}

#[test]
fn the_share_is_rounded_up() {
    // The text is 10 cells beside the date, with 17 cells to share: 60% of
    // them is 10.2, so the text is owed 11 and the date goes.
    let natural = Natural {
        text: 200,
        date: 5,
        ..Natural::default()
    };
    for (avail, date, text) in [(19, 0, 17), (20, 5, 11)] {
        let got = Layout::new(natural, 0, avail);
        assert_eq!((got.date, got.text), (date, text), "available {avail}");
    }
}

#[test]
fn the_cells_before_the_text_are_not_shared() {
    // no priority and no number: the text has more cells to share
    let plain = Natural {
        priority: 0,
        ..AGENT
    };
    let got = Layout::new(plain, 0, 100);
    assert_eq!([got.tags, got.owner, got.text], [11, 0, 59]);
    let got = Layout::new(plain, 2, 100);
    assert_eq!([got.tags, got.owner, got.text], [10, 0, 57]);
}

#[test]
fn a_human_claim_has_no_owner_to_narrow() {
    let got = Layout::new(HUMAN, 2, 60);
    assert_eq!((got.tags, got.owner, got.took, got.text), (0, 0, 0, 35));
}

#[test]
fn a_column_without_values_takes_no_room() {
    let got = Layout::new(Natural::default(), 0, 40);
    assert_eq!(got.text, 40 - 2);
}

#[test]
fn the_priority_column_is_there_or_not_and_is_never_narrowed() {
    for (priority, avail, text) in [(0, 60, 58), (3, 60, 54), (3, 20, 14)] {
        let natural = Natural {
            priority,
            ..Natural::default()
        };
        let got = Layout::new(natural, 0, avail);
        assert_eq!((got.priority, got.text), (priority, text), "{avail}");
    }
    let crowded = Layout::new(AGENT, 2, 28);
    assert_eq!((crowded.priority, crowded.tags), (3, 0));
}
