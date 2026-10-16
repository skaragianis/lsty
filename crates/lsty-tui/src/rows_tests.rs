use lsty_core::{config::Config, theme::Slot, todotxt::Line};

use crate::{
    rows::{Inline, text_spans},
    test_util::{alone, workdir_model},
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
fn ids_and_deps_show_in_the_text_where_they_stand() {
    const ROWS: [Ids; 10] = [
        (true, "id:a Call client", "id:a Call client"),
        (true, "Call id:a client", "Call id:a client"),
        (true, "Call client dep:b", "Call client dep:b"),
        (true, "Call +w dep:b id:a", "Call +w dep:b id:a"),
        (false, "id:a Call client", "Call client"),
        (false, "Call dep:b client id:a", "Call client"),
        (false, "Call +w dep:b id:a", "Call +w"),
        (true, "x 2026-09-28 Call id:a", "Call id:a"),
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
fn an_id_shows_muted_and_counts_in_the_width() {
    let line = Line::parse("Call id:a");
    let spans = text_spans(&line, None, Inline::new(&Config::default()));
    let slots: Vec<_> = spans.iter().map(|s| s.slot).collect();
    let muted = Some(Slot::Muted);
    assert_eq!(slots, [Some(Slot::Text), Some(Slot::Text), muted]);
    assert_eq!(spans.iter().map(|s| width_of(&s.text)).sum::<usize>(), 9);
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
