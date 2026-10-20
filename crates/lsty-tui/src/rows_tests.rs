use lsty_core::{config::Config, theme::Slot, todotxt::Line};

use crate::{
    rows::{Inline, base_slot, marker, text_spans},
    test_util::{alone, snapshot, workdir_model},
    text::{Span, width_of},
};

fn joined(spans: &[Span<'_>]) -> String {
    spans.iter().map(|s| s.text.as_ref()).collect()
}

#[test]
fn bytes_that_are_not_text_show_one_replacement_each() {
    let line = Line::parse(b"Call ag\xe2\x80 +pr\xe2\x80");
    let config = Config::default();
    let got = joined(&text_spans(&line, None, Inline::new(&config)));
    assert_eq!(got, "Call ag\u{fffd}\u{fffd} +pr\u{fffd}\u{fffd}");
}

/// `show_ids`, the line and the text it shows.
type Ids = (bool, &'static str, &'static str);

#[test]
fn deps_show_in_the_text_where_they_stand_and_ids_never_do() {
    const ROWS: [Ids; 10] = [
        (true, "id:a Call client", "Call client"),
        (true, "Call id:a client", "Call client"),
        (true, "Call client dep:b", "Call client dep:b"),
        (true, "Call +w dep:b id:a", "Call +w dep:b"),
        (false, "id:a Call client", "Call client"),
        (false, "Call dep:b client id:a", "Call client"),
        (false, "Call +w dep:b id:a", "Call +w"),
        (true, "x 2026-09-28 Call id:a", "Call"),
        (false, "Call due:2026-09-28 pri:A", "Call"),
        // projects and contexts stay where they were written
        (true, "Call +work client @phone", "Call +work client @phone"),
    ];
    for (ids, raw, text) in ROWS {
        let config = Config {
            show_ids: ids,
            ..Config::default()
        };
        let line = Line::parse(raw);
        let got = joined(&text_spans(&line, None, Inline::new(&config)));
        assert_eq!(got, text, "{raw} ids {ids}");
    }
}

#[test]
fn a_tag_shows_muted_and_counts_in_the_width() {
    let line = Line::parse("Call dep:a");
    let spans = text_spans(&line, None, Inline::new(&Config::default()));
    let slots: Vec<_> = spans.iter().map(|s| s.slot).collect();
    let muted = Some(Slot::Muted);
    assert_eq!(slots, [Some(Slot::Text), Some(Slot::Text), muted]);
    assert_eq!(spans.iter().map(|s| width_of(&s.text)).sum::<usize>(), 10);
}

#[test]
fn a_url_shows_in_the_link_slot_unless_it_is_code() {
    let line =
        Line::parse("See https://x.org/a, `ftp://h` file:b.md mailto:a@b");
    let inline = Inline::new(&Config::default());
    let got: Vec<(String, Option<Slot>)> = text_spans(&line, None, inline)
        .iter()
        .filter(|s| s.text.as_ref() != " ")
        .map(|s| (s.text.to_string(), s.slot))
        .collect();
    let want = [
        ("See", Slot::Text),
        ("https://x.org/a,", Slot::Link),
        ("`ftp://h`", Slot::Code),
        ("file:b.md", Slot::Link),
        ("mailto:a@b", Slot::Muted),
    ]
    .map(|(text, slot)| (text.to_owned(), Some(slot)));
    assert_eq!(got, want);
}

#[test]
fn the_blank_between_two_links_is_not_a_link() {
    let line = Line::parse("https://x.org file:a.md");
    let spans = text_spans(&line, None, Inline::new(&Config::default()));
    let slots: Vec<_> = spans.iter().map(|s| s.slot).collect();
    let (url, text) = (Some(Slot::Link), Some(Slot::Text));
    assert_eq!(slots, [url, text, url]);
}

#[test]
fn a_row_counts_the_blanks_between_its_words_in_its_width() {
    let (_dir, model) = workdir_model("ab cd +ef\n", alone());
    assert_eq!(model.rows[0].text_width, 9);
}

#[test]
fn the_marker_tells_a_human_claim_from_an_agent_claim() {
    let todo = "Free\n\
        Human started:2026-09-29T0700Z\n\
        Agent started:2026-09-29T0700Z owner:agent1\n\
        x 2026-09-29 Done started:2026-09-29T0700Z owner:agent1\n\
        Open id:a\n\
        Blocked dep:a\n";
    let snap = snapshot(todo, "", "2026-09-29T0900Z");
    let got: Vec<_> = snap
        .tasks()
        .iter()
        .map(|task| {
            let span = marker(task, base_slot(task));
            (span.text.to_string(), span.slot)
        })
        .collect();
    let want = [
        (" ", None),
        ("▷", Some(Slot::Doing)),
        ("▶", Some(Slot::Doing)),
        ("✓", Some(Slot::Done)),
        (" ", None),
        ("⧗", Some(Slot::Blocked)),
    ]
    .map(|(text, slot)| (text.to_owned(), slot));
    assert_eq!(got, want);
}

/// The id cell and the text of each row.
type IdRows = Vec<(String, Option<Slot>, String)>;

/// The width of the id column and the rows of a model of `todo`.
fn id_column(todo: &str, show_ids: bool) -> (usize, IdRows) {
    let config = Config {
        show_ids,
        ..alone()
    };
    let inline = Inline::new(&config);
    let (_dir, model) = workdir_model(todo, config);
    let rows = model.rows.iter().map(|row| {
        let task = model.snap.task(row.id).unwrap_or_else(|| panic!("task"));
        let text = joined(&text_spans(model.snap.line(task), None, inline));
        (row.id_cell.text.clone(), row.id_cell.slot, text)
    });
    (model.natural.id, rows.collect())
}

#[test]
fn an_id_shows_in_the_column_in_the_slot_of_its_row() {
    let todo = "A id:a1\nB id:b2 dep:a1\nx 2026-09-29 C id:c3\nD dep:a1\n";
    let (width, rows) = id_column(todo, true);
    assert_eq!(width, 2);
    let (id, blocked, done) =
        (Some(Slot::Id), Some(Slot::Blocked), Some(Slot::Done));
    let want = [
        ("a1", id, "A"),
        ("b2", blocked, "B dep:a1"),
        ("c3", done, "C"),
        ("", None, "D dep:a1"),
    ]
    .map(|(id, slot, text)| (id.to_owned(), slot, text.to_owned()));
    assert_eq!(rows, want);
}

#[test]
fn the_id_column_is_absent_when_no_row_has_an_id() {
    let (width, rows) = id_column("A dep:a1\nB\n", true);
    assert_eq!(width, 0);
    assert_eq!(rows[0], (String::new(), None, "A dep:a1".to_owned()));
}

#[test]
fn a_long_id_is_as_wide_as_the_greatest_id_column() {
    let (width, rows) = id_column("A id:abcdefghijkl\nB id:ab\n", true);
    assert_eq!(width, 8);
    assert_eq!(rows[0].0, "abcdefghijkl");
}

#[test]
fn no_id_column_and_no_id_in_the_text_with_show_ids_off() {
    let (width, rows) = id_column("A id:a1 dep:b2\n", false);
    assert_eq!(width, 0);
    assert_eq!(rows[0], (String::new(), None, "A".to_owned()));
}
