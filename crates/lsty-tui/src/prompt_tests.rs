use std::{fs, path::Path};

use lsty_core::{
    config::{Config, Hidden},
    theme::Slot,
};

use crate::{
    field::Field,
    model::Model,
    msg::Msg,
    prompt::matching,
    test_util::{alone, deps, keys, press, screen, started, todo_of, workdir},
};

const LIST: &str = "A +home @phone\nB +work\nC +work.acme @email\n";

fn model_of(dir: &Path) -> Model {
    started(deps(dir, alone()))
}

fn text_of(model: &Model) -> String {
    model
        .line
        .as_ref()
        .map(|l| l.field.text())
        .unwrap_or_default()
}

fn pool(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| (*w).to_owned()).collect()
}

/// The pool, the word, the candidates.
type Row = (
    &'static [&'static str],
    &'static str,
    &'static [&'static str],
);

const MATCHES: [Row; 25] = [
    (&["+ΑΣ", "+ΑΣΑ"], "+ασ", &["+ΑΣ", "+ΑΣΑ"]),
    (&["+ΑΣΑ"], "+ΑΣ", &["+ΑΣΑ"]),
    (&["+ix", "+İy"], "+İ", &["+ix", "+İy"]),
    (&["+work", "+home"], "+w", &["+work"]),
    (&["+work", "+Work"], "+w", &["+Work", "+work"]),
    (&["+work", "+work", "+w"], "+w", &["+w", "+work"]),
    (&["+home"], "+zzz", &[]),
    (&["+work.acme", "+work"], "", &["+work", "+work.acme"]),
    (&["é:", "É:", "e:"], "é", &["É:", "é:"]),
    (&["@people.russ", "@phone"], "@r", &["@people.russ"]),
    (&["+a.work", "+work", "+home"], "+w", &["+work", "+a.work"]),
    (&["+work/q3", "+work.Q4"], "+q", &["+work.Q4", "+work/q3"]),
    (&["+people.russ"], "@r", &[]),
    (&["+people.russ"], "+peo.r", &[]),
    (&["file:docs/x"], "file:x", &[]),
    (&["+work.acme"], "+cme", &["+work.acme"]),
    (
        &["+work", "+network", "+ork", "+a.ork"],
        "+ork",
        &["+ork", "+a.ork", "+network", "+work"],
    ),
    (&["@work", "+work"], "+ork", &["+work"]),
    (&["+work"], "+orks", &[]),
    (&["+work"], "+WORK!", &[]),
    (
        &["+Work", "+x.ORK", "+hork"],
        "+ORK",
        &["+x.ORK", "+Work", "+hork"],
    ),
    (&["+LSTY-rs", "+lsty"], "+lsty", &["+lsty", "+LSTY-rs"]),
    (&["+日本語", "+日本"], "+本", &["+日本", "+日本語"]),
    (&["due:", "owner:", "id:"], "ue", &["due:"]),
    (&["sort", "resort", "set"], "ort", &["resort", "sort"]),
];

#[test]
fn candidates_match_without_case_in_byte_order_once_each() {
    for (words, word, want) in MATCHES {
        assert_eq!(matching(pool(words), word), pool(want), "{word}");
    }
}

#[test]
fn the_word_under_the_cursor_is_found_and_replaced() {
    let mut field = Field::new("+work +h more");
    for _ in 0..4 {
        field.act(lsty_core::keys::Action::CursorLeft);
    }
    let (from, to) = field.word_span();
    assert_eq!(field.slice(from, to), "more");
    field.replace(from, to, "+home");
    assert_eq!(field.text(), "+work +h +home");
    assert_eq!(field.word_span(), (9, 14));
}

#[test]
fn the_cursor_sits_after_the_label_and_the_text() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    assert_eq!(model.cursor_at(), None);
    press(&mut model, "gd");
    assert_eq!(model.cursor_at(), Some((15, 23)));
    press(&mut model, "fri");
    assert_eq!(model.cursor_at(), Some((18, 23)));
    keys(&mut model, "<Esc>gt");
    assert_eq!(model.cursor_at(), Some((15, 23)));
    press(&mut model, "漢");
    assert_eq!(model.cursor_at(), Some((17, 23)));
}

#[test]
fn escape_closes_the_prompt_and_writes_nothing() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    keys(&mut model, "gtx<Esc>");
    assert!(model.line.is_none());
    assert_eq!(model.mode().as_str(), "normal");
    assert_eq!(todo_of(dir.path()), LIST);
}

#[test]
fn a_selection_with_an_archived_task_opens_no_prompt() {
    let dir = workdir("Buy\n");
    fs::write(dir.path().join("done.txt"), "x 2026-09-01 Old\n")
        .unwrap_or_else(|e| panic!("{e}"));
    let config = Config {
        hidden_tasks: vec![Hidden::Later],
        ..alone()
    };
    let mut model = started(deps(dir.path(), config));
    press(&mut model, "2gd");
    let lines = screen(&model, 80, 24);
    assert!(model.line.is_none(), "{lines:?}");
    assert_eq!(
        model.message.as_deref(),
        Some("the selection has archived tasks — nothing changed")
    );
}

#[test]
fn another_key_closes_the_list_and_keeps_the_word() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    keys(&mut model, "gt+w<Tab>");
    assert_eq!(text_of(&model), "+work");
    press(&mut model, "x");
    assert_eq!(text_of(&model), "+workx");
    let lines = screen(&model, 80, 24);
    assert!(
        lines.get(22).is_some_and(|l| !l.contains("+work.acme")),
        "{lines:?}"
    );
}

