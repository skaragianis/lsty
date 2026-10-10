use std::fs;
use std::path::Path;

use lsty_core::config::{Config, Hidden};

use crate::field::Field;
use crate::model::Model;
use crate::prompt::matching;
use crate::test_util::{
    alone, deps, keys, press, screen, started, todo_of, workdir,
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

const MATCHES: [Row; 16] = [
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
    (&["+work.acme"], "+cme", &[]),
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
    keys(&mut model, "<Esc>");
    press(&mut model, "gt");
    assert_eq!(model.cursor_at(), Some((15, 23)));
    press(&mut model, "漢");
    assert_eq!(model.cursor_at(), Some((17, 23)));
}

#[test]
fn escape_closes_the_prompt_and_writes_nothing() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "gtx");
    keys(&mut model, "<Esc>");
    assert!(model.line.is_none());
    assert_eq!(model.mode().as_str(), "normal");
    assert_eq!(todo_of(dir.path()), LIST);
}

#[test]
fn a_selection_with_an_archived_item_opens_no_prompt() {
    let dir = workdir("Buy\n");
    fs::write(dir.path().join("done.txt"), "x 2026-09-01 Old\n")
        .unwrap_or_else(|e| panic!("{e}"));
    let config = Config {
        hidden_items: vec![Hidden::Later],
        ..alone()
    };
    let mut model = started(deps(dir.path(), config));
    press(&mut model, "2gd");
    let lines = screen(&model, 80, 24);
    assert!(model.line.is_none(), "{lines:?}");
    assert_eq!(
        model.message.as_deref(),
        Some("the selection has archived items — nothing changed")
    );
}

#[test]
fn another_key_closes_the_list_and_keeps_the_word() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "gt+w");
    keys(&mut model, "<Tab>");
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
fn the_untag_candidates_are_the_tokens_of_the_prompts_items() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "gT");
    keys(&mut model, "<Tab>");
    assert_eq!(text_of(&model), "+home");
    keys(&mut model, "<S-Tab>");
    assert_eq!(text_of(&model), "");
    keys(&mut model, "<S-Tab>");
    assert_eq!(text_of(&model), "@phone");
}

#[test]
fn nothing_completes_a_date() {
    let dir = workdir(LIST);
    let mut model = model_of(dir.path());
    press(&mut model, "gdt");
    keys(&mut model, "<Tab>");
    assert_eq!(text_of(&model), "t");
}

#[test]
fn the_tag_prompt_cuts_words_where_completion_does() {
    let dir = workdir("A\n");
    let mut model = model_of(dir.path());
    press(&mut model, "gt+a\u{a0}b");
    keys(&mut model, "<CR>");
    assert_eq!(todo_of(dir.path()), "A +a\u{a0}b\n");
}
