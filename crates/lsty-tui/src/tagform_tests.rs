use lsty_core::config::Config;

use crate::{
    tagform::forms,
    test_util::{alone, deps, snapshot, started, workdir, workdir_model},
};

/// The width and least width of each entry of each row of `todo`.
fn tags_of(todo: &str) -> Vec<Vec<(usize, usize)>> {
    let (_dir, model) = workdir_model(todo, alone());
    model.rows.iter().map(|r| r.tags.clone()).collect()
}

#[test]
fn an_entry_shortens_to_the_start_no_other_entry_shares() {
    let got = tags_of(
        "a +alpha +alphabet\nb +beta @beta\nc +bug +test +testing\nd +solo +ab +ac\n",
    );
    let want = [
        // the start of another stays whole; the other needs one more cell
        vec![(6, 6), (9, 8)],
        // the sign tells +beta and @beta apart
        vec![(5, 4), (5, 3)],
        // +bu… is not narrower than +bug, nor +a… than +ab
        vec![(4, 4), (5, 5), (8, 7)],
        // alone: its sign and one cell
        vec![(5, 3), (3, 3), (3, 3)],
    ];
    assert_eq!(got, want);
}

#[test]
fn a_wide_character_is_counted_in_cells_and_never_split() {
    let got = tags_of("a +abcdef日xy +abcdef本xy\n");
    assert_eq!(got, [vec![(11, 10), (11, 10)]]);
}

#[test]
fn the_column_width_is_the_widest_row_at_each_limit() {
    let (_dir, model) =
        workdir_model("a +apple @phone\nb +apricot @phone\n", alone());
    let at = model.natural.tags_at;
    // the limit 1 to 9 cells: nothing goes under the least forms
    assert_eq!(at.get(..9), Some(&[9, 9, 9, 10, 11, 13, 14, 15, 15][..]));
    assert_eq!(at.last(), Some(&15));
}

#[test]
fn an_entry_of_the_text_stays_out_of_the_column() {
    let config = Config {
        inline_projects: true,
        ..alone()
    };
    let (_dir, model) = workdir_model("a +alpha @home\nb +alphabet\n", config);
    let got: Vec<_> = model.rows.iter().map(|r| r.tags.clone()).collect();
    assert_eq!(got, [vec![(5, 3)], vec![]]);
}

#[test]
fn archived_names_tell_entries_apart_only_while_the_archive_is_shown() {
    let snap = snapshot("a +alpha\n", "x +alphabet\n", "2026-10-07T1000Z");
    let at = |archived| {
        let forms = forms(&snap, archived);
        (
            forms.get(&b"+alpha"[..]).copied(),
            forms.get(&b"+alphabet"[..]).copied(),
        )
    };
    // alone in the todo file, +alpha shortens
    assert_eq!(at(false), (Some((6, 3)), None));
    // beside +alphabet it is the start of another and stays whole
    assert_eq!(at(true), (Some((6, 6)), Some((9, 8))));
}

#[test]
fn a_name_that_is_not_valid_text_is_never_shortened() {
    // the bad byte is three bytes long once it is text
    let dir = workdir(b"a +\xe9abcx\nb +\xe9abdy\n".as_slice());
    let model = started(deps(dir.path(), alone()));
    let got: Vec<_> = model.rows.iter().map(|r| r.tags.clone()).collect();
    assert_eq!(got, [vec![(6, 6)], vec![(6, 6)]]);
}