#[test]
fn the_untag_candidates_are_the_tokens_of_the_prompts_tasks() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    keys(&mut model, "gT<Tab>");
    assert_eq!(text_of(&model), "+home");
    keys(&mut model, "<S-Tab>");
    assert_eq!(text_of(&model), "");
    keys(&mut model, "<S-Tab>");
    assert_eq!(text_of(&model), "@phone");
}

#[test]
fn the_prompts_know_a_name_before_its_punctuation() {
    let dir = workdir("Ship it for +work.\n");
    let mut model = model_of(dir.path());
    keys(&mut model, "gT<Tab>");
    assert_eq!(text_of(&model), "+work");
    keys(&mut model, "<Esc>");
    keys(&mut model, "gt+work<CR>");
    assert_eq!(todo_of(dir.path()), "Ship it for +work.\n");
    keys(&mut model, "gT+work<CR>");
    assert_eq!(todo_of(dir.path()), "Ship it for\n");
}

#[test]
fn nothing_completes_a_date() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    keys(&mut model, "gdt<Tab>");
    assert_eq!(text_of(&model), "t");
}

#[test]
fn the_tag_prompt_cuts_words_where_completion_does() {
    let dir = workdir("A\n");
    let mut model = model_of(dir.path());
    keys(&mut model, "gt+a\u{a0}b<CR>");
    assert_eq!(todo_of(dir.path()), "A +a\u{a0}b\n");
}

#[test]
fn the_tag_prompt_keeps_a_no_break_space_at_the_edge_of_its_text() {
    let dir = workdir("A\n");
    let mut model = model_of(dir.path());
    keys(&mut model, "gt+a\u{a0}<CR>");
    assert_eq!(todo_of(dir.path()), "A +a\u{a0}\n");
}

/// The candidates the prompt offers after `tabs` presses of the tab key
/// with the five names `+aa` to `+ee` to pick from, in `w` cells.
fn offered(tabs: usize, w: usize) -> Vec<(String, Option<Slot>)> {
    let dir = workdir("A +aa +bb +cc +dd +ee\n");
    let mut model = model_of(dir.path());
    keys(&mut model, &format!("gt+{}", "<Tab>".repeat(tabs)));
    let comp = model.line.as_ref().and_then(|l| l.comp.as_ref());
    let spans = comp.map(|c| c.spans(w)).unwrap_or_default();
    spans
        .into_iter()
        .map(|s| (s.text.into_owned(), s.slot))
        .collect()
}

#[test]
fn the_candidate_in_the_text_is_marked_among_those_offered() {
    let (text, selected) = (Some(Slot::Text), Some(Slot::Selection));
    let gap = || ("  ".to_owned(), None);
    let name = |n: &str, slot| (n.to_owned(), slot);
    let want = vec![
        name("+aa", text),
        gap(),
        name("+bb", text),
        gap(),
        name("+cc", selected),
        gap(),
        name("+dd", text),
        gap(),
        name("+ee", text),
    ];
    assert_eq!(offered(3, 80), want);
}

#[test]
fn the_candidates_offered_start_where_the_one_in_the_text_still_fits() {
    // Names are three cells wide, with two cells between them; the line
    // keeps one cell for an ellipsis unless the last one is in the text.
    for (tabs, w, first) in [
        (5, 13, "+cc"),
        (5, 12, "+dd"),
        (3, 14, "+aa"),
        (3, 13, "+bb"),
        (1, 3, "+aa"),
    ] {
        let shown = offered(tabs, w);
        let head = shown.first().map(|(t, _)| t.as_str());
        assert_eq!(head, Some(first), "{tabs} {w}");
    }
}

/// The message after `keys` over the tasks `A` and `B` in `w` cells.
fn said(w: u16, keys_typed: &str) -> Option<String> {
    let dir = workdir("A\nB\n");
    let mut model = model_of(dir.path());
    model.update(Msg::Resize(w, 24));
    keys(&mut model, keys_typed);
    model.message
}

#[test]
fn many_tasks_are_counted_when_their_messages_do_not_fit_the_status_line() {
    // The message is 24 cells; the status line keeps 25 and two spare.
    let message = "1 tagged +x; 2 tagged +x";
    assert_eq!(said(51, "Vjgt+x<CR>").as_deref(), Some(message));
    assert_eq!(said(50, "Vjgt+x<CR>").as_deref(), Some("2 tasks changed"));
}

#[test]
fn one_task_keeps_its_message_however_long() {
    let name = format!("+{}", "x".repeat(60));
    let said = said(40, &format!("gt{name}<CR>"));
    assert_eq!(said, Some(format!("1 tagged {name}")));
}

/// The keys typed in the search, and its text after them.
const SEARCH_COMPLETES: [(&str, &str); 11] = [
    ("/+w<Tab>", "+work"),
    ("/+w<Tab><Tab>", "+work.acme"),
    ("/@e<Tab>", "@email"),
    ("/fix +h<Tab>", "fix +home"),
    ("/is:do<Tab>", "is:doing"),
    ("/ope<Tab>", "is:open"),
    ("/-+h<Tab>", "-+home"),
    ("/-<Tab>", "-is:archived"),
    ("/+home|+w<Tab>", "+home|+work"),
    ("/is:done|is:la<Tab>", "is:done|is:later"),
    ("/+h|+w<Left><Left><Left><Tab>", "+home|+w"),
];

#[test]
fn the_search_completes_the_alternative_under_the_cursor() {
    let dir = workdir(LIST);
    for (typed, want) in SEARCH_COMPLETES {
        let mut model = model_of(dir.path());
        keys(&mut model, typed);
        assert_eq!(text_of(&model), want, "{typed}");
    }
}
