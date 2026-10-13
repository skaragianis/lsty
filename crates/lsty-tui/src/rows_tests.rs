use lsty_core::{config::Config, theme::Slot, todotxt::Line};

use crate::{
    rows::{Inline, tag_spans, text_spans},
    text::{Span, width_of},
};

fn joined(spans: &[Span<'_>]) -> String {
    spans.iter().map(|s| s.text.as_ref()).collect()
}

/// Inline projects, inline contexts, the text and the tags column.
type Row = (bool, bool, &'static str, &'static str);

#[test]
fn the_flags_choose_which_names_stand_in_the_text() {
    const ROWS: [Row; 4] = [
        (false, false, "Call client", "+work @phone"),
        (true, false, "Call +work client", "@phone"),
        (false, true, "Call client @phone", "+work"),
        (true, true, "Call +work client @phone", ""),
    ];
    let line = Line::parse("Call +work client @phone");
    for (projects, contexts, text, tags) in ROWS {
        let config = Config {
            inline_projects: projects,
            inline_contexts: contexts,
            ..Config::default()
        };
        let inline = Inline::new(&config);
        let case = format!("projects {projects}, contexts {contexts}");
        assert_eq!(joined(&text_spans(&line, None, inline)), text, "{case}");
        let got = joined(&tag_spans(&line, None, inline));
        assert_eq!(got, tags, "{case}");
    }
}

#[test]
fn bytes_that_are_not_text_show_one_replacement_each() {
    let line = Line::parse(b"Call ag\xe2\x80 +pr\xe2\x80");
    let config = Config::default();
    let got = joined(&text_spans(&line, None, Inline::new(&config)));
    assert_eq!(got, "Call ag\u{fffd}\u{fffd}");
}

/// `show_ids`, inline projects, the line and the text it shows.
type Ids = (bool, bool, &'static str, &'static str);

#[test]
fn ids_and_deps_show_in_the_text_where_they_stand() {
    const ROWS: [Ids; 10] = [
        (true, false, "id:a Call client", "id:a Call client"),
        (true, false, "Call id:a client", "Call id:a client"),
        (true, false, "Call client dep:b", "Call client dep:b"),
        (true, false, "Call +w dep:b id:a", "Call dep:b id:a"),
        (true, true, "Call +w dep:b id:a", "Call +w dep:b id:a"),
        (false, false, "id:a Call client", "Call client"),
        (false, false, "Call dep:b client id:a", "Call client"),
        (false, true, "Call +w dep:b id:a", "Call +w"),
        (true, false, "x 2026-09-28 Call id:a", "Call id:a"),
        (false, false, "Call due:2026-09-28 pri:A", "Call"),
    ];
    for (ids, projects, raw, text) in ROWS {
        let config = Config {
            show_ids: ids,
            inline_projects: projects,
            ..Config::default()
        };
        let line = Line::parse(raw);
        let got = joined(&text_spans(&line, None, Inline::new(&config)));
        assert_eq!(got, text, "{raw} ids {ids} projects {projects}");
    }
}

#[test]
fn an_id_shows_muted_and_counts_in_the_width() {
    let line = Line::parse("Call id:a");
    let spans = text_spans(&line, None, Inline::new(&Config::default()));
    let slots: Vec<_> = spans.iter().map(|s| s.slot).collect();
    let muted = Some(Slot::Muted);
    assert_eq!(slots, [Some(Slot::Text), muted, muted]);
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
