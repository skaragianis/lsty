use crate::columns::{Layout, Natural};

/// Natural widths, available cells, and the expected tags, date, claim
/// and text widths.
type Row = (Natural, usize, [usize; 4]);

#[test]
fn columns_are_dropped_from_the_right_until_the_text_has_room() {
    const WIDE: Natural = Natural {
        tags: 24,
        date: 14,
        claim: 16,
    };
    // (natural widths, available cells, expected tags, date, claim, text)
    const ROWS: [Row; 4] = [
        (WIDE, 120, [24, 14, 16, 120 - 9 - 26 - 16 - 18]),
        (WIDE, 76, [24, 14, 0, 76 - 9 - 26 - 16]),
        (WIDE, 50, [24, 0, 0, 50 - 9 - 26]),
        (WIDE, 30, [0, 0, 0, 30 - 9]),
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
    };
    let got = Layout::new(natural, 0, 40);
    assert_eq!(got.text, 40 - 6);
}
