use crate::{
    columns::{Layout, Natural, owner_floor},
    text::width_of,
};

/// Natural widths, available cells, and the expected date, owner,
/// duration and text widths.
type Row = (Natural, usize, [usize; 4]);

const AGENT: Natural = Natural {
    owner_floor: 3,
    text: 200,
    date: 14,
    owner: 16,
    duration: 8,
    priority: 3,
};
/// A claim that is its duration alone: there is no owner to narrow.
const HUMAN: Natural = Natural {
    owner: 0,
    duration: 3,
    ..AGENT
};

/// The layout of `natural` after a gutter of 2, in `avail` cells.
fn date_owner_duration_text(natural: Natural, avail: usize) -> [usize; 4] {
    let got = Layout::new(natural, 2, avail);
    [got.date, got.owner, got.duration, got.text]
}

#[test]
fn the_columns_give_way_in_the_order_of_the_steps() {
    // The text is owed 60% of the cells after the number, mark and
    // priority (9 of them), rounded up.
    const ROWS: [Row; 12] = [
        // the full columns leave the text its share: nothing changes
        (AGENT, 200, [14, 16, 8, 147]),
        (AGENT, 119, [14, 16, 8, 66]),
        // 1 the owner narrows, by the cells missing only
        (AGENT, 118, [14, 15, 8, 66]),
        (AGENT, 88, [14, 3, 8, 48]),
        // 2 the owner goes
        (AGENT, 87, [14, 3, 8, 47]),
        (AGENT, 86, [14, 0, 8, 51]),
        // 3 the duration goes
        (AGENT, 74, [14, 0, 8, 39]),
        (AGENT, 73, [14, 0, 0, 48]),
        // 4 the date goes
        (AGENT, 49, [14, 0, 0, 24]),
        (AGENT, 48, [0, 0, 0, 39]),
        // then the text has all there is, however wide it is
        (AGENT, 12, [0, 0, 0, 3]),
        // a short owner is not widened: there is nothing missing
        (Natural { owner: 2, ..AGENT }, 120, [14, 2, 8, 81]),
    ];
    for (natural, avail, want) in ROWS {
        let got = date_owner_duration_text(natural, avail);
        assert_eq!(got, want, "available {avail}");
    }
}

#[test]
fn a_text_that_fits_beside_the_full_columns_changes_nothing() {
    // 20 cells of text: far under 60% of the list, and beside the columns
    let short = Natural { text: 20, ..AGENT };
    for (avail, want) in [
        (100, [14, 16, 8, 47]),
        (90, [14, 16, 8, 37]),
        (60, [14, 3, 8, 20]),
    ] {
        let got = date_owner_duration_text(short, avail);
        assert_eq!(got, want, "available {avail}");
    }
    // no text at all: the columns stay full in a room that holds them, and
    // fit the row in one that does not (the duration and date never cut)
    let empty = Natural { text: 0, ..AGENT };
    for (avail, want) in [
        (120, [14, 16, 8, 67]),
        (40, [14, 3, 8, 0]),
        (30, [14, 0, 0, 5]),
    ] {
        let got = date_owner_duration_text(empty, avail);
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
    assert_eq!([got.date, got.owner, got.text], [14, 11, 59]);
    let got = Layout::new(plain, 2, 100);
    assert_eq!([got.date, got.owner, got.text], [14, 10, 57]);
}

#[test]
fn a_human_claim_has_no_owner_to_narrow() {
    let got = Layout::new(HUMAN, 2, 60);
    assert_eq!(
        (got.date, got.owner, got.duration, got.text),
        (14, 0, 0, 35)
    );
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
    assert_eq!((crowded.priority, crowded.date), (3, 0));
}

#[test]
fn the_owner_narrows_to_the_shortest_ending_that_tells_the_owners_apart() {
    type Row = (&'static [&'static str], usize);
    const ROWS: [Row; 13] = [
        // a lone owner is whole or gone, up to the limit of 12
        (&["agent-591a"], 10),
        (&["agent-591abc"], 12),
        // a lone owner over the limit is cut to it
        (&["agent-591abcd"], 12),
        (&["agent-very-long-name-1"], 12),
        (&["agent-591a", "agent-591a", ""], 10),
        // no owner at all
        (&["", ""], 0),
        // they differ in the last character: the ellipsis and one cell
        (&["agent-591a", "agent-591b"], 2),
        // they differ only at the front: no shortening
        (&["a-591", "b-591"], 5),
        // two of three share a longer ending
        (&["agent-a91", "agent-b91", "agent-c20"], 4),
        // one is the ending of the other: the whole one reads differently
        (&["b20", "agent-b20"], 3),
        // a long owner is told apart like any other
        (&["agent-1", "agent-2", "agent-very-long-name-1"], 4),
        // the ending that tells them apart is wider than the limit
        (&["a-agent-long-name", "b-agent-long-name"], 17),
        // two long owners that differ at the end narrow to two cells
        (&["agent-very-long-1", "agent-very-long-2"], 2),
    ];
    for (owners, want) in ROWS {
        let widest = owners.iter().map(|o| width_of(o)).max().unwrap_or(0);
        let got = owner_floor(owners.iter().copied(), widest);
        assert_eq!(got, want, "{owners:?}");
    }
}

#[test]
fn a_list_narrower_than_its_lead_has_no_text() {
    for avail in [0, 1, 3] {
        let got = Layout::new(AGENT, 2, avail);
        assert_eq!(got.text, 0, "available {avail}");
    }
}
