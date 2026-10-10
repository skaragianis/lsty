use lsty_core::{config::Config, todotxt::Line};

use crate::{
    rows::{Inline, tag_spans, text_spans},
    text::Span,
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
