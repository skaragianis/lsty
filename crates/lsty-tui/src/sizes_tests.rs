use crate::sizes::Sizes;

/// Entries of 1, 3, 2, 1, 3, 1 rows in a pane of 5 rows.
fn sized<R>(f: impl FnOnce(Sizes<'_>) -> R) -> R {
    const HEIGHTS: [usize; 6] = [1, 3, 2, 1, 3, 1];
    f(Sizes {
        len: 6,
        rows: 5,
        heights: Some(&|i| HEIGHTS[i]),
    })
}

#[test]
fn entries_of_mixed_heights_fill_the_pane_by_whole_entries() {
    // (top, at, window): the view moves the least that shows `at` whole.
    const WINDOWS: [(usize, usize, usize); 6] = [
        (0, 2, 1),
        (0, 4, 3),
        (3, 1, 1),
        (2, 5, 3),
        (0, 0, 0),
        (5, 3, 3),
    ];
    for (top, at, want) in WINDOWS {
        assert_eq!(sized(|s| s.window(top, at)), want, "{top} {at}");
    }
    assert_eq!(sized(|s| s.last_top()), 3);
    assert_eq!(sized(|s| s.scrolled(1, 9)), 3);
    assert_eq!(sized(|s| s.scrolled(1, -9)), 0);
}

#[test]
fn entries_count_whole_when_they_are_in_view() {
    // whole entries from the top: 1+3 then 2 does not fit; 3 alone.
    assert_eq!(sized(|s| s.whole(0)), 2);
    assert_eq!(sized(|s| s.whole(4)), 2);
    assert!(sized(|s| s.seen(3, 4)));
    assert!(!sized(|s| s.seen(4, 3)));
    assert!(!sized(|s| s.seen(2, 4)));
}

#[test]
fn entries_reach_as_far_as_the_cells_go_and_one_at_least() {
    // (from, cells, down, entries)
    const REACH: [(usize, usize, bool, usize); 6] = [
        (0, 4, true, 2),
        (0, 3, true, 1),
        (1, 1, true, 1),
        (4, 6, false, 3),
        (4, 3, false, 2),
        (0, 5, false, 1),
    ];
    for (from, cells, down, want) in REACH {
        let got = sized(|s| s.reach(from, cells, down));
        assert_eq!(got, want, "{from} {cells} {down}");
    }
    assert_eq!(sized(|s| s.above(4, 3)), 2);
    assert_eq!(sized(|s| s.above(4, 1)), 3);
}

#[test]
fn the_entries_shown_are_cut_by_the_bottom() {
    assert_eq!(sized(|s| s.shown(0)), [(0, 1), (1, 3), (2, 1)]);
    assert_eq!(sized(|s| s.shown(3)), [(3, 1), (4, 3), (5, 1)]);
}
