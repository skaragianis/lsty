use crate::columns::{Layout, Natural};

/// Natural widths, available cells, and the expected tags, date, claim
/// and text widths.
type Row = (Natural, usize, [usize; 4]);

const AGENT: Natural = Natural {
    tags: 24,
    date: 14,
    claim: 16,
    duration: 3,
};
/// A claim that is its duration alone: nothing in it narrows.
const HUMAN: Natural = Natural { claim: 3, ..AGENT };

#[test]
fn the_claim_then_the_tags_narrow_by_the_cells_missing_then_columns_drop() {
    const ROWS: [Row; 8] = [
        // room: nothing narrows
        (AGENT, 120, [24, 14, 16, 120 - 9 - 26 - 16 - 18]),
        // 3 cells missing: the claim gives them
        (AGENT, 76, [24, 14, 13, 10]),
        // the claim is down to its duration, the tags are whole
        (AGENT, 66, [24, 14, 3, 10]),
        // the claim at its floor, the tags give the rest
        (AGENT, 60, [18, 14, 3, 10]),
        // the claim drops, the date stays, the tags narrow
        (AGENT, 48, [11, 14, 0, 10]),
        // the date drops too, the tags exactly at their floor
        (AGENT, 29, [8, 0, 0, 10]),
        // one cell less: the tags drop too
        (AGENT, 28, [0, 0, 0, 19]),
        // no owner to cut: straight to the tags
        (HUMAN, 55, [13, 14, 3, 10]),
    ];
    for (natural, avail, [tags, date, claim, text]) in ROWS {
        let got = Layout::new(natural, 2, avail);
        let want = Layout {
            gutter: 2,
            text,
            tags,
            date,
            claim,
        };
        assert_eq!(got, want, "available {avail}");
    }
}

#[test]
fn a_column_without_values_takes_no_room() {
    let natural = Natural {
        tags: 0,
        date: 0,
        claim: 0,
        duration: 0,
    };
    let got = Layout::new(natural, 0, 40);
    assert_eq!(got.text, 40 - 6);
}
