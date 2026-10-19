use crate::columns::{Layout, Natural};

/// Natural widths, available cells, and the expected date, id, duration
/// and text widths.
type Row = (Natural, usize, [usize; 4]);

const FULL: Natural = Natural {
    text: 200,
    date: 14,
    id: 4,
    duration: 8,
    priority: 3,
};

/// The layout of `natural` after a gutter of 2, in `avail` cells.
fn date_id_duration_text(natural: Natural, avail: usize) -> [usize; 4] {
    let got = Layout::new(natural, 2, avail);
    [got.date, got.id, got.duration, got.text]
}

#[test]
fn the_columns_give_way_in_their_order() {
    // The text is owed 60% of the cells after the number, mark and
    // priority (9 of them), rounded up.
    const ROWS: [Row; 11] = [
        // the full columns leave the text its share: nothing changes
        (FULL, 200, [14, 4, 8, 159]),
        (FULL, 89, [14, 4, 8, 48]),
        // 1 the duration goes
        (FULL, 88, [14, 4, 0, 57]),
        (FULL, 64, [14, 4, 0, 33]),
        // 2 the date goes
        (FULL, 63, [0, 4, 0, 48]),
        (FULL, 24, [0, 4, 0, 9]),
        // 3 the id goes
        (FULL, 23, [0, 0, 0, 14]),
        // then the text has all there is, however wide it is
        (FULL, 12, [0, 0, 0, 3]),
        // a column that is not there takes nothing
        (Natural { id: 0, ..FULL }, 74, [14, 0, 8, 39]),
        (Natural { id: 0, ..FULL }, 73, [14, 0, 0, 48]),
        (
            Natural {
                duration: 0,
                ..FULL
            },
            88,
            [14, 4, 0, 57],
        ),
    ];
    for (natural, avail, want) in ROWS {
        let got = date_id_duration_text(natural, avail);
        assert_eq!(got, want, "available {avail}");
    }
}

#[test]
fn a_text_that_fits_beside_the_full_columns_changes_nothing() {
    // 20 cells of text: far under 60% of the list, and beside the columns
    let short = Natural { text: 20, ..FULL };
    for (avail, want) in [
        (100, [14, 4, 8, 59]),
        (60, [14, 4, 0, 29]),
        (45, [0, 4, 0, 30]),
    ] {
        let got = date_id_duration_text(short, avail);
        assert_eq!(got, want, "available {avail}");
    }
    // no text at all: the columns stay full in a room that holds them, and
    // fit the row in one that does not
    let empty = Natural { text: 0, ..FULL };
    for (avail, want) in [
        (120, [14, 4, 8, 79]),
        (40, [14, 4, 0, 9]),
        (30, [0, 4, 0, 15]),
    ] {
        let got = date_id_duration_text(empty, avail);
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
        ..FULL
    };
    let got = Layout::new(plain, 0, 100);
    assert_eq!([got.date, got.text], [14, 66]);
    let got = Layout::new(plain, 2, 100);
    assert_eq!([got.date, got.text], [14, 63]);
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
    let crowded = Layout::new(FULL, 2, 28);
    assert_eq!((crowded.priority, crowded.date), (3, 0));
}

#[test]
fn a_list_narrower_than_its_lead_has_no_text() {
    for avail in [0, 1, 3] {
        let got = Layout::new(FULL, 2, avail);
        assert_eq!(got.text, 0, "available {avail}");
    }
}
