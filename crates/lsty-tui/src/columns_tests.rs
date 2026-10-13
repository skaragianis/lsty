use crate::columns::{Layout, Natural};

/// Natural widths, available cells, and the expected tags, date, owner,
/// duration and text widths.
type Row = (Natural, usize, [usize; 5]);

const AGENT: Natural = Natural {
    tags: 24,
    date: 14,
    owner: 16,
    took: 8,
};
/// A claim that is its duration alone: there is no owner to narrow.
const HUMAN: Natural = Natural {
    owner: 0,
    took: 3,
    ..AGENT
};

#[test]
fn the_owner_then_the_tags_narrow_by_the_cells_missing_then_columns_drop() {
    const ROWS: [Row; 11] = [
        // room: nothing narrows
        (AGENT, 120, [24, 14, 16, 8, 120 - 79]),
        // a short owner stays while nothing is missing
        (Natural { owner: 2, ..AGENT }, 120, [24, 14, 2, 8, 120 - 65]),
        // 3 cells missing: the owner gives them
        (AGENT, 86, [24, 14, 13, 8, 10]),
        // the owner at its floor
        (AGENT, 76, [24, 14, 3, 8, 10]),
        // under the floor the owner goes, the duration stays
        (AGENT, 75, [24, 14, 0, 8, 14]),
        // the owner is gone: the tags give the rest
        (AGENT, 65, [18, 14, 0, 8, 10]),
        // the tags at their floor
        (AGENT, 55, [8, 14, 0, 8, 10]),
        // the duration drops, the tags narrow again
        (AGENT, 54, [17, 14, 0, 0, 10]),
        // the date drops too
        (AGENT, 44, [23, 0, 0, 0, 10]),
        // the tags exactly at their floor
        (AGENT, 29, [8, 0, 0, 0, 10]),
        // one cell less: the tags drop too
        (AGENT, 28, [0, 0, 0, 0, 19]),
    ];
    for (natural, avail, [tags, date, owner, took, text]) in ROWS {
        let got = Layout::new(natural, 2, avail);
        let want = Layout {
            gutter: 2,
            text,
            tags,
            date,
            owner,
            took,
        };
        assert_eq!(got, want, "available {avail}");
    }
}

#[test]
fn a_human_claim_has_no_owner_to_narrow() {
    let got = Layout::new(HUMAN, 2, 60);
    assert_eq!((got.tags, got.owner, got.took, got.text), (18, 0, 3, 10));
}

#[test]
fn a_column_without_values_takes_no_room() {
    let natural = Natural {
        tags: 0,
        date: 0,
        owner: 0,
        took: 0,
    };
    let got = Layout::new(natural, 0, 40);
    assert_eq!(got.text, 40 - 6);
}
